use crate::dsl::CardId;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::state::{Clicks, GamePhase, GameState, Side};

/// Clicks the Corp receives at the start of each turn. A base turn-structure
/// constant of the game, not a card rule — same category as the "1 click"
/// cost every basic action already hardcodes.
const CORP_CLICKS_PER_TURN: u32 = 3;
/// Clicks the Runner receives at the start of each turn.
const RUNNER_CLICKS_PER_TURN: u32 = 4;
/// Maximum hand size before a side owes a mandatory discard at end of turn.
const CORP_MAX_HAND_SIZE: usize = 5;
const RUNNER_MAX_HAND_SIZE: usize = 5;

fn clicks_for(side: Side) -> u32 {
    match side {
        Side::Corp => CORP_CLICKS_PER_TURN,
        Side::Runner => RUNNER_CLICKS_PER_TURN,
    }
}

fn max_hand_size(side: Side) -> usize {
    match side {
        Side::Corp => CORP_MAX_HAND_SIZE,
        Side::Runner => RUNNER_MAX_HAND_SIZE,
    }
}

fn hand_size(state: &GameState, side: Side) -> usize {
    match side {
        Side::Corp => state.corp.hq.len(),
        Side::Runner => state.runner.grip.len(),
    }
}

/// Extracts `side` from `state.phase` if it's currently `Action(side)`, for
/// either side. `EndTurn` is symmetric — valid during whichever side's
/// Action phase happens to be active — unlike the fixed-side actions in
/// `engine.rs`, which know their expected side up front and gate on a
/// concrete `GamePhase::Action(side)` via `engine::require_phase` instead.
fn require_action_phase(state: &GameState) -> Result<Side, RulesError> {
    match state.phase {
        GamePhase::Action(side) => Ok(side),
        actual => Err(RulesError::NotInActionPhase { actual }),
    }
}

/// Extracts `(side, required)` from `state.phase` if it's currently
/// `Discard { .. }`, for whichever side owes the discard.
fn require_discard_phase(state: &GameState) -> Result<(Side, usize), RulesError> {
    match state.phase {
        GamePhase::Discard { side, required } => Ok((side, required)),
        actual => Err(RulesError::NotInDiscardPhase { actual }),
    }
}

/// Removes `card_id` from `side`'s hand (Corp's `hq` or Runner's `grip`).
/// Errors with `RulesError::CardNotInHand` if it isn't there.
fn take_from_hand(state: &mut GameState, side: Side, card_id: &CardId) -> Result<(), RulesError> {
    let hand = match side {
        Side::Corp => &mut state.corp.hq,
        Side::Runner => &mut state.runner.grip,
    };
    let position = hand
        .iter()
        .position(|c| c == card_id)
        .ok_or_else(|| RulesError::CardNotInHand {
            side,
            card: card_id.clone(),
        })?;
    hand.remove(position);
    Ok(())
}

/// Moves a discarded card into `side`'s discard pile (Corp's `archives` or
/// Runner's `heap`) — both fully public zones, unlike `hq`/`grip`.
fn discard_to_pile(state: &mut GameState, side: Side, card_id: CardId) {
    match side {
        Side::Corp => state.corp.archives.push(card_id),
        Side::Runner => state.runner.heap.push(card_id),
    }
}

/// End the active side's turn. Hands control to the other side via
/// [`enter_start_of_turn`] if the ending side's hand is within its max hand
/// size (`CORP_MAX_HAND_SIZE`/`RUNNER_MAX_HAND_SIZE`); otherwise transitions
/// to `GamePhase::Discard { side, required }` first — control only passes
/// once `PlayerAction::DiscardCard` (via [`discard_card`]) clears it.
///
/// Deliberately NOT modeled: start/end-of-turn card triggers (needs the
/// `dsl` trigger system wired into the engine, which doesn't happen yet).
///
/// Credits are untouched — they carry over turn to turn. The ending side's
/// own stale clicks are also left untouched rather than zeroed: every
/// click-spending action is already gated by `engine::require_phase`, so
/// leftover clicks are inert until that side's own next `end_turn`.
pub fn end_turn(state: &GameState) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = require_action_phase(state)?;
    if state.active_run.is_some() {
        return Err(RulesError::CannotEndTurnWhileRunActive);
    }

    let mut next = state.clone();
    let mut events = vec![GameEvent::TurnEnded { side }];

    let over_by = hand_size(&next, side).saturating_sub(max_hand_size(side));
    if over_by > 0 {
        next.phase = GamePhase::Discard { side, required: over_by };
        events.push(GameEvent::DiscardPending { side, required: over_by });
    } else {
        enter_start_of_turn(&mut next, &mut events, side.other());
    }

    Ok((next, events))
}

