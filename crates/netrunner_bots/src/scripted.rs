//! The opponent seat of a lesson: canned actions, then a passive fallback.
//!
//! A tutorial opponent has one job — put the board where the lesson needs
//! it and otherwise stay out of the way. A `RandomAgent` would wander; a
//! `HeuristicAgent` would rez, run and score on its own schedule, and the
//! learner's second play-through would look nothing like the first. So this
//! agent plays exactly the script the lesson author wrote, and when the
//! script is exhausted (or its head is not yet legal) it does the least
//! eventful legal thing: keeps its hand, passes priority, clicks for
//! credits, ends its turn. It never rezzes, runs or plays a card unless
//! told to.
//!
//! Lives here rather than in `netrunner_core::tutorial` because it is a
//! `BotAgent` — bot logic belongs in this crate, per AGENTS.md — and
//! rather than in the session crate because a seat is not a driver.

use std::collections::VecDeque;

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::PlayerAction;
use netrunner_core::tutorial::ScriptedAction;
use netrunner_core::view::ClientView;

use crate::agent::BotAgent;

pub struct ScriptedAgent {
    script: VecDeque<ScriptedAction>,
}

impl ScriptedAgent {
    pub fn new(script: Vec<ScriptedAction>) -> Self {
        Self { script: script.into() }
    }

    /// Canned actions not yet played.
    pub fn remaining(&self) -> usize {
        self.script.len()
    }

    /// The fallback, in order of preference. `KeepHand` and `PassPriority`
    /// first because they are the "nothing happens" answers to the
    /// decisions the engine forces on every seat; `GainCreditClick` before
    /// `EndTurn` so the opponent spends its clicks (an opponent that ends
    /// with three clicks unspent is a stranger board than one that clicked
    /// for credits, and a Corp that never draws would deck out no sooner
    /// either way); `DiscardCard` for the discard phase; and the first
    /// legal action for anything else — a pending choice some card forced,
    /// where "first option" is as neutral as this agent can be.
    fn passive(legal: &[PlayerAction]) -> PlayerAction {
        let prefer = |predicate: fn(&PlayerAction) -> bool| legal.iter().find(|action| predicate(action)).cloned();
        prefer(|a| matches!(a, PlayerAction::KeepHand))
            .or_else(|| prefer(|a| matches!(a, PlayerAction::PassPriority { .. })))
            .or_else(|| prefer(|a| matches!(a, PlayerAction::DeclinePendingPaidChoice)))
            .or_else(|| prefer(|a| matches!(a, PlayerAction::GainCreditClick { .. })))
            .or_else(|| prefer(|a| matches!(a, PlayerAction::EndTurn)))
            .or_else(|| prefer(|a| matches!(a, PlayerAction::DiscardCard { .. })))
            .unwrap_or_else(|| legal[0].clone())
    }
}

