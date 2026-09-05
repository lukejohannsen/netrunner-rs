use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::PlayerAction;
use netrunner_core::view::ClientView;

use crate::agent::BotAgent;

/// Picks uniformly at random among `view.legal_actions` — the same policy
/// `netrunner_cli`'s headless self-play loop used inline before this crate
/// existed. No determinization needed: it never looks past the immediate
/// legal action set.
pub struct RandomAgent {
    rng: StdRng,
}

impl RandomAgent {
    pub fn new(seed: u64) -> Self {
        Self { rng: StdRng::seed_from_u64(seed) }
    }
}

impl BotAgent for RandomAgent {
    fn select_action(&mut self, view: &ClientView, _registry: &CardRegistry) -> PlayerAction {
        assert!(!view.legal_actions.is_empty(), "BotAgent::select_action requires at least one legal action");
        // Uniform over the *progressive* moves: a random walk that can
        // deselect spends most of its time below a `min == max` prompt's
        // bound, and the one seating the sweeps call "unbiased" was quietly
        // the one most able to livelock. See `agent::is_regressive`.
        let choices = crate::agent::progressive(&view.legal_actions, view.pending_decision.as_ref());
        choices[self.rng.random_range(0..choices.len())].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{GamePhase, GameState, Side};
    use netrunner_core::view::build_client_view;

    fn view() -> ClientView {
        let mut state = GameState::new(0);
        state.corp.resources.clicks = netrunner_core::rules::Clicks(3);
        state.phase = GamePhase::Action(Side::Corp);
        build_client_view(&state, &CardRegistry::new(), Side::Corp)
    }

    #[test]
    fn always_returns_a_member_of_legal_actions() {
        let view = view();
        assert!(!view.legal_actions.is_empty());
        let mut agent = RandomAgent::new(42);

        for _ in 0..20 {
            let action = agent.select_action(&view, &CardRegistry::new());
            assert!(view.legal_actions.contains(&action));
        }
    }

    #[test]
    fn same_seed_is_deterministic() {
        let view = view();
        let mut a = RandomAgent::new(7);
        let mut b = RandomAgent::new(7);
        for _ in 0..10 {
            assert_eq!(a.select_action(&view, &CardRegistry::new()), b.select_action(&view, &CardRegistry::new()));
        }
    }
}
