use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{GameState, PlayerAction};

use crate::agent::BotAgent;

/// Picks uniformly at random among the legal actions — the same policy
/// `netrunner_cli`'s headless self-play loop used inline before this crate
/// existed (see `netrunner_cli::headless`), now packaged behind `BotAgent`.
pub struct RandomAgent {
    rng: StdRng,
}

impl RandomAgent {
    pub fn new(seed: u64) -> Self {
        Self { rng: StdRng::seed_from_u64(seed) }
    }
}

impl BotAgent for RandomAgent {
    fn select_action(&mut self, _state: &GameState, _registry: &CardRegistry, legal_actions: &[PlayerAction]) -> PlayerAction {
        assert!(!legal_actions.is_empty(), "BotAgent::select_action requires at least one legal action");
        legal_actions[self.rng.random_range(0..legal_actions.len())].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::Side;

    #[test]
    fn always_returns_a_member_of_legal_actions() {
        let state = GameState::new(0);
        let registry = CardRegistry::new();
        let legal = vec![
            PlayerAction::GainCreditClick { side: Side::Corp },
            PlayerAction::EndTurn,
            PlayerAction::DrawCardClick,
        ];
        let mut agent = RandomAgent::new(42);

        for _ in 0..20 {
            let action = agent.select_action(&state, &registry, &legal);
            assert!(legal.contains(&action));
        }
    }

    #[test]
    fn same_seed_is_deterministic() {
        let state = GameState::new(0);
        let registry = CardRegistry::new();
        let legal = vec![
            PlayerAction::GainCreditClick { side: Side::Corp },
            PlayerAction::EndTurn,
            PlayerAction::DrawCardClick,
        ];

        let mut a = RandomAgent::new(7);
        let mut b = RandomAgent::new(7);
        for _ in 0..10 {
            assert_eq!(a.select_action(&state, &registry, &legal), b.select_action(&state, &registry, &legal));
        }
    }
}
