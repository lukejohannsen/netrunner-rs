//! Automated `netrunner_core` players: a `BotAgent` trait plus baseline
//! `RandomAgent`, `HeuristicAgent`, and `MctsAgent` implementations.
//!
//! **Phase 2 / masked view:** every agent operates only on a per-side
//! `netrunner_core::view::ClientView` — never the raw `GameState` — matching
//! the masking `netrunner_server`'s `MatchSession` enforces for real
//! clients. `HeuristicAgent`/`MctsAgent` still need a concrete `GameState`
//! to run `apply_action`-based lookahead/rollouts, which a masked view
//! can't provide alone; `determinize` samples one, consistent with
//! everything the view actually reveals, for them to search against.
//! `MctsAgent` resampling independently per parallel tree is what makes it
//! genuine (if basic) Information Set MCTS — see its own doc comment for
//! the known simplifications in what gets sampled.

pub mod agent;
pub mod determinize;
pub mod eval;
pub mod heuristic;
pub mod mcts;
pub mod random;

pub use agent::BotAgent;
pub use determinize::determinize;
pub use eval::evaluate_state;
pub use heuristic::HeuristicAgent;
pub use mcts::MctsAgent;
pub use random::RandomAgent;
