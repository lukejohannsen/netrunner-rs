//! A synchronous, local single-player match controller for driving
//! human-vs-bot (or bot-vs-bot) Netrunner matches with no network or async
//! runtime dependency — depends only on `netrunner_core` and
//! `netrunner_bots`. See `session` for the turn loop and `history` for the
//! per-match action/event log.

pub mod history;
pub mod session;

pub use history::{HistoryEntry, MatchHistory};
pub use session::{HumanPromptDriver, PlayerDriver, SinglePlayerSession, MAX_STEPS};
