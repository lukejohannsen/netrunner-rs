//! Trace bidding resolution: the Corp commits a credit bid on top of a
//! trace's base strength, then the Runner commits a credit bid (added to
//! `RunnerState::link_strength`) trying to meet or beat it. See
//! `state::TraceState`'s doc comment for why this can't be a normal
//! synchronously-resolved `Effect`, and `ability::resolve_unbroken_subroutines`
//! for how a trace fired mid-subroutine-loop suspends that loop until this
//! module resolves it.

use crate::dsl::Cost;
use crate::rules::ability;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::paid_ability;
use crate::rules::state::{GameState, Side, TraceResume};

/// Resolves `PlayerAction::SubmitCorpTraceBid`. Requires an active trace
/// still awaiting the Corp's bid (`RulesError::TraceNotAwaitingCorpBid`
/// otherwise); deducts `amount` via the same `pay_cost` path every other
/// credit cost in this engine uses, so an unaffordable bid fails with the
/// generic `RulesError::NotEnoughCredits` rather than a bespoke variant.
pub(crate) fn submit_corp_bid(state: &mut GameState, amount: u32) -> Result<Vec<GameEvent>, RulesError> {
    let trace = state.active_trace.as_ref().ok_or(RulesError::TraceNotAwaitingCorpBid)?;
    if trace.corp_bid.is_some() {
        return Err(RulesError::TraceNotAwaitingCorpBid);
    }

    let mut events = ability::pay_cost(state, Side::Corp, &Cost::Credits(amount), None)?;

    let trace = state.active_trace.as_mut().expect("checked Some above");
    trace.corp_bid = Some(amount);
    let total_strength = trace.base_strength.saturating_add(amount);
    events.push(GameEvent::TraceCorpBidSubmitted { corp_bid: amount, total_strength });
    Ok(events)
}

