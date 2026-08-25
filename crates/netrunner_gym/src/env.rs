//! A plain-Rust, single-episode-at-a-time RL environment over
//! `netrunner_core`'s fixed `ActionSpace`. No PyO3 types appear anywhere in
//! this module's API — see `crate::python` for the thin binding that
//! exposes this to Python. Testable directly via plain `cargo test`, no GIL
//! involved.
//!
//! Shape mirrors `netrunner_server::MatchSession::run`: the environment
//! owns a fixed `agent_side` and drives the *other* side with an embedded
//! `BotAgent`, fast-forwarding through every one of its decisions (however
//! many — mulligan, discard, paid-ability windows, trace bids, run
//! choices) via `current_actor`, exactly the way `MatchSession` drives a
//! full bot-vs-bot game. `reset`/`step_index` never return control to the
//! caller until it's genuinely `agent_side`'s decision again, or the game
//! is over.

use std::str::FromStr;

use netrunner_bots::{evaluate_state, step_index as bots_step_index, BotAgent, HeuristicAgent, IndexedActionError, RandomAgent};
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{current_actor, get_action_mask, ActionSpace, Deck, GamePhase, GameState, Side};
use netrunner_core::view::build_client_view;

use crate::fixtures;
use crate::observation::encode_observation;

pub const ACTION_SPACE_SIZE: usize = ActionSpace::SIZE;

/// Guards the opponent fast-forward loop against ever looping unboundedly
/// — mirrors `netrunner_server::match_session::MAX_STEPS`'s role, just
/// scoped to "opponent decisions between two of the agent's own steps"
/// rather than a whole game.
const MAX_OPPONENT_FAST_FORWARD_STEPS: u32 = 1_000;

/// Squash scale for `evaluate_state` deltas, matching
/// `netrunner_bots::policy::UniformPolicyEvaluator`'s value-head
/// convention — keeps reward roughly in `[-1, 1]` per step, including at
/// `GameOver` (`evaluate_state` returns a literal `±1000` there, i.e.
/// `tanh(±10)`, already effectively saturated).
const REWARD_SQUASH_SCALE: f64 = 100.0;

/// Penalty applied for an in-range but currently-illegal action index —
/// distinct from (and much larger in magnitude than) ordinary per-step
/// reward deltas, so a masked-out action is clearly discouraged without
/// crashing the episode.
const INVALID_ACTION_PENALTY: f32 = -1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opponent {
    Random,
    Heuristic,
}

impl FromStr for Opponent {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "random" => Ok(Opponent::Random),
            "heuristic" => Ok(Opponent::Heuristic),
            other => Err(format!("unknown opponent {other:?}; expected \"random\" or \"heuristic\"")),
        }
    }
}

impl Opponent {
    fn build(self, side: Side, seed: u64) -> Box<dyn BotAgent> {
        match self {
            Opponent::Random => Box::new(RandomAgent::new(seed)),
            Opponent::Heuristic => Box::new(HeuristicAgent::new(side, seed)),
        }
    }
}

/// The result of one `NetrunnerEnv::step_index` call — already carries
/// everything a Gymnasium `step()` needs (`observation`/`action_mask`
/// split rather than pre-merged into a dict, since dict assembly is
/// `crate::python`'s/the Python wrapper's job, not this layer's).
#[derive(Debug)]
pub struct StepOutcome {
    pub observation: Vec<f32>,
    pub action_mask: Vec<bool>,
    pub reward: f32,
    pub terminated: bool,
    pub truncated: bool,
    pub invalid_action: bool,
    pub message: Option<String>,
}

/// Returned by `step_index` when `index` is outside `0..ACTION_SPACE_SIZE`
/// — a genuine caller bug (not a training-time event), kept distinct from
/// `StepOutcome::invalid_action`'s in-range-but-currently-illegal case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfRangeIndex(pub usize);

pub struct NetrunnerEnv {
    registry: CardRegistry,
    corp_deck: Deck,
    runner_deck: Deck,
    agent_side: Side,
    opponent_kind: Opponent,
    opponent: Box<dyn BotAgent>,
    opponent_seed: u64,
    state: GameState,
    seed: u64,
    steps_this_episode: u32,
    max_episode_steps: u32,
}

