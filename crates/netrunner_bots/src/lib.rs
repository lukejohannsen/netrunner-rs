//! Automated `netrunner_core` players: a `BotAgent` trait plus baseline
//! `RandomAgent`, `HeuristicAgent`, and `MctsAgent` implementations.
//!
//! **Phase 1 / god-view:** every agent in this crate operates on the full,
//! unmasked `GameState` (the same access `netrunner_cli`'s TUI `App` and
//! headless self-play loop already have) rather than a per-player masked
//! view. Per `AGENTS.md`, Fog of War is a `netrunner_server`-layer concern;
//! this crate doesn't implement it, and `MctsAgent` is accordingly plain
//! (perfect-information) MCTS rather than true Information Set MCTS — see
//! its own doc comment for what a future phase would need to change.

pub mod agent;
pub mod eval;
pub mod heuristic;
pub mod mcts;
pub mod random;

pub use agent::BotAgent;
pub use eval::evaluate_state;
pub use heuristic::HeuristicAgent;
pub use mcts::MctsAgent;
pub use random::RandomAgent;
