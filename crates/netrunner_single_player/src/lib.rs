//! The index-based seat adapter over `netrunner_session`: drives a local
//! human-vs-bot (or bot-vs-bot) match synchronously against
//! `netrunner_bots::Agent`s, which pick a fixed `ActionSpace` index rather
//! than a `PlayerAction`.
//!
//! The match loop, the step budget and the action log all live in
//! `netrunner_session::Session` now; `MatchHistory`/`HistoryEntry`/
//! `MAX_STEPS` are re-exported here so existing callers keep working. See
//! `session`'s doc comment for why the raw-`GameState` seat shape survives
//! in this one crate.

pub mod session;

pub use netrunner_session::{HistoryEntry, MatchHistory, MAX_STEPS};
pub use session::SinglePlayerSession;