impl NetrunnerEnv {
    pub fn new(agent_side: Side, seed: u64, opponent: Opponent, max_episode_steps: u32) -> Self {
        let registry = fixtures::kate_vs_hb_registry();
        let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
        let opponent_seed = seed ^ 0xC0FF_EE00_C0FF_EE00;

        // `GameState::setup` needs a real `GameState` before `reset` can
        // build one — construct a throwaway one just to satisfy field
        // initialization, immediately overwritten by `reset` below.
        let (placeholder_state, _events) =
            GameState::setup(&corp_deck, &runner_deck, &registry, seed).expect("fixtures decks are legal by construction");

        let mut env = NetrunnerEnv {
            registry,
            corp_deck,
            runner_deck,
            agent_side,
            opponent_kind: opponent,
            opponent: opponent.build(agent_side.other(), opponent_seed),
            opponent_seed,
            state: placeholder_state,
            seed,
            steps_this_episode: 0,
            max_episode_steps: max_episode_steps.max(1),
        };
        env.reset(Some(seed));
        env
    }

    /// Starts a fresh episode (`GameState::setup` with a new seed, if
    /// given) and fast-forwards through every opponent decision until it's
    /// genuinely `agent_side`'s turn or the game is already over. Returns
    /// `(observation, action_mask)` for the resulting state.
    pub fn reset(&mut self, seed: Option<u64>) -> (Vec<f32>, Vec<bool>) {
        if let Some(seed) = seed {
            self.seed = seed;
        }
        self.opponent = self.opponent_kind.build(self.agent_side.other(), self.opponent_seed);
        let (state, _events) = GameState::setup(&self.corp_deck, &self.runner_deck, &self.registry, self.seed)
            .expect("fixtures decks are legal by construction");
        self.state = state;
        self.steps_this_episode = 0;

        self.fast_forward_opponent();
        (self.observation(), self.action_mask())
    }

    pub fn action_mask(&self) -> Vec<bool> {
        get_action_mask(&self.state, &self.registry)
    }

    pub fn observation(&self) -> Vec<f32> {
        encode_observation(&self.state, &self.registry, self.agent_side)
    }

    pub fn is_over(&self) -> bool {
        matches!(self.state.phase, GamePhase::GameOver(_))
    }

    /// Applies `index` as `agent_side`'s action, then fast-forwards through
    /// the opponent's reply (however many decisions that takes). An index
    /// outside `0..ACTION_SPACE_SIZE` is a caller bug, reported as `Err`;
    /// an in-range index that's illegal *right now* is reported via
    /// `StepOutcome::invalid_action` instead (state left untouched, a
    /// penalty reward applied) — see the module doc comment and
    /// `crate::env`'s design notes for why these are handled differently.
    pub fn step_index(&mut self, index: usize) -> Result<StepOutcome, OutOfRangeIndex> {
        if index >= ACTION_SPACE_SIZE {
            return Err(OutOfRangeIndex(index));
        }

        let value_before = evaluate_state(&self.state, self.agent_side);

        match bots_step_index(&self.state, &self.registry, index) {
            Ok((next_state, _events)) => {
                self.state = next_state;
                self.steps_this_episode += 1;
                self.fast_forward_opponent();

                let value_after = evaluate_state(&self.state, self.agent_side);
                let reward = (squash(value_after) - squash(value_before)) as f32;
                let terminated = self.is_over();
                let truncated = !terminated && self.steps_this_episode >= self.max_episode_steps;

                Ok(StepOutcome {
                    observation: self.observation(),
                    action_mask: self.action_mask(),
                    reward,
                    terminated,
                    truncated,
                    invalid_action: false,
                    message: None,
                })
            }
            Err(error) => Ok(StepOutcome {
                observation: self.observation(),
                action_mask: self.action_mask(),
                reward: INVALID_ACTION_PENALTY,
                terminated: false,
                truncated: false,
                invalid_action: true,
                message: Some(describe_indexed_action_error(&error)),
            }),
        }
    }

    /// Repeatedly resolves whichever side isn't `agent_side` via the
    /// embedded opponent `BotAgent`, exactly as `MatchSession::run` drives
    /// a bot slot — see the module doc comment. Stops once it's
    /// `agent_side`'s decision, the game ends, or the safety budget is
    /// exhausted (treated the same as "stop": the caller's next
    /// `action_mask`/`observation` call reflects wherever this left off).
    fn fast_forward_opponent(&mut self) {
        for _ in 0..MAX_OPPONENT_FAST_FORWARD_STEPS {
            if self.is_over() {
                return;
            }
            let Some(side) = current_actor(&self.state) else { return };
            if side == self.agent_side {
                return;
            }

            let view = build_client_view(&self.state, &self.registry, side);
            if view.legal_actions.is_empty() {
                return;
            }
            let action = self.opponent.select_action(&view, &self.registry);
            match self.state.step(&self.registry, action) {
                Ok((next, _events)) => self.state = next,
                // The opponent only ever picks from `view.legal_actions`,
                // so this should never actually happen — stop rather than
                // loop forever on a state that can't advance.
                Err(_) => return,
            }
        }
    }
}

