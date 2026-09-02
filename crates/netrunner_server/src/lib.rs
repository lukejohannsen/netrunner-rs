//! Authoritative match host for `netrunner_core`: `MatchSession` owns the
//! real `GameState` and only ever hands out masked `ClientView`s (via
//! `netrunner_core::view::build_client_view`) to whichever side is
//! channel-backed — an embedded `BotAgent` slot never even goes through a
//! channel. See `match_session`'s doc comment for the run loop itself.
//!
//! Local play wires up `tokio::sync::mpsc` directly (see `protocol`'s doc
//! comment) — `ClientMessage`/`ServerMessage` are plain serializable data
//! with no transport assumptions baked in. `net` carries the same types
//! over a real WebSocket for `--serve` mode's remote clients.

pub mod fixtures;
pub mod match_session;
pub mod net;
pub mod protocol;

pub use match_session::{MatchSession, PlayerSlot};
pub use protocol::{ClientMessage, GameEndReason, HistoryEntry, PublicHistoryEntry, ServerMessage};

/// Re-exported so existing `netrunner_server::classify_end_reason` callers
/// keep working; it now lives in `netrunner_session::outcome`. A caller
/// that isn't otherwise talking to a server should depend on
/// `netrunner_session` directly instead.
pub use netrunner_session::classify_end_reason;
