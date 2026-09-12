//! `netrunner_cli diag`: measurements whose product is a number rather
//! than a behaviour change.
//!
//! Each submodule answers one open ROADMAP question and stays in the tree
//! so the number can be re-taken later — the `diag/` branch convention
//! from the repo's git hygiene rules, in code form. They share nothing
//! but that intent: a diagnostic is allowed to reach into the engine's
//! authoritative `GameState` in ways a client never may, because it is
//! not a client.

pub mod leaf_sensitivity;
pub mod rez_rate;