/// Resolves `PlayerAction::SubmitRunnerTraceBid`. Requires an active trace
/// with the Corp's bid already submitted (`RulesError::TraceNotAwaitingRunnerBid`
/// otherwise). Deducts `amount` the same way as the Corp's bid, then
/// compares totals: ties favor the Runner (`>=` avoids the trace). On a
/// successful trace, evaluates `effect_on_success` with the trace's
/// `initiating_card` as context. If this trace suspended
/// `ability::resolve_unbroken_subroutines` mid-loop (`resume ==
/// ResumeSubroutines`), resumes it via `paid_ability::resolve_encounter_ice`
/// after resolving — whether avoided or not — so any remaining subroutines
/// on the ICE still fire and the run still advances.
pub(crate) fn submit_runner_bid(state: &mut GameState, amount: u32) -> Result<Vec<GameEvent>, RulesError> {
    let trace = state.active_trace.as_ref().ok_or(RulesError::TraceNotAwaitingRunnerBid)?;
    if trace.corp_bid.is_none() {
        return Err(RulesError::TraceNotAwaitingRunnerBid);
    }

    let mut events = ability::pay_cost(state, Side::Runner, &Cost::Credits(amount), None)?;

    let trace = state.active_trace.take().expect("checked Some above");
    let corp_total = trace.base_strength.saturating_add(trace.corp_bid.expect("checked Some above"));
    let runner_total = state.runner.link_strength.saturating_add(amount);
    events.push(GameEvent::TraceRunnerBidSubmitted { runner_bid: amount, total_strength: runner_total });

    if runner_total >= corp_total {
        events.push(GameEvent::TraceAvoided { corp_total, runner_total });
    } else {
        events.push(GameEvent::TraceSuccessful { corp_total, runner_total });
        events.extend(ability::evaluate_effect(state, &trace.effect_on_success, trace.initiating_card.as_ref())?);
    }

    if trace.resume == TraceResume::ResumeSubroutines {
        events.extend(paid_ability::resolve_encounter_ice(state)?);
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardId, Effect};
    use crate::rules::event::GameEvent;
    use crate::rules::run::{EncounteredSubroutine, RunIce, RunPhase, RunState, ServerId, SubroutineStatus};
    use crate::rules::state::{
        AgendaPoints, Clicks, Credits, CorpState, GamePhase, MemoryUnits, PlayerResources, RunnerState,
        TraceState,
    };

    fn game_state() -> GameState {
        GameState {
            corp: CorpState { identity: None, bad_publicity: 0,
                scored_agendas: Vec::new(),
                resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
                hq: Vec::new(),
                r_and_d: Vec::new(),
                archives: Vec::new(),
                installed: Vec::new(),
            },
            runner: RunnerState { identity: None,
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
            },
            phase: GamePhase::Action(Side::Runner),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            seed: 0,
            rng_step: 0,
        }
    }

    fn active_trace(base_strength: u32, corp_bid: Option<u32>, on_success: Effect) -> TraceState {
        TraceState {
            initiating_card: None,
            base_strength,
            corp_bid,
            effect_on_success: on_success,
            resume: TraceResume::None,
        }
    }

    #[test]
    fn exact_match_avoids_the_trace() {
        let mut state = game_state();
        state.active_trace = Some(active_trace(3, Some(2), Effect::GiveTags(1)));
        state.runner.link_strength = 2;

        let events = submit_runner_bid(&mut state, 3).unwrap();

        assert!(state.active_trace.is_none());
        assert_eq!(state.runner.tags, 0, "on_success must not fire when avoided");
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 3 },
                GameEvent::TraceRunnerBidSubmitted { runner_bid: 3, total_strength: 5 },
                GameEvent::TraceAvoided { corp_total: 5, runner_total: 5 },
            ]
        );
    }

    #[test]
    fn runner_total_below_corp_total_fires_effect_on_success() {
        let mut state = game_state();
        state.active_trace = Some(active_trace(3, Some(2), Effect::GiveTags(1)));
        state.runner.link_strength = 1;

        let events = submit_runner_bid(&mut state, 3).unwrap();

        assert!(state.active_trace.is_none());
        assert_eq!(state.runner.tags, 1);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 3 },
                GameEvent::TraceRunnerBidSubmitted { runner_bid: 3, total_strength: 4 },
                GameEvent::TraceSuccessful { corp_total: 5, runner_total: 4 },
                GameEvent::TagsGiven { side: Side::Runner, amount: 1 },
            ]
        );
    }

    #[test]
    fn zero_bid_both_sides_ties_and_is_avoided() {
        let mut state = game_state();
        state.active_trace = Some(active_trace(0, Some(0), Effect::GiveTags(9)));

        let events = submit_runner_bid(&mut state, 0).unwrap();

        assert!(state.active_trace.is_none());
        assert_eq!(state.runner.tags, 0);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 0 },
                GameEvent::TraceRunnerBidSubmitted { runner_bid: 0, total_strength: 0 },
                GameEvent::TraceAvoided { corp_total: 0, runner_total: 0 },
            ]
        );
    }

    #[test]
    fn corp_bid_with_insufficient_credits_errors_and_leaves_state_untouched() {
        let mut state = game_state();
        state.active_trace = Some(active_trace(2, None, Effect::GiveTags(1)));

        let result = submit_corp_bid(&mut state, 10);

        assert_eq!(result, Err(RulesError::NotEnoughCredits { side: Side::Corp, available: 5, requested: 10 }));
        assert_eq!(state.corp.resources.credits, Credits(5));
        assert_eq!(state.active_trace.unwrap().corp_bid, None);
    }

    #[test]
    fn runner_bid_with_insufficient_credits_errors_and_leaves_trace_pending() {
        let mut state = game_state();
        state.active_trace = Some(active_trace(2, Some(1), Effect::GiveTags(1)));

        let result = submit_runner_bid(&mut state, 10);

        assert_eq!(result, Err(RulesError::NotEnoughCredits { side: Side::Runner, available: 5, requested: 10 }));
        assert_eq!(state.runner.resources.credits, Credits(5));
        assert!(state.active_trace.is_some(), "trace must stay pending — pay_cost errors before the take()");
    }

    #[test]
    fn runner_bid_before_corp_bid_errors() {
        let mut state = game_state();
        state.active_trace = Some(active_trace(2, None, Effect::GiveTags(1)));

        assert_eq!(submit_runner_bid(&mut state, 0), Err(RulesError::TraceNotAwaitingRunnerBid));
    }

    #[test]
    fn corp_bid_when_no_trace_is_active_errors() {
        let mut state = game_state();
        assert_eq!(submit_corp_bid(&mut state, 0), Err(RulesError::TraceNotAwaitingCorpBid));
    }

    fn ice_with_trace_pending_resume(remaining_effect: Effect, on_success: Effect) -> RunState {
        RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce {
                card_id: CardId("ice_wall".to_string()),
                current_strength: 0,
                ice_type: crate::dsl::IceType::Barrier,
                subroutines: vec![
                    EncounteredSubroutine {
                        id: 0,
                        definition: crate::dsl::SubroutineDef {
                            text: "trace".to_string(),
                            effect: Effect::Trace { base: 2, on_success: Box::new(on_success) },
                        },
                        status: SubroutineStatus::Resolved,
                    },
                    EncounteredSubroutine {
                        id: 1,
                        definition: crate::dsl::SubroutineDef { text: "remaining".to_string(), effect: remaining_effect },
                        status: SubroutineStatus::Pending,
                    },
                ],
                rezzed: true,
            }],
            position: 0,
            access_state: None,
            jack_out_permitted: false,
        }
    }

    #[test]
    fn nested_subroutine_trace_resumes_remaining_subroutines_after_avoidance() {
        let mut state = game_state();
        state.active_run = Some(ice_with_trace_pending_resume(Effect::GiveTags(3), Effect::EndTheRun));
        state.active_trace = Some(TraceState {
            initiating_card: None,
            base_strength: 2,
            corp_bid: Some(0),
            effect_on_success: Effect::EndTheRun,
            resume: TraceResume::ResumeSubroutines,
        });
        state.runner.link_strength = 5;

        submit_runner_bid(&mut state, 0).unwrap();

        assert_eq!(state.runner.tags, 3, "remaining subroutine should have fired after resume");
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::Success, "run should advance past the ICE");
    }

    #[test]
    fn nested_subroutine_trace_resumes_remaining_subroutines_after_success() {
        let mut state = game_state();
        state.active_run = Some(ice_with_trace_pending_resume(Effect::GiveTags(3), Effect::EndTheRun));
        state.active_trace = Some(TraceState {
            initiating_card: None,
            base_strength: 5,
            corp_bid: Some(0),
            effect_on_success: Effect::EndTheRun,
            resume: TraceResume::ResumeSubroutines,
        });
        state.runner.link_strength = 0;

        submit_runner_bid(&mut state, 0).unwrap();

        assert!(state.active_run.is_none(), "EndTheRun should have fired and ended the run");
        assert_eq!(state.runner.tags, 0, "remaining subroutine must never fire once the run ended");
    }
}
