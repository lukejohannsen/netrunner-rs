//! The shared match driver every seat type is pumped through: one
//! `current_actor` → action → `apply_action` → `GameOver` loop, one
//! `MAX_STEPS`, one `MatchHistory`, and a seat interface that speaks only
//! masked `ClientView`s.
//!
//! Depends on `netrunner_core` and `netrunner_bots` and nothing else — no
//! async runtime, no transport, no terminal. `netrunner_server` pumps a
//! `Session` inside `tokio`, `netrunner_cli` pumps one on the render
//! thread, `netrunner_gym` pumps one from Python, and they share this
//! single definition of what a match *is*. See `session` for why the loop
//! is shaped as a step function rather than a blocking `run`.

pub mod coverage;
pub mod history;
pub mod lesson;
pub mod outcome;
pub mod session;

pub use coverage::Coverage;
pub use history::{HistoryEntry, MatchHistory, PublicHistoryEntry};
pub use lesson::{LessonError, LessonSession, LessonStep};
pub use outcome::{classify_end_reason, GameEndReason};
pub use session::{Seat, Session, SessionStep, StallReason, SubmitError, MAX_STEPS};
