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
pub use netrunner_core::rules::{ConcealedAction, PublicAction};
pub use netrunner_session::{GameEndReason, HistoryEntry, PublicHistoryEntry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Ask for a seat. `room` narrows who this player may be paired with
    /// under a human-vs-human daemon: `None` is the public queue, a name
    /// pairs only with the same name. `serde(default)` so a client built
    /// before rooms existed still connects.
    Connect {
        player_name: String,
        preferred_side: Option<Side>,
        #[serde(default)]
        room: Option<String>,
    },
    /// Take a seat back after the socket that held it dropped. The token is
    /// the one `MatchJoined` issued for that seat, and it is the *only*
    /// credential: a seat is worth exactly what a WebSocket connection was
    /// worth before, so a 122-bit random identifier that only ever crossed
    /// this one connection is the same trust the original `Connect` had.
    /// The reply is `MatchJoined` again (same token, same side) followed by
    /// a fresh `StateUpdate`, or `ResumeRejected`. Presented while still
    /// queued in the lobby (the token `Queued` carried is the same one), it
    /// swaps the socket under the queue entry and the reply is `Queued`.
    Resume { session_token: Uuid },
    /// What the daemon is hosting. Answered with `MatchList` before any
    /// seat is taken, and the socket stays open for a `Connect` after.
    /// Sent once seated it is ignored, like a repeated `Connect`.
    ListMatches,
    SubmitAction(PlayerAction),
    Surrender,
}

/// One running match as `ListMatches` reports it. Names only: the seed is
/// never on the wire, because with the fixed decklist it reproduces the
/// order of R&D.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSummary {
    pub match_id: Uuid,
    pub corp: String,
    pub runner: String,
    pub started_secs_ago: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// The seat is taken. `session_token` is what `ClientMessage::Resume`
    /// presents to take it back after a dropped connection; it is per
    /// *seat*, not per match, so one player's token never reseats the
    /// other. Sent again, unchanged, on a successful resume.
    MatchJoined { match_id: Uuid, assigned_side: Side, session_token: Uuid },
    /// Parked in the lobby until another human arrives: `position` is how
    /// many are waiting, this player included. The token is the seat's
    /// credential *already* — `MatchJoined` will carry the same one — so
    /// a client that drops while waiting presents it with `Resume` and
    /// gets its place back with nothing new to hold. A separate lobby
    /// token was rejected: the client would hold two credentials and
    /// swap at `MatchJoined`, and its reconnect loop keys on one.
    Queued { session_token: Uuid, position: usize },
    /// A `Connect` the host will not honour — at its match limit, or
    /// setup failed — after which it closes the socket. Its own variant
    /// for the reason `ResumeRejected` is: the client is waiting for
    /// `MatchJoined` and has no action to have rejected.
    ConnectRejected { reason: String },
    /// The reply to `ListMatches`. `waiting_in_lobby` counts only waiters
    /// whose socket is still open, so a client (or a test) can poll it to
    /// see a dropped waiter go.
    MatchList { matches: Vec<MatchSummary>, waiting_in_lobby: usize, max_matches: Option<usize> },
    /// `ClientMessage::Resume` named a token the host does not hold: never
    /// issued, or its match already over — including a match that ended
    /// *because* this seat's grace period ran out. A client that missed
    /// the `GameEnded` learns it this way; the host keeps no record of
    /// finished matches, so it cannot say who won. Its own variant rather
    /// than `ActionRejected` because a resuming client is waiting for
    /// `MatchJoined` and nothing else — it has no action to have rejected.
    ResumeRejected { reason: String },
    /// Boxed — `ClientView` is by far the largest variant here, and this
    /// enum is passed around/cloned as a whole regardless of which variant
    /// is active.
    StateUpdate(Box<ClientView>),
    /// One resolved action, sent immediately after the `StateUpdate` it
    /// produced, so a client can render a running game log.
    ///
    /// **Per viewer, like `StateUpdate`.** The Corp's and the Runner's
    /// copies differ: the acting side's action and the engine's raw events
    /// name cards the other seat's view conceals (the Corp's facedown
    /// install, the HQ card the Runner just looked at), so each seat gets
    /// `Session::last_entry_for(side)` — masked by
    /// `netrunner_core::rules::masking` at the same boundary as the view.
    /// The full `HistoryEntry` never leaves the host.
    ///
    /// One message per action rather than the whole log each time: a
    /// `StateUpdate` is already per-action and the client already drains
    /// messages in a loop, so resending a growing log would cost O(n²)
    /// bytes over a match. Boxed for the same reason `StateUpdate` is — an
    /// entry carries the action's `Vec<GameEvent>`.
    ActionLog(Box<PublicHistoryEntry>),
    ActionRejected { reason: String },
    GameEnded { winner: Side, reason: GameEndReason },
}