/// Discard `card_id` from hand to satisfy a pending mandatory discard (see
/// [`end_turn`]). Errors with `RulesError::NotInDiscardPhase` outside
/// `GamePhase::Discard`, or `RulesError::CardNotInHand` if the card isn't in
/// the owing side's hand. Once the phase's `required` count reaches zero,
/// hands control to the other side via [`enter_start_of_turn`] — the same
/// handoff `end_turn` performs directly when no discard was owed at all.
pub fn discard_card(
    state: &GameState,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let (side, required) = require_discard_phase(state)?;
    let mut next = state.clone();
    take_from_hand(&mut next, side, &card_id)?;
    discard_to_pile(&mut next, side, card_id.clone());
    let mut events = vec![GameEvent::CardDiscarded { side, card: card_id }];

    let remaining = required - 1; // `required > 0` is an invariant of the Discard phase.
    if remaining == 0 {
        enter_start_of_turn(&mut next, &mut events, side.other());
    } else {
        next.phase = GamePhase::Discard { side, required: remaining };
    }

    Ok((next, events))
}

/// Flips control, refills clicks, and resolves `StartOfTurn(next_side)`'s
/// mandatory triggers before auto-advancing to `Action(next_side)`.
/// Centralizing entry here (rather than a bare side check inline in
/// `end_turn`, as before `GamePhase` existed) is what lets a future
/// `StartOfTurn(Runner)` trigger reuse this hook instead of `end_turn`/
/// `discard_card` growing another special case. Called from both `end_turn`
/// (hand size already within limits) and `discard_card` (last mandatory
/// discard just cleared).
///
/// If control is passing to the Corp and their R&D is empty, the Corp is
/// unable to make their mandatory draw and loses immediately (deck-out) —
/// the turn never actually starts: no clicks are refilled and no
/// `TurnStarted` is emitted, only `GameEvent::GameOver`. This check has to
/// live here rather than in `win::check_win_conditions`, since it's this
/// exact draw attempt that fails, not a standing condition safely
/// re-derivable from `GameState` alone elsewhere — see
/// `check_win_conditions`'s doc comment.
fn enter_start_of_turn(next: &mut GameState, events: &mut Vec<GameEvent>, next_side: Side) {
    next.phase = GamePhase::StartOfTurn(next_side);

    if next_side == Side::Corp && next.corp.r_and_d.is_empty() {
        next.phase = GamePhase::GameOver(Side::Runner);
        events.push(GameEvent::GameOver { winner: Side::Runner });
        return;
    }

    let clicks = clicks_for(next_side);
    next.resources_mut(next_side).clicks = Clicks(clicks);
    events.push(GameEvent::TurnStarted { side: next_side, clicks });

    if next_side == Side::Corp {
        // Top of R&D mirrors `RunnerState::stack`'s convention — drawing
        // pops the end of the Vec (see `engine.rs::draw_card_click`).
        if let Some(card) = next.corp.r_and_d.pop() {
            next.corp.hq.push(card);
            events.push(GameEvent::CardDrawn { side: Side::Corp });
        }
    }

    next.phase = GamePhase::Action(next_side);
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
                heap: Vec::new(),
            },
            phase: GamePhase::Action(active_turn),
            active_run: None,
            seed: 0,
            rng_step: 0,
        }
    }

    #[test]
    fn corp_ending_turn_hands_control_to_runner_with_four_clicks() {
        let state = game_state(Side::Corp, 0, 5, 0, 2);
        let (next, events) = end_turn(&state).expect("should succeed");

        assert_eq!(next.phase, GamePhase::Action(Side::Runner));
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
        let mut state = game_state(Side::Runner, 0, 5, 0, 2);
        // A non-empty R&D so the Corp's mandatory draw succeeds rather than
        // decking out — this test is about the click handoff, not deck-out.
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];
        let (next, events) = end_turn(&state).expect("should succeed");

        assert_eq!(next.phase, GamePhase::Action(Side::Corp));
        assert_eq!(next.corp.resources.clicks, Clicks(3));
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
    fn mandatory_draw_with_empty_rd_ends_game_with_runner_win() {
        let state = game_state(Side::Runner, 0, 5, 0, 2);
        let (next, events) = end_turn(&state).expect("should succeed");

        // Deck-out: the Corp can't make their mandatory draw, so the game
        // ends immediately — no underflow/panic, but also no turn starts
        // (no clicks refilled, no `TurnStarted`).
        assert!(next.corp.hq.is_empty());
        assert_eq!(next.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(next.corp.resources.clicks, Clicks(0));
        assert_eq!(
            events,
            vec![
                GameEvent::TurnEnded { side: Side::Runner },
                GameEvent::GameOver { winner: Side::Runner },
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

    #[test]
    fn ending_turn_outside_action_phase_returns_not_in_action_phase() {
        let mut state = game_state(Side::Corp, 3, 5, 0, 2);
        state.phase = GamePhase::Discard { side: Side::Corp, required: 1 };

        assert_eq!(
            end_turn(&state),
            Err(RulesError::NotInActionPhase {
                actual: GamePhase::Discard { side: Side::Corp, required: 1 }
            })
        );
    }

    #[test]
    fn ending_turn_over_hand_size_transitions_to_discard_instead_of_next_start_of_turn() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.corp.hq = (0..6).map(|i| CardId(format!("card_{i}"))).collect();
        let (next, events) = end_turn(&state).expect("should succeed");

        assert_eq!(next.phase, GamePhase::Discard { side: Side::Corp, required: 1 });
        // Control has NOT passed to the Runner yet — clicks are untouched.
        assert_eq!(next.runner.resources.clicks, Clicks(0));
        assert_eq!(
            events,
            vec![
                GameEvent::TurnEnded { side: Side::Corp },
                GameEvent::DiscardPending { side: Side::Corp, required: 1 },
            ]
        );
    }

    #[test]
    fn ending_turn_within_hand_size_skips_discard_entirely() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.corp.hq = (0..5).map(|i| CardId(format!("card_{i}"))).collect();
        let (next, _events) = end_turn(&state).expect("should succeed");

        assert_eq!(next.phase, GamePhase::Action(Side::Runner));
    }

    #[test]
    fn discard_card_moves_card_from_hq_to_archives() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.phase = GamePhase::Discard { side: Side::Corp, required: 1 };
        state.corp.hq = vec![CardId("hedge_fund".to_string())];

        let (next, events) =
            discard_card(&state, CardId("hedge_fund".to_string())).expect("should succeed");

        assert!(next.corp.hq.is_empty());
        assert_eq!(next.corp.archives, vec![CardId("hedge_fund".to_string())]);
        // Last mandatory discard cleared: control passes to the Runner.
        assert_eq!(next.phase, GamePhase::Action(Side::Runner));
        assert_eq!(next.runner.resources.clicks, Clicks(4));
        assert_eq!(
            events,
            vec![
                GameEvent::CardDiscarded { side: Side::Corp, card: CardId("hedge_fund".to_string()) },
                GameEvent::TurnStarted { side: Side::Runner, clicks: 4 },
            ]
        );
    }

    #[test]
    fn discard_card_moves_card_from_grip_to_heap() {
        let mut state = game_state(Side::Runner, 0, 5, 0, 2);
        state.phase = GamePhase::Discard { side: Side::Runner, required: 1 };
        state.runner.grip = vec![CardId("sure_gamble".to_string())];
        // A non-empty R&D so the Corp's mandatory draw succeeds rather than
        // decking out — this test is about the heap mechanic, not deck-out.
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];

        let (next, _events) =
            discard_card(&state, CardId("sure_gamble".to_string())).expect("should succeed");

        assert!(next.runner.grip.is_empty());
        assert_eq!(next.runner.heap, vec![CardId("sure_gamble".to_string())]);
        assert_eq!(next.phase, GamePhase::Action(Side::Corp));
    }

    #[test]
    fn discard_card_with_required_greater_than_one_stays_in_discard_phase() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.phase = GamePhase::Discard { side: Side::Corp, required: 2 };
        state.corp.hq =
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())];

        let (next, events) =
            discard_card(&state, CardId("hedge_fund".to_string())).expect("should succeed");

        assert_eq!(next.corp.hq, vec![CardId("ice_wall".to_string())]);
        assert_eq!(next.phase, GamePhase::Discard { side: Side::Corp, required: 1 });
        assert_eq!(
            events,
            vec![GameEvent::CardDiscarded {
                side: Side::Corp,
                card: CardId("hedge_fund".to_string())
            }]
        );
    }

    #[test]
    fn discard_card_outside_discard_phase_returns_not_in_discard_phase() {
        let state = game_state(Side::Corp, 3, 5, 0, 2);

        assert_eq!(
            discard_card(&state, CardId("hedge_fund".to_string())),
            Err(RulesError::NotInDiscardPhase { actual: GamePhase::Action(Side::Corp) })
        );
    }

    #[test]
    fn discard_card_not_in_hand_returns_card_not_in_hand() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.phase = GamePhase::Discard { side: Side::Corp, required: 1 };

        assert_eq!(
            discard_card(&state, CardId("hedge_fund".to_string())),
            Err(RulesError::CardNotInHand {
                side: Side::Corp,
                card: CardId("hedge_fund".to_string())
            })
        );
    }
}
