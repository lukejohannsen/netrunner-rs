//! Transport-agnostic wire messages between a `MatchSession` and one
//! player's client — deliberately just serializable data, no transport
//! assumptions baked in, so an in-process `tokio::sync::mpsc` pair (what
//! this crate actually wires up today) and a future WebSocket transport can
//! both carry the exact same types unchanged.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use netrunner_core::rules::{PlayerAction, Side};
use netrunner_core::view::ClientView;

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
    ActionRejected { reason: String },
    GameEnded { winner: Side, reason: GameEndReason },
}

/// Why a match ended. Not tracked as a distinct concept anywhere in
/// `netrunner_core` (`GameEvent::GameOver { winner }` doesn't say why), so
/// `MatchSession` derives this heuristically from the trailing `GameEvent`s
/// of whichever `apply_action` call produced `GameOver` — presentation
/// logic, not a core engine capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameEndReason {
    AgendaThreshold,
    Flatline,
    Deckout,
    Surrender,
}
