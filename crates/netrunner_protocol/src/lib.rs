//! Transport-agnostic wire messages between a server's `MatchSession` and
//! one player's client — deliberately just serializable data, no transport
//! assumptions baked in, so an in-process `tokio::sync::mpsc` pair and a
//! WebSocket carry the exact same types unchanged.
//!
//! Lifted out of `netrunner_server` (Phase 4 §6 item 2) so a client can
//! speak the protocol without depending on the server: the server
//! re-exports this crate as its `protocol` module, so every
//! `netrunner_server::{protocol::,}ClientMessage` path still resolves.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use netrunner_core::decks::DeckFile;
use netrunner_core::format::NsgFormat;
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
    ///
    /// `deck` is the decklist this player brings. **A brought deck decides
    /// the seat** — its side is the side they play, so `preferred_side`
    /// must agree or be `None` — and it is checked against the daemon's
    /// format before anything else happens, refused with `ConnectRejected`
    /// if either validator objects. `None` is the old behaviour: the daemon
    /// deals that seat a deck, pinned or rotating. `serde(default)` so a
    /// client built before this still connects. A daemon built before it
    /// ignores the field and deals as it always did, which is why a client
    /// that brought a deck checks `MatchJoined`'s deck ids.
    ///
    /// **There is no picking an opponent** (decided 26 September 2026):
    /// a lobby pairs whoever is waiting, and a room is how two people who
    /// already know each other meet. Listing the waiters and challenging
    /// one was proposed and declined.
    Connect {
        player_name: String,
        preferred_side: Option<Side>,
        #[serde(default)]
        room: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deck: Option<Box<DeckFile>>,
        /// The lobby: players are paired only with players in the same
        /// format (and room), and a brought deck is checked against it.
        /// `None` is the daemon's first format, which is also what a
        /// client built before lobbies gets. A format the daemon does not
        /// offer is refused with `ConnectRejected`, naming the ones it
        /// does.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<NsgFormat>,
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
    /// Watch a running match (an id from `MatchList`) from the
    /// `Viewer::Spectator` perspective: the intersection of what the two
    /// players see, and nothing to submit. Answered with `Spectating` and
    /// then every `StateUpdate`/`ActionLog`/`DecisionClock`/`GameEnded`
    /// the seats get, masked for a spectator; or `ConnectRejected` when no
    /// live match has that id. Anything a spectator sends afterwards is
    /// ignored — it holds no seat.
    Spectate { match_id: Uuid },
    SubmitAction(PlayerAction),
    Surrender,
    /// Attach to the server with nothing but a name, and stay attached:
    /// the lobbies are browsed, joined and left, and a game is looked for
    /// and played, all on this one connection, with no deck until a game
    /// is looked for (Phase 4 §7, decided 26 September 2026). Answered
    /// with `Attached`. `Connect` remains the one-shot way in for a client
    /// that wants a single game: it attaches, joins a lobby and looks for
    /// a game in one message, and the socket closes when the game does.
    Attach { player_name: String },
    /// The open lobbies, answered with `Lobbies`. A closed lobby is never
    /// listed; it is joined by its id.
    ListLobbies,
    /// Make a lobby and join it: open (listed) or closed (joined by its
    /// id, and by `password` when one is set). Answered with
    /// `LobbyJoined`, whose id is the one to share, or `LobbyRefused`.
    CreateLobby { name: String, format: NsgFormat, closed: bool, password: Option<String> },
    /// Join a lobby by its id — a listed one, or a closed one given by its
    /// maker — leaving the one this connection was in. A password is
    /// asked of a lobby made with one. Answered with `LobbyJoined` or
    /// `LobbyRefused`.
    JoinLobby { lobby: String, password: Option<String> },
    LeaveLobby,
    /// Look for a game in the lobby joined, with the deck or decks the
    /// chair needs. Answered with `Queued`, then `MatchJoined` when the
    /// lobby pairs this player; or `SeekRefused`. A second seek while one
    /// is open is refused: `CancelSeek` first, and the new one is a new
    /// place in the queue. The decks go no further than the server, which
    /// deals from them and tells nobody else what they are.
    Seek { chair: Chair },
    /// Stop looking. Answered with `SeekCancelled`.
    CancelSeek,
}

/// Which chair a player looks for a game in, and the deck it needs: one
/// deck for a chair chosen, one for each side for a random one, whose
/// side the server picks at pairing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Chair {
    Corp(Box<DeckFile>),
    Runner(Box<DeckFile>),
    Random { corp: Box<DeckFile>, runner: Box<DeckFile> },
}

