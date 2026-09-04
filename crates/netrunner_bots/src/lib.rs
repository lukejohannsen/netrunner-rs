//! Automated `netrunner_core` players: a `BotAgent` trait plus baseline
//! `RandomAgent`, `HeuristicAgent`, `MctsAgent`, and `PuctAgent`
//! implementations.
//!
//! **Phase 2 / masked view:** every agent operates only on a per-side
//! `netrunner_core::view::ClientView` — never the raw `GameState` — matching
//! the masking `netrunner_server`'s `MatchSession` enforces for real
//! clients. `HeuristicAgent`/`MctsAgent`/`PuctAgent` still need a concrete
//! `GameState` to run `apply_action`-based lookahead/rollouts/search, which
//! a masked view can't provide alone; `determinize` samples one, consistent
//! with everything the view actually reveals, for them to search against.
//! `MctsAgent` resampling independently per parallel tree is what makes it
//! genuine (if basic) Information Set MCTS — see its own doc comment for
//! the known simplifications in what gets sampled.
//!
//! **Phase 3 / fixed action space:** `action_space` and `policy` build on
//! `netrunner_core::rules::{ActionSpace, get_action_mask}` — a fixed
//! `0..ActionSpace::SIZE` categorical encoding of `PlayerAction`, suited to
//! a neural-net policy head. `PuctAgent` (`puct`) is the search that
//! consumes it: PUCT over fixed indices, driven by a pluggable
//! `PolicyEvaluator` instead of `MctsAgent`'s random rollouts.

pub mod action_space;
pub mod agent;
pub mod agent_adapter;
pub mod determinize;
pub mod eval;
pub mod heuristic;
pub mod mcts;
#[cfg(feature = "onnx")]
pub mod onnx_fixture;
#[cfg(feature = "onnx")]
pub mod onnx_policy;
pub mod observation;
pub mod personality;
pub mod policy;
pub mod puct;
pub mod random;
pub mod scripted;

pub use action_space::{legal_indices, step_index, IndexedActionError};
pub use agent::BotAgent;
#[cfg(feature = "onnx")]
pub use agent_adapter::IndexedOnnxAgent;
pub use agent_adapter::{Agent, BotAgentIndexAdapter, IndexedHeuristicAgent, IndexedRandomAgent};
pub use determinize::determinize;
pub use eval::{evaluate_state, evaluate_state_with, Weights};
pub use heuristic::HeuristicAgent;
pub use mcts::MctsAgent;
#[cfg(feature = "onnx")]
pub use onnx_policy::{OnnxPolicyError, OnnxPolicyEvaluator};
pub use observation::{encode_observation, to_observation_vector, OBS_SIZE};
pub use personality::Personality;
pub use policy::{PolicyEvaluator, SplitEvaluator, UniformPolicyEvaluator};
pub use puct::{pick_action, ActionStat, CycleGuard, PuctAgent, PuctConfig, PuctSearchStats, MAX_CYCLE_WIDTH, MAX_GREEDY_REPEATS};
pub use random::RandomAgent;
pub use scripted::ScriptedAgent;
