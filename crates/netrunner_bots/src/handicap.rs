//! A strong agent made deliberately worse, by a measured amount.
//!
//! **Why this exists, and it is a measurement rather than a preference.**
//! A difficulty ladder needs rungs a player can feel apart, and the
//! obvious way to build them — turn the search budget down — does not
//! work on this workspace's Corp chair. Measured against a fixed one-ply
//! Runner, `puct` as Corp scores **0.458 at 32 simulations, 0.581 at 128
//! and 0.714 at 512** (ROADMAP Phase 2 §5 item 34) while the one-ply
//! heuristic Corp scores **0.583**. So search does not overtake one ply
//! until somewhere past 128, and the bots available are really three
//! strengths wearing five names: random near 0.04, a cluster at ~0.58,
//! and `puct@512` at 0.71. The first cut of `difficulty` dialled search
//! and calibrated to 0.562 / 0.500 / 0.458 across its middle three Corp
//! rungs — flat, and sloping the wrong way (Phase 5 §1).
//!
//! **So a rung is a strong bot playing worse, not a weak bot trying.**
//! With probability `epsilon` this picks a uniformly random legal action
//! instead of asking the inner agent. That makes the ladder **monotone by
//! construction**: `epsilon` 1.0 is exactly `RandomAgent`, 0.0 is exactly
//! the inner agent, and every value between is a mixture of two policies
//! whose strengths are known. Nothing has to be re-measured to know that
//! a smaller `epsilon` is stronger — only *how much* stronger.
//!
//! **It is also cheap where cheapness matters.** A low rung built by
//! handicapping `puct@512` would think for as long as the top rung while
//! playing badly, which is the worst of both for a person sitting in
//! front of it. `difficulty` therefore handicaps the *cheap* bot at the
//! bottom of the ladder and the expensive one only near the top.
//!
//! **The blunder is a whole action, not a worse evaluation.** The
//! alternative — perturbing `eval::Weights` — was rejected because a
//! mis-weighted bot is *consistently* wrong, which reads to a player as
//! a strange opponent rather than a beatable one, and because its
//! strength is then a function of six weights nobody has measured. A
//! random legal action is a recognisable mistake of the kind a human
//! makes, and it is one number.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{GameEvent, PlayerAction};
use netrunner_core::view::ClientView;

use crate::agent::BotAgent;

/// Wraps any `BotAgent`, replacing an `epsilon` share of its decisions
/// with a uniformly random legal action.
pub struct HandicapAgent<A: BotAgent> {
    inner: A,
    epsilon: f64,
    rng: StdRng,
}

impl<A: BotAgent> HandicapAgent<A> {
    /// `epsilon` is clamped to `0.0..=1.0`; 0.0 is the inner agent
    /// untouched and 1.0 never consults it at all.
    pub fn new(inner: A, epsilon: f64, seed: u64) -> Self {
        Self { inner, epsilon: epsilon.clamp(0.0, 1.0), rng: StdRng::seed_from_u64(seed) }
    }

    pub fn epsilon(&self) -> f64 {
        self.epsilon
    }
}

impl<A: BotAgent> BotAgent for HandicapAgent<A> {
    fn select_action(&mut self, view: &ClientView, registry: &CardRegistry) -> PlayerAction {
        assert!(!view.legal_actions.is_empty(), "BotAgent::select_action requires at least one legal action");
        // Rolled before the branch, not inside it, so the sequence of
        // draws — and therefore the whole game — is identical whether or
        // not this particular decision ends up handicapped. A rung's
        // games are reproducible for the same reason every other
        // measurement here is.
        let blunder = self.rng.random::<f64>() < self.epsilon;
        if !blunder {
            return self.inner.select_action(view, registry);
        }
        // The same `progressive` filter `RandomAgent` uses: a random walk
        // that may deselect inside a `min == max` prompt is the seating
        // most able to livelock, and a handicap must not reintroduce the
        // deadlock class the sweeps exist to catch.
        let choices = crate::agent::progressive(&view.legal_actions, view.pending_decision.as_ref());
        choices[self.rng.random_range(0..choices.len())].clone()
    }

    /// Forwarded: a handicapped agent is still the same agent, and one
    /// that tracks state would otherwise see a game it did not play.
    fn observe(&mut self, event: &GameEvent) {
        self.inner.observe(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{Clicks, GamePhase, GameState, Side};
    use netrunner_core::view::build_client_view;

    /// An inner agent that always picks the last legal action, so "did
    /// the handicap fire" is observable without a real search.
    struct Last;

    impl BotAgent for Last {
        fn select_action(&mut self, view: &ClientView, _registry: &CardRegistry) -> PlayerAction {
            view.legal_actions.last().expect("a legal action").clone()
        }
    }

    fn view() -> ClientView {
        let mut state = GameState::new(0);
        state.corp.resources.clicks = Clicks(3);
        state.phase = GamePhase::Action(Side::Corp);
        build_client_view(&state, &CardRegistry::new(), Side::Corp)
    }

    #[test]
    fn epsilon_zero_is_the_inner_agent_and_one_never_asks_it() {
        let view = view();
        let registry = CardRegistry::new();
        let expected = view.legal_actions.last().unwrap().clone();

        let mut untouched = HandicapAgent::new(Last, 0.0, 7);
        for _ in 0..32 {
            assert_eq!(untouched.select_action(&view, &registry), expected, "epsilon 0 must not blunder");
        }

        // At epsilon 1 every decision is random, so over many draws the
        // inner agent's answer cannot be the only one seen.
        let mut blind = HandicapAgent::new(Last, 1.0, 7);
        let picked: Vec<PlayerAction> = (0..64).map(|_| blind.select_action(&view, &registry)).collect();
        assert!(picked.iter().all(|action| view.legal_actions.contains(action)), "always legal");
        assert!(picked.iter().any(|action| *action != expected), "epsilon 1 must not be the inner agent");
    }

    /// The dial has to be ordered to be a difficulty dial at all: more
    /// epsilon, more blunders, over the same seed and the same position.
    #[test]
    fn more_epsilon_means_more_blunders() {
        let view = view();
        let registry = CardRegistry::new();
        let best = view.legal_actions.last().unwrap().clone();
        let blunders = |epsilon: f64| {
            let mut agent = HandicapAgent::new(Last, epsilon, 11);
            (0..400).filter(|_| agent.select_action(&view, &registry) != best).count()
        };
        let (low, mid, high) = (blunders(0.1), blunders(0.4), blunders(0.8));
        assert!(low < mid && mid < high, "{low} < {mid} < {high}");
        assert_eq!(blunders(0.0), 0);
    }

    #[test]
    fn epsilon_is_clamped_rather_than_trusted() {
        assert_eq!(HandicapAgent::new(Last, -1.0, 1).epsilon(), 0.0);
        assert_eq!(HandicapAgent::new(Last, 9.0, 1).epsilon(), 1.0);
    }
}
