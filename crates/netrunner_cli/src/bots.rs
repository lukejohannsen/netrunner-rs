//! Bridges `config::BotKind` selection to `netrunner_bots::BotAgent`
//! instances, and resolves whose decision is actually pending in a
//! `GameState` right now — needed because `GameState::phase` alone isn't
//! enough mid-run: `phase` stays `Action(Runner)` throughout a run even
//! while a `PaidAbilityWindow` briefly hands priority to the Corp (to rez
//! ICE, say), or while a `TraceState` awaits the Corp's bid.

use netrunner_bots::{BotAgent, HeuristicAgent, MctsAgent, RandomAgent};
use netrunner_core::rules::{GamePhase, GameState, Side};

use crate::config::BotKind;

/// `Human => None` — no agent drives that side; `App`/`headless::run` fall
/// back to waiting on a human keypress or (in headless mode) treat this the
/// same as `Random`, per `BotKind::Human`'s doc comment.
pub fn make_agent(kind: BotKind, side: Side, seed: u64) -> Option<Box<dyn BotAgent>> {
    match kind {
        BotKind::Human => None,
        BotKind::Random => Some(Box::new(RandomAgent::new(seed))),
        BotKind::Heuristic => Some(Box::new(HeuristicAgent::new(side, seed))),
        BotKind::Mcts => Some(Box::new(MctsAgent::new(side, seed))),
    }
}

/// Whose `PlayerAction` is legal right now, if anyone's. Precedence mirrors
/// the engine's own documented semantics (see `netrunner_core::rules::
/// action::PlayerAction::PassPriority`/`SubmitCorpTraceBid`'s doc comments
/// and `state::GamePhase`'s):
///
/// 1. An active trace awaits a bid — Corp first, then Runner.
/// 2. An open paid ability window holds priority for one side.
/// 3. Otherwise it's whichever side `GamePhase` names directly.
/// 4. `StartOfTurn`/`GameOver` — no player decision is pending.
pub fn current_actor(state: &GameState) -> Option<Side> {
    if let Some(trace) = &state.active_trace {
        return Some(if trace.corp_bid.is_none() { Side::Corp } else { Side::Runner });
    }
    if let Some(window) = &state.paid_ability_window {
        return Some(window.active_priority);
    }
    match state.phase {
        GamePhase::Mulligan(side) | GamePhase::Discard { side, .. } | GamePhase::Action(side) => Some(side),
        GamePhase::StartOfTurn(_) | GamePhase::GameOver(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{TraceResume, TraceState};

    fn base_state() -> GameState {
        GameState::new(0)
    }

    #[test]
    fn action_phase_names_the_acting_side_directly() {
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        assert_eq!(current_actor(&state), Some(Side::Runner));
    }

    #[test]
    fn discard_and_mulligan_phases_name_their_side() {
        let mut state = base_state();
        state.phase = GamePhase::Discard { side: Side::Corp, required: 1 };
        assert_eq!(current_actor(&state), Some(Side::Corp));

        state.phase = GamePhase::Mulligan(Side::Runner);
        assert_eq!(current_actor(&state), Some(Side::Runner));
    }

    #[test]
    fn start_of_turn_and_game_over_have_no_pending_actor() {
        let mut state = base_state();
        state.phase = GamePhase::StartOfTurn(Side::Corp);
        assert_eq!(current_actor(&state), None);

        state.phase = GamePhase::GameOver(Side::Runner);
        assert_eq!(current_actor(&state), None);
    }

    #[test]
    fn paid_ability_window_overrides_phase() {
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.paid_ability_window =
            Some(netrunner_core::rules::PaidAbilityWindow { active_priority: Side::Corp, consecutive_passes: 0, return_phase: Box::new(state.phase) });
        assert_eq!(current_actor(&state), Some(Side::Corp));
    }

    #[test]
    fn active_trace_overrides_phase_and_window_and_tracks_bid_progress() {
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Corp);
        state.active_trace = Some(TraceState {
            initiating_card: None,
            base_strength: 0,
            corp_bid: None,
            effect_on_success: netrunner_core::dsl::Effect::GiveTags(1),
            resume: TraceResume::None,
        });
        assert_eq!(current_actor(&state), Some(Side::Corp));

        state.active_trace.as_mut().unwrap().corp_bid = Some(2);
        assert_eq!(current_actor(&state), Some(Side::Runner));
    }
}