/// A lobby as `Lobbies` and `LobbyJoined` report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyInfo {
    /// What `JoinLobby` names it by: a format's name for the server's own
    /// lobby, a short code for a player's.
    pub id: String,
    pub name: String,
    pub format: NsgFormat,
    /// The server's own lobby for its format, which never goes away.
    pub permanent: bool,
    /// Not listed; joined by its id.
    pub closed: bool,
    /// Joining asks for a password.
    pub password: bool,
    /// Attached connections in the lobby.
    pub players: usize,
    /// Of them, how many are looking for a game.
    pub seeking: usize,
}

/// One running match as `ListMatches` reports it: the players' names and
/// the lobby. The seed is never on the wire, because it reproduces the
/// order of R&D.
///
/// **No decks** (Phase 4 §7 stage 2, 26 September 2026). The two deck ids
/// used to be here, so that a list could say what each match was while
/// the daemon rotated the sample pool. But a saved deck's id is a slug of
/// the name its builder gave it, so the list told anyone who asked what
/// every player had built and called it — and a spectator is one message
/// from telling a player. The identities are public the moment a game is
/// watched; the lists are nobody's but their players'. A client built
/// when the ids were here reads them as empty (`serde(default)`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSummary {
    pub match_id: Uuid,
    pub corp: String,
    pub runner: String,
    pub started_secs_ago: u64,
    /// The lobby the match was paired in; `None` from a daemon built
    /// before lobbies.
    #[serde(default)]
    pub format: Option<NsgFormat>,
}

/// One of a daemon's lobbies as `MatchList` reports it: a format it
/// pairs players in, and how many wait in its public queue (a named
/// room's waiters are counted in `MatchList::waiting_in_lobby` only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lobby {
    pub format: NsgFormat,
    pub waiting: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// The seat is taken. `session_token` is what `ClientMessage::Resume`
    /// presents to take it back after a dropped connection; it is per
    /// *seat*, not per match, so one player's token never reseats the
    /// other. Sent again, unchanged, on a successful resume.
    ///
    /// `corp_deck`/`runner_deck` name the deck each side plays
    /// (`decks::DeckFile::id`) — **but only the seat's own.** The other
    /// side's is always empty: a deck is its player's secret, and what
    /// the opponent learns of it is what the rules reveal (Phase 4 §7
    /// stage 2). The seat's own is there so a player knows what the host
    /// dealt them, and so a client that brought a deck can tell a host
    /// that predates bringing one dealt it something else.
    /// `#[serde(default)]` so an older client still parses the message.
    MatchJoined {
        match_id: Uuid,
        assigned_side: Side,
        session_token: Uuid,
        #[serde(default)]
        corp_deck: String,
        #[serde(default)]
        runner_deck: String,
    },
    /// The reply to `Spectate`, before the first spectator `StateUpdate`.
    /// Its own variant rather than a `MatchJoined` with no side: that
    /// message's `assigned_side` and `session_token` are pinned by every
    /// reconnect test and `netrunner_client::connection`, and a spectator has
    /// neither. There is no token because there is nothing to resume —
    /// `Spectate` again is the whole of reconnecting.
    Spectating { match_id: Uuid },
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
    ///
    /// `lobbies` is every format the daemon pairs players in, its first
    /// the one a `Connect` naming none joins; empty from a daemon built
    /// before lobbies.
    MatchList {
        matches: Vec<MatchSummary>,
        waiting_in_lobby: usize,
        max_matches: Option<usize>,
        #[serde(default)]
        lobbies: Vec<Lobby>,
    },
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
    /// `side` has `remaining` to answer the decision it was just offered,
    /// or forfeits with `GameEndReason::TimedOut`. Sent to both seats when
    /// a clock starts, and again to a seat that reattaches mid-decision
    /// with what is left; never sent when the host runs without a clock,
    /// so a clock-less match's message sequence is exactly what it was.
    /// A message rather than a `ClientView` field: the view is the
    /// engine's, and `netrunner_core` knows no wall clock. One message per
    /// decision rather than ticks: the client can count down by itself.
    DecisionClock { side: Side, remaining: Duration },
    GameEnded { winner: Side, reason: GameEndReason },
    /// The reply to `Attach`: attached, and the open lobbies.
    Attached { lobbies: Vec<LobbyInfo> },
    /// The reply to `ListLobbies`.
    Lobbies { lobbies: Vec<LobbyInfo> },
    /// In the lobby now.
    LobbyJoined { lobby: LobbyInfo },
    /// `CreateLobby` or `JoinLobby` refused: no such lobby, the wrong
    /// password, a format the server does not offer.
    LobbyRefused { reason: String },
    /// Out of every lobby, after `LeaveLobby`.
    LobbyLeft,
    /// `Seek` refused: not in a lobby, a deck for the wrong side, a deck
    /// the lobby's format does not allow, already seeking or playing.
    SeekRefused { reason: String },
    /// No longer looking, after `CancelSeek`.
    SeekCancelled,
    /// The game this attached connection was playing has ended and it is
    /// back in its lobby, free to look for the next one.
    BackInLobby { lobby: Option<LobbyInfo> },
}
