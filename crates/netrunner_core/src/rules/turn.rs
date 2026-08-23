use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::state::{Clicks, GameState, Side};

/// Clicks the Corp receives at the start of each turn. A base turn-structure
/// constant of the game, not a card rule — same category as the "1 click"
/// cost every basic action already hardcodes.
const CORP_CLICKS_PER_TURN: u32 = 3;
/// Clicks the Runner receives at the start of each turn.
const RUNNER_CLICKS_PER_TURN: u32 = 4;

/// End the active side's turn: hand control to the other side, refill their
/// clicks to their fixed per-turn allotment, and — if control is passing to
/// the Corp — perform their mandatory start-of-turn draw from R&D into HQ.
///
/// Deliberately NOT modeled: end-of-turn hand-size discard/cleanup, the
/// Runner's turn having no mandatory draw (correct per the real rules — only
/// the Corp draws automatically), and start/end-of-turn card triggers (needs
/// the `dsl` trigger system wired into the engine, which doesn't happen
/// yet).
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

    let mut events = vec![
        GameEvent::TurnEnded { side: ending_side },
        GameEvent::TurnStarted { side: next_side, clicks },
    ];

    if next_side == Side::Corp {
        // Top of R&D mirrors `RunnerState::stack`'s convention — drawing
        // pops the end of the Vec (see `engine.rs::draw_card_click`).
        if let Some(card) = next.corp.r_and_d.pop() {
            next.corp.hq.push(card);
            events.push(GameEvent::CardDrawn { side: Side::Corp });
        }
    }

    Ok((next, events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::CardId;
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
            seed: 0,
            rng_step: 0,
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
    fn runner_ending_turn_gives_corp_a_mandatory_draw_into_hq() {
        let mut state = game_state(Side::Runner, 0, 5, 0, 2);
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())];
        let (next, events) = end_turn(&state).expect("should succeed");

        // Draws from the top of R&D, i.e. the end of the Vec (mirrors
        // RunnerState::stack's convention).
        assert_eq!(next.corp.r_and_d, vec![CardId("hedge_fund".to_string())]);
        assert_eq!(next.corp.hq, vec![CardId("ice_wall".to_string())]);
        assert_eq!(
            events,
            vec![
                GameEvent::TurnEnded { side: Side::Runner },
                GameEvent::TurnStarted { side: Side::Corp, clicks: 3 },
                GameEvent::CardDrawn { side: Side::Corp },
            ]
        );
    }

    #[test]
    fn corp_ending_turn_gives_no_draw_since_only_the_corp_draws_automatically() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.runner.stack = vec![CardId("sure_gamble".to_string())];
        let (next, events) = end_turn(&state).expect("should succeed");

        // Control passed to the Runner, so no automatic draw happens here —
        // only the Corp draws automatically at the start of their turn.
        assert_eq!(next.runner.stack, vec![CardId("sure_gamble".to_string())]);
        assert!(next.runner.grip.is_empty());
        assert_eq!(
            events,
            vec![
                GameEvent::TurnEnded { side: Side::Corp },
                GameEvent::TurnStarted { side: Side::Runner, clicks: 4 },
            ]
        );
    }

    #[test]
    fn mandatory_draw_with_empty_rd_does_not_underflow() {
        let state = game_state(Side::Runner, 0, 5, 0, 2);
        let (next, events) = end_turn(&state).expect("should succeed");

        assert!(next.corp.hq.is_empty());
        assert_eq!(
            events,
            vec![
                GameEvent::TurnEnded { side: Side::Runner },
                GameEvent::TurnStarted { side: Side::Corp, clicks: 3 },
            ]
        );
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
