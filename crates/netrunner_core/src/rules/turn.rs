use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::state::{Clicks, GameState, Side};

/// Clicks the Corp receives at the start of each turn. A base turn-structure
/// constant of the game, not a card rule — same category as the "1 click"
/// cost every basic action already hardcodes.
const CORP_CLICKS_PER_TURN: u32 = 3;
/// Clicks the Runner receives at the start of each turn.
const RUNNER_CLICKS_PER_TURN: u32 = 4;

/// End the active side's turn: hand control to the other side and refill
/// their clicks to their fixed per-turn allotment.
///
/// Deliberately NOT modeled: mandatory draw, end-of-turn hand-size
/// discard/cleanup, and start/end-of-turn card triggers (needs the `dsl`
/// trigger system wired into the engine, which doesn't happen yet).
///
/// Credits are untouched — they carry over turn to turn. The ending side's
/// own stale clicks are also left untouched rather than zeroed: every
/// click-spending action is already gated by `require_active_turn`, so
/// leftover clicks are inert until that side's own next `end_turn`.
pub fn end_turn(state: &GameState) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    if state.active_run.is_some() {
        return Err(RulesError::CannotEndTurnWhileRunActive);
    }

    let ending_side = state.active_turn;
    let next_side = match ending_side {
        Side::Corp => Side::Runner,
        Side::Runner => Side::Corp,
    };
    let clicks = match next_side {
        Side::Corp => CORP_CLICKS_PER_TURN,
        Side::Runner => RUNNER_CLICKS_PER_TURN,
    };

    let mut next = state.clone();
    next.active_turn = next_side;
    next.resources_mut(next_side).clicks = Clicks(clicks);

    Ok((
        next,
        vec![
            GameEvent::TurnEnded { side: ending_side },
            GameEvent::TurnStarted { side: next_side, clicks },
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::run::{RunPhase, RunState, ServerId};
    use crate::rules::state::{
        AgendaPoints, CorpState, Credits, MemoryUnits, PlayerResources, RunnerState,
    };

    fn game_state(
        active_turn: Side,
        corp_clicks: u32,
        corp_credits: u32,
        runner_clicks: u32,
        runner_credits: u32,
    ) -> GameState {
        GameState {
            corp: CorpState {
                resources: PlayerResources {
                    credits: Credits(corp_credits),
                    clicks: Clicks(corp_clicks),
                    agenda_points: AgendaPoints(0),
                },
                hq: Vec::new(),
                r_and_d: Vec::new(),
                archives: Vec::new(),
                installed: Vec::new(),
            },
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(runner_credits),
                    clicks: Clicks(runner_clicks),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
            },
            active_turn,
            active_run: None,
        }
    }

    #[test]
    fn corp_ending_turn_hands_control_to_runner_with_four_clicks() {
        let state = game_state(Side::Corp, 0, 5, 0, 2);
        let (next, events) = end_turn(&state).expect("should succeed");

        assert_eq!(next.active_turn, Side::Runner);
        assert_eq!(next.runner.resources.clicks, Clicks(4));
        assert_eq!(
            events,
            vec![
                GameEvent::TurnEnded { side: Side::Corp },
                GameEvent::TurnStarted { side: Side::Runner, clicks: 4 },
            ]
        );
    }

    #[test]
    fn runner_ending_turn_hands_control_to_corp_with_three_clicks() {
        let state = game_state(Side::Runner, 0, 5, 0, 2);
        let (next, events) = end_turn(&state).expect("should succeed");

        assert_eq!(next.active_turn, Side::Corp);
        assert_eq!(next.corp.resources.clicks, Clicks(3));
        assert_eq!(
            events,
            vec![
                GameEvent::TurnEnded { side: Side::Runner },
                GameEvent::TurnStarted { side: Side::Corp, clicks: 3 },
            ]
        );
    }

    #[test]
    fn ending_turn_does_not_change_either_sides_credits() {
        let state = game_state(Side::Corp, 0, 5, 0, 2);
        let (next, _events) = end_turn(&state).expect("should succeed");

        assert_eq!(next.corp.resources.credits, Credits(5));
        assert_eq!(next.runner.resources.credits, Credits(2));
    }

    #[test]
    fn ending_turn_while_a_run_is_active_errors() {
        let mut state = game_state(Side::Runner, 0, 5, 0, 2);
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: Vec::new(),
            position: 0,
        });

        assert_eq!(end_turn(&state), Err(RulesError::CannotEndTurnWhileRunActive));
    }
}
