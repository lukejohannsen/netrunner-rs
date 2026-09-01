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

use netrunner_bots::{evaluate_state, BotAgent, HeuristicAgent, IndexedActionError, RandomAgent};
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{get_action_mask, ActionSpace, Deck, GamePhase, GameState, Side};
use netrunner_session::{Seat, Session, SubmitError};

use crate::fixtures;
use netrunner_bots::observation::encode_observation;

pub const ACTION_SPACE_SIZE: usize = ActionSpace::SIZE;

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
    opponent_seed: u64,
    /// The shared match driver. The agent's own seat is `Seat::External`
    /// (its action arrives from Python, long after the session would have
    /// had to ask for it); the opponent is a `Seat::Agent` the session
    /// resolves itself, which is what `fast_forward_opponent` now does by
    /// simply pumping `run`.
    session: Session,
    seed: u64,
    steps_this_episode: u32,
    max_episode_steps: u32,
}

impl NetrunnerEnv {
    pub fn new(agent_side: Side, seed: u64, opponent: Opponent, max_episode_steps: u32) -> Self {
        let registry = fixtures::registry();
        let (corp_deck, runner_deck) = fixtures::decks_for_seed(seed);
        let opponent_seed = seed ^ 0xC0FF_EE00_C0FF_EE00;

        // `GameState::setup` needs a real `GameState` before `reset` can
        // build one — construct a throwaway one just to satisfy field
        // initialization, immediately overwritten by `reset` below.
        let (placeholder_state, _events) =
            GameState::setup(&corp_deck, &runner_deck, &registry, seed).expect("fixtures decks are legal by construction");

        let mut env = NetrunnerEnv {
            session: build_session(
                placeholder_state,
                registry.clone(),
                agent_side,
                opponent.build(agent_side.other(), opponent_seed),
            ),
            registry,
            corp_deck,
            runner_deck,
            agent_side,
            opponent_kind: opponent,
            opponent_seed,
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
            // The matchup is a function of the seed, so a reseeded episode
            // gets the pairing that seed selects rather than staying pinned
            // to whichever one construction happened to pick.
            let (corp_deck, runner_deck) = fixtures::decks_for_seed(seed);
            self.corp_deck = corp_deck;
            self.runner_deck = runner_deck;
        }
        let opponent = self.opponent_kind.build(self.agent_side.other(), self.opponent_seed);
        let (state, _events) = GameState::setup(&self.corp_deck, &self.runner_deck, &self.registry, self.seed)
            .expect("fixtures decks are legal by construction");
        // A fresh `Session` per episode, which also resets its step budget.
        self.session = build_session(state, self.registry.clone(), self.agent_side, opponent);
        self.steps_this_episode = 0;

        self.fast_forward_opponent();
        (self.observation(), self.action_mask())
    }

    pub fn action_mask(&self) -> Vec<bool> {
        get_action_mask(self.session.state(), &self.registry)
    }

    pub fn observation(&self) -> Vec<f32> {
        encode_observation(self.session.state(), &self.registry, self.agent_side)
    }

    pub fn is_over(&self) -> bool {
        matches!(self.session.state().phase, GamePhase::GameOver(_))
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

        let value_before = evaluate_state(self.session.state(), self.agent_side, self.session.registry());

        match self.submit_index(index) {
            Ok(()) => {
                self.steps_this_episode += 1;
                self.fast_forward_opponent();

                let value_after = evaluate_state(self.session.state(), self.agent_side, self.session.registry());
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

    /// Applies one `ActionSpace` index as the agent's own action.
    ///
    /// Deliberately does **not** consult `current_actor` first, matching
    /// the behaviour this replaced: `get_action_mask` is side-agnostic (see
    /// `legal_actions`' doc comment — `RezIce` is legal for the Corp during
    /// a Runner-priority window), so the mask this env hands Python
    /// legitimately contains indices the engine will accept even when the
    /// *other* side nominally holds priority. `Session::submit` is likewise
    /// ungated for exactly this reason; re-deriving legality here would
    /// start rejecting actions that succeed today and silently shift the
    /// training distribution.
    fn submit_index(&mut self, index: usize) -> Result<(), IndexedActionError> {
        let action =
            ActionSpace::action_at(self.session.state(), index).ok_or(IndexedActionError::NoActionAtIndex(index))?;
        self.session.submit(action).map_err(|error| match error {
            SubmitError::Rules(rules) => IndexedActionError::Rules(rules),
            // `Ended` and `NoActor` are not rules rejections, but from
            // Python's point of view they are the same thing as an index
            // that will not apply right now: state untouched, penalty,
            // episode carries on. `step_index` already reports `terminated`
            // separately, so nothing is lost by folding them in here.
            SubmitError::Ended | SubmitError::NoActor => IndexedActionError::NoActionAtIndex(index),
        })
    }

    /// Resolves every opponent decision until it is genuinely the agent's
    /// turn, the game ends, or the session stalls.
    ///
    /// This used to be a hand-rolled copy of the match loop with its own
    /// `1_000`-step budget that reset on every call — so a pathological
    /// episode was effectively unbounded. `Session::run` resolves the
    /// opponent's `Seat::Agent` decisions itself and stops at the agent's
    /// `Awaiting`, under one `MAX_STEPS` budget for the whole episode.
    fn fast_forward_opponent(&mut self) {
        self.session.run();
    }
}

/// The env's seat wiring, in one place because `new` and `reset` both need
/// it: the agent is `External` (Python supplies its action), the opponent
/// is an in-process `Seat::Agent` the session resolves during fast-forward.
///
/// `without_history`: an RL episode's `MatchHistory` is never read, and the
/// env runs millions of them.
fn build_session(state: GameState, registry: CardRegistry, agent_side: Side, opponent: Box<dyn BotAgent>) -> Session {
    let (corp, runner) = match agent_side {
        Side::Corp => (Seat::External, Seat::Agent(opponent)),
        Side::Runner => (Seat::Agent(opponent), Seat::External),
    };
    Session::new(state, registry, corp, runner).without_history()
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
        assert_eq!(mask, get_action_mask(env.session.state(), &env.registry));
    }

    #[test]
    fn observation_length_matches_obs_size() {
        let env = env(Side::Corp, 2);
        assert_eq!(env.observation().len(), netrunner_bots::observation::OBS_SIZE);
    }

    #[test]
    fn reset_lands_on_the_agent_sides_own_decision_or_game_over() {
        for side in [Side::Corp, Side::Runner] {
            let mut env = env(side, 3);
            let (_obs, mask) = env.reset(Some(3));
            // Equivalent to the old `current_actor(&env.state) == Some(side)`:
            // fast-forward stops exactly where the session awaits the
            // agent's own External seat.
            assert!(env.is_over() || env.session.awaiting() == Some(side));
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