fn squash(value: f64) -> f64 {
    (value / REWARD_SQUASH_SCALE).tanh()
}

fn describe_indexed_action_error(error: &IndexedActionError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(agent_side: Side, seed: u64) -> NetrunnerEnv {
        NetrunnerEnv::new(agent_side, seed, Opponent::Random, 200)
    }

    #[test]
    fn action_mask_length_and_content_matches_get_action_mask() {
        let env = env(Side::Runner, 1);
        let mask = env.action_mask();
        assert_eq!(mask.len(), ACTION_SPACE_SIZE);
        assert_eq!(mask, get_action_mask(&env.state, &env.registry));
    }

    #[test]
    fn observation_length_matches_obs_size() {
        let env = env(Side::Corp, 2);
        assert_eq!(env.observation().len(), crate::observation::OBS_SIZE);
    }

    #[test]
    fn reset_lands_on_the_agent_sides_own_decision_or_game_over() {
        for side in [Side::Corp, Side::Runner] {
            let mut env = env(side, 3);
            let (_obs, mask) = env.reset(Some(3));
            assert!(env.is_over() || current_actor(&env.state) == Some(side));
            assert_eq!(mask.len(), ACTION_SPACE_SIZE);
        }
    }

    #[test]
    fn stepping_a_legal_index_advances_the_episode_step_count() {
        let mut env = env(Side::Corp, 4);
        let mask = env.action_mask();
        let legal_index = mask.iter().position(|&legal| legal).expect("fresh mulligan decision has legal actions");

        let outcome = env.step_index(legal_index).unwrap();
        assert!(!outcome.invalid_action);
        assert_eq!(env.steps_this_episode, 1);
    }

    #[test]
    fn stepping_a_masked_out_index_does_not_advance_the_episode() {
        let mut env = env(Side::Corp, 5);
        let mask = env.action_mask();
        let illegal_index = mask.iter().position(|&legal| !legal).expect("mulligan phase never has every slot legal");

        let outcome = env.step_index(illegal_index).unwrap();
        assert!(outcome.invalid_action);
        assert_eq!(outcome.reward, INVALID_ACTION_PENALTY);
        assert!(!outcome.terminated);
        assert_eq!(env.steps_this_episode, 0);
        assert_eq!(env.action_mask(), mask);
    }

    #[test]
    fn stepping_an_out_of_range_index_returns_an_error() {
        let mut env = env(Side::Runner, 6);
        // `StepOutcome` isn't `Debug`/`PartialEq` (it's a one-shot result
        // struct, not meant for equality comparisons), so compare only the
        // `Err` side rather than the whole `Result`.
        assert_eq!(env.step_index(ACTION_SPACE_SIZE).unwrap_err(), OutOfRangeIndex(ACTION_SPACE_SIZE));
        assert_eq!(env.step_index(ACTION_SPACE_SIZE + 1_000).unwrap_err(), OutOfRangeIndex(ACTION_SPACE_SIZE + 1_000));
    }

    #[test]
    fn a_full_random_policy_episode_terminates_or_truncates_without_panicking() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        for side in [Side::Corp, Side::Runner] {
            let mut rng = StdRng::seed_from_u64(7);
            let mut env = env(side, 7);
            let mut ended_via_terminated_or_truncated = false;

            loop {
                let mask = env.action_mask();
                let legal: Vec<usize> = mask.iter().enumerate().filter_map(|(i, &legal)| legal.then_some(i)).collect();
                // A non-empty mask on every reachable state is exactly
                // what `legal_actions`/`current_actor` guarantee whenever
                // it's someone's decision and the game isn't over — an
                // empty mask here would mean this test's own driving loop
                // is stuck, not a real game state, so fail loudly instead
                // of silently stopping.
                assert!(!legal.is_empty() || env.is_over(), "no legal actions but the game isn't over");
                if legal.is_empty() {
                    break;
                }
                let index = legal[rand::Rng::random_range(&mut rng, 0..legal.len())];
                let outcome = env.step_index(index).unwrap();
                if outcome.terminated || outcome.truncated {
                    ended_via_terminated_or_truncated = true;
                    break;
                }
            }

            assert!(ended_via_terminated_or_truncated || env.is_over());
        }
    }
}