impl BotAgent for ScriptedAgent {
    fn select_action(&mut self, view: &ClientView, _registry: &CardRegistry) -> PlayerAction {
        assert!(!view.legal_actions.is_empty(), "BotAgent::select_action requires at least one legal action");
        if let Some(head) = self.script.front()
            && head.turn.is_none_or(|turn| turn == view.turn)
            && view.legal_actions.contains(&head.action)
        {
            return self.script.pop_front().expect("front was Some").action;
        }
        Self::passive(&view.legal_actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::cards::register_playable_cards;
    use netrunner_core::decks;
    use netrunner_core::rules::{GamePhase, GameState, ServerId, Side};
    use netrunner_core::view::build_client_view;

    fn registry() -> CardRegistry {
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        registry
    }

    fn scripted(action: PlayerAction, turn: Option<u32>) -> ScriptedAction {
        ScriptedAction { action, turn }
    }

    #[test]
    fn the_script_head_plays_when_legal_and_on_its_turn() {
        let registry = registry();
        let mut state = GameState::new(3);
        state.runner.resources.clicks = netrunner_core::rules::Clicks(4);
        state.runner.resources.credits = netrunner_core::rules::Credits(5);
        state.phase = GamePhase::Action(Side::Runner);
        state.turn = 2;
        let view = build_client_view(&state, &registry, Side::Runner);
        let run = PlayerAction::InitiateRun { server: ServerId::Hq };
        assert!(view.legal_actions.contains(&run));

        let mut later = ScriptedAgent::new(vec![scripted(run.clone(), Some(3))]);
        assert_eq!(later.select_action(&view, &registry), PlayerAction::GainCreditClick { side: Side::Runner }, "not its turn yet");
        assert_eq!(later.remaining(), 1);

        let mut now = ScriptedAgent::new(vec![scripted(run.clone(), Some(2)), scripted(PlayerAction::JackOut, None)]);
        assert_eq!(now.select_action(&view, &registry), run);
        assert_eq!(now.remaining(), 1);
        assert_eq!(now.select_action(&view, &registry), PlayerAction::GainCreditClick { side: Side::Runner }, "JackOut is not legal here, so the fallback plays");
    }

    #[test]
    fn the_fallback_prefers_the_quietest_legal_action() {
        let legal = vec![PlayerAction::EndTurn, PlayerAction::DrawCardClick { side: Side::Corp }, PlayerAction::GainCreditClick { side: Side::Corp }];
        assert_eq!(ScriptedAgent::passive(&legal), PlayerAction::GainCreditClick { side: Side::Corp });
        let window = vec![PlayerAction::RezIce { ice: netrunner_core::rules::InstallId(1) }, PlayerAction::PassPriority { side: Side::Corp }];
        assert_eq!(ScriptedAgent::passive(&window), PlayerAction::PassPriority { side: Side::Corp }, "never rezzes on its own");
        let mulligan = vec![PlayerAction::KeepHand, PlayerAction::TakeMulligan];
        assert_eq!(ScriptedAgent::passive(&mulligan), PlayerAction::KeepHand);
        let forced = vec![PlayerAction::ResolvePendingChoice { option_index: 0 }, PlayerAction::ResolvePendingChoice { option_index: 1 }];
        assert_eq!(ScriptedAgent::passive(&forced), PlayerAction::ResolvePendingChoice { option_index: 0 });
    }

    /// A passive opponent on both seats plays a whole starter game without
    /// ever choosing an illegal action — the session panics if it does —
    /// and, never scoring or stealing, ends it by decking out.
    #[test]
    fn two_passive_seats_play_a_starter_game_to_its_end() {
        use netrunner_session_free::run_to_end;
        let registry = registry();
        let corp = decks::by_id("the_syndicate_starter").unwrap();
        let runner = decks::by_id("the_catalyst_starter").unwrap();
        let (state, _) = GameState::setup(&corp.to_deck(), &runner.to_deck(), &registry, 5).unwrap();
        let phase = run_to_end(state, &registry, ScriptedAgent::new(vec![]), ScriptedAgent::new(vec![]));
        assert!(matches!(phase, GamePhase::GameOver(_)), "expected a finished game, got {phase:?}");
    }

    /// `netrunner_bots` cannot depend on `netrunner_session` (it is the
    /// other way round), so this drives two agents with the engine's own
    /// `current_actor`/`apply_action` — the one place outside the session
    /// crate that may, because it is testing agents, not running a match.
    mod netrunner_session_free {
        use super::*;
        use netrunner_core::rules::{apply_action, current_actor};

        pub fn run_to_end(mut state: GameState, registry: &CardRegistry, mut corp: ScriptedAgent, mut runner: ScriptedAgent) -> GamePhase {
            for _ in 0..4_000 {
                if matches!(state.phase, GamePhase::GameOver(_)) {
                    break;
                }
                let Some(side) = current_actor(&state) else { panic!("no actor at {:?}", state.phase) };
                let view = build_client_view(&state, registry, side);
                assert!(!view.legal_actions.is_empty(), "{side:?} has no legal action at {:?}", state.phase);
                let action = match side {
                    Side::Corp => corp.select_action(&view, registry),
                    Side::Runner => runner.select_action(&view, registry),
                };
                state = apply_action(&state, registry, action.clone())
                    .unwrap_or_else(|e| panic!("passive {side:?} chose {action:?}, rejected: {e:?}"))
                    .0;
            }
            state.phase
        }
    }
}
