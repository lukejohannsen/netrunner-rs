//! Transport-agnostic wire messages between a `MatchSession` and one
//! player's client — deliberately just serializable data, no transport
//! assumptions baked in, so an in-process `tokio::sync::mpsc` pair (what
//! this crate actually wires up today) and a future WebSocket transport can
//! both carry the exact same types unchanged.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use netrunner_core::rules::{PlayerAction, Side};
use netrunner_core::view::ClientView;

/// Re-exported, not defined here: both live in `netrunner_session` beside
/// the driver that produces them. `GameEndReason` was never a transport
/// concern — `netrunner_cli` used to depend on this whole crate purely to
/// call `classify_end_reason` on its *offline* local path. Re-exporting
/// keeps every existing `netrunner_server::{protocol::,}GameEndReason` path
/// resolving, and the wire format is unaffected: moving a type does not
/// change its serde representation.
pub use netrunner_session::{GameEndReason, HistoryEntry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    Connect { player_name: String, preferred_side: Option<Side> },
    SubmitAction(PlayerAction),
    Surrender,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    MatchJoined { match_id: Uuid, assigned_side: Side },
    /// Boxed — `ClientView` is by far the largest variant here, and this
    /// enum is passed around/cloned as a whole regardless of which variant
    /// is active.
    StateUpdate(Box<ClientView>),
    /// One resolved action, sent immediately after the `StateUpdate` it
    /// produced, so a client can render a running game log.
    ///
    /// One message per action rather than the whole log each time: a
    /// `StateUpdate` is already per-action and the client already drains
    /// messages in a loop, so resending a growing log would cost O(n²)
    /// bytes over a match. Boxed for the same reason `StateUpdate` is — a
    /// `HistoryEntry` carries the action's full `Vec<GameEvent>`.
    ActionLog(Box<HistoryEntry>),
    ActionRejected { reason: String },
    GameEnded { winner: Side, reason: GameEndReason },
}
