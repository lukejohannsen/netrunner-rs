//! Connects to a remote `netrunner_server --serve` daemon over WebSocket
//! and hands back the same `mpsc::UnboundedSender<ClientMessage>`/
//! `UnboundedReceiver<ServerMessage>` pair that `tui::run`'s local mode
//! gets from an in-process `MatchSession` — `App` doesn't distinguish
//! between the two.
//!
//! Deliberately its own small bridge rather than
//! `netrunner_server::net::bridge_websocket`: that helper is server-shaped
//! (reads the wire as `ClientMessage`, writes `ServerMessage`), which is
//! backwards for a client that sends `ClientMessage` and receives
//! `ServerMessage`.
//!
//! **Resuming.** `MatchJoined` carries a per-seat session token; when the
//! socket drops mid-match, `Reconnector` presents it with
//! `ClientMessage::Resume` and gets the same seat back, followed by a fresh
//! `StateUpdate`. The retry loop is paced from the *TUI's* render loop
//! (`Reconnector::try_resume` is one bounded, blocking attempt) rather than
//! run to completion here, so the board keeps drawing and `q` keeps
//! working while the client waits for the server to come back.

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

use netrunner_core::rules::{Side, Viewer};
use netrunner_server::{ClientMessage, MatchSummary, ServerMessage};

/// A place at a match on the server: the perspective it was given, the
/// token that takes a *seat* back after a drop (`None` for a spectator,
/// who reconnects by asking to spectate again), and the channel pair to
/// watch or play it through.
pub struct Joined {
    pub viewer: Viewer,
    pub session_token: Option<Uuid>,
    pub tx: mpsc::UnboundedSender<ClientMessage>,
    pub rx: mpsc::UnboundedReceiver<ServerMessage>,
}

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    /// Boxed: `tungstenite::Error` is 136 bytes, and clippy rightly
    /// objects to every `Result<Joined, _>` carrying that in its `Err`.
    #[error("connecting to the server: {0}")]
    Transport(Box<tokio_tungstenite::tungstenite::Error>),
    #[error("server closed the connection before assigning a seat")]
    ClosedBeforeSeat,
    /// The server refused `ClientMessage::Resume` — the match is over
    /// (possibly forfeited by this very seat's absence) or the token is
    /// not one it issued — or refused `Connect` outright (at its match
    /// limit). Final — no retry will change the answer.
    #[error("the server refused to resume the seat: {0}")]
    Rejected(String),
    #[error("could not reconnect within {0:?}")]
    GaveUp(Duration),
}

/// First connection: asks for a seat and waits for `MatchJoined`. Under a
/// human-vs-human daemon that can mean waiting in the lobby; `Queued` is
/// reported on stderr, which is fine because the TUI has not started yet
/// — `run_remote` connects before `ratatui::init`. Resuming a *lobby*
/// place after a drop is not attempted here: the token arrives before
/// `MatchJoined`, and the retry loop keys on a seat. Still open.
pub async fn connect_remote(url: &str, preferred_side: Option<Side>, room: Option<String>) -> Result<Joined, RemoteError> {
    open(url, ClientMessage::Connect { player_name: "CLI Player".into(), preferred_side, room }).await
}

/// Reconnection: presents the seat's token and waits for `MatchJoined`.
pub async fn resume_remote(url: &str, session_token: Uuid) -> Result<Joined, RemoteError> {
    open(url, ClientMessage::Resume { session_token }).await
}

/// Watching: asks for a match by id and waits for `Spectating`.
pub async fn spectate_remote(url: &str, match_id: Uuid) -> Result<Joined, RemoteError> {
    open(url, ClientMessage::Spectate { match_id }).await
}

/// One `ListMatches` round trip on its own socket, closed afterwards.
pub async fn list_matches(url: &str) -> Result<(Vec<MatchSummary>, usize, Option<usize>), RemoteError> {
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.map_err(|error| RemoteError::Transport(Box::new(error)))?;
    let hello = serde_json::to_string(&ClientMessage::ListMatches).expect("a ClientMessage serializes");
    socket.send(WsMessage::Text(hello)).await.map_err(|error| RemoteError::Transport(Box::new(error)))?;
    let reply = loop {
        match socket.next().await {
            Some(Ok(WsMessage::Text(text))) => {
                if let Ok(ServerMessage::MatchList { matches, waiting_in_lobby, max_matches }) = serde_json::from_str(&text) {
                    break (matches, waiting_in_lobby, max_matches);
                }
            }
            Some(Ok(_)) => continue,
            Some(Err(error)) => return Err(RemoteError::Transport(Box::new(error))),
            None => return Err(RemoteError::ClosedBeforeSeat),
        }
    };
    let _ = socket.close(None).await;
    Ok(reply)
}

/// `netrunner_cli matches`: what the daemon is hosting, one line each.
pub async fn print_matches(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (matches, waiting, cap) = list_matches(url).await?;
    match cap {
        Some(cap) => println!("{} of {cap} matches running, {waiting} waiting in the lobby", matches.len()),
        None => println!("{} matches running, {waiting} waiting in the lobby", matches.len()),
    }
    for summary in matches {
        println!("{}  {} (Corp) vs {} (Runner), started {}s ago", summary.match_id, summary.corp, summary.runner, summary.started_secs_ago);
    }
    Ok(())
}

async fn open(url: &str, hello: ClientMessage) -> Result<Joined, RemoteError> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(url).await.map_err(|error| RemoteError::Transport(Box::new(error)))?;
    let (mut ws_writer, mut ws_reader) = ws_stream.split();

    let (tx_to_server, mut rx_to_server) = mpsc::unbounded_channel::<ClientMessage>();
    let (tx_from_server, mut rx_from_server) = mpsc::unbounded_channel::<ServerMessage>();

    tokio::spawn(async move {
        while let Some(msg) = rx_to_server.recv().await {
            let Ok(json) = serde_json::to_string(&msg) else { continue };
            if ws_writer.send(WsMessage::Text(json)).await.is_err() {
                break;
            }
        }
    });

    // Ends — closing `tx_from_server`, which is how `App` learns the
    // connection is gone — on a `Close` frame, a transport error, or the
    // stream running out; other frame kinds are skipped, not fatal.
    tokio::spawn(async move {
        while let Some(frame) = ws_reader.next().await {
            match frame {
                Ok(WsMessage::Text(text)) => {
                    if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&text)
                        && tx_from_server.send(server_msg).is_err()
                    {
                        break;
                    }
                }
                Ok(WsMessage::Close(_)) | Err(_) => break,
                Ok(_) => continue,
            }
        }
    });

    tx_to_server.send(hello).map_err(|_| RemoteError::ClosedBeforeSeat)?;

    loop {
        match rx_from_server.recv().await {
            Some(ServerMessage::MatchJoined { assigned_side, session_token, .. }) => {
                return Ok(Joined {
                    viewer: Viewer::Player(assigned_side),
                    session_token: Some(session_token),
                    tx: tx_to_server,
                    rx: rx_from_server,
                });
            }
            Some(ServerMessage::Spectating { .. }) => {
                return Ok(Joined { viewer: Viewer::Spectator, session_token: None, tx: tx_to_server, rx: rx_from_server });
            }
            Some(ServerMessage::ResumeRejected { reason } | ServerMessage::ConnectRejected { reason }) => {
                return Err(RemoteError::Rejected(reason));
            }
            Some(ServerMessage::Queued { position, .. }) => {
                eprintln!("Waiting in the lobby for another player ({position} waiting)...");
            }
            Some(_) => continue,
            None => return Err(RemoteError::ClosedBeforeSeat),
        }
    }
}

/// Retries `resume_remote` from a synchronous render loop, one attempt per
/// call, giving up after `MAX_WAIT`.
pub struct Reconnector {
    url: String,
    session_token: Uuid,
    lost_at: Instant,
    next_attempt: Instant,
    attempts: u32,
}

impl Reconnector {
    /// How long the client keeps trying. Longer than the server's default
    /// grace on purpose: if the server needed this seat the moment it
    /// dropped, it forfeits at its own deadline and the next attempt gets
    /// a `Rejected` that says so — better than giving up first and never
    /// learning the outcome.
    pub const MAX_WAIT: Duration = Duration::from_secs(60);
    /// Spacing between attempts. A refused TCP connection fails instantly,
    /// so without this a down server would burn every attempt at once.
    pub const RETRY_EVERY: Duration = Duration::from_secs(1);
    /// Bound on one attempt, so a half-open socket cannot freeze the TUI.
    pub const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(3);

    pub fn new(url: String, session_token: Uuid) -> Self {
        let now = Instant::now();
        Reconnector { url, session_token, lost_at: now, next_attempt: now, attempts: 0 }
    }

    /// One attempt if one is due. `Ok(None)` means "not yet — draw a frame
    /// and call again"; `Err` is final. Blocks the calling thread for at
    /// most `ATTEMPT_TIMEOUT`, which is why it must be called from inside
    /// a multi-threaded tokio runtime (`block_in_place` parks this worker
    /// and lets the others carry on).
    pub fn try_resume(&mut self) -> Result<Option<Joined>, RemoteError> {
        let now = Instant::now();
        if now.duration_since(self.lost_at) > Self::MAX_WAIT {
            return Err(RemoteError::GaveUp(Self::MAX_WAIT));
        }
        if now < self.next_attempt {
            return Ok(None);
        }
        self.attempts += 1;
        self.next_attempt = now + Self::RETRY_EVERY;

        let url = self.url.clone();
        let token = self.session_token;
        let attempt = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(tokio::time::timeout(Self::ATTEMPT_TIMEOUT, resume_remote(&url, token)))
        });
        match attempt {
            Ok(Ok(joined)) => Ok(Some(joined)),
            Ok(Err(RemoteError::Rejected(reason))) => Err(RemoteError::Rejected(reason)),
            // Transport trouble or a slow server: try again next tick.
            Ok(Err(_)) | Err(_) => Ok(None),
        }
    }

    /// A one-line status for the header while waiting.
    pub fn status_line(&self) -> String {
        format!(
            "Connection lost — reconnecting (attempt {}, {}s of {}s)",
            self.attempts,
            self.lost_at.elapsed().as_secs(),
            Self::MAX_WAIT.as_secs()
        )
    }
}

/// The client's half of the handshake against a real `netrunner_server`
/// on an ephemeral port — the server crate's own tests drive raw sockets;
/// this one drives `connect_remote` and `Reconnector` the way the TUI does.
#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::PlayerAction;
    use netrunner_server::serve::{ServeBotKind, ServeOptions, Server};

    async fn start_server() -> String {
        let options = ServeOptions { bot_runner: ServeBotKind::Heuristic, seed: Some(1), ..ServeOptions::default() };
        let server = Server::bind("127.0.0.1:0", options).await.unwrap();
        let addr = server.local_addr().unwrap();
        tokio::spawn(server.run());
        format!("ws://{addr}")
    }

    async fn first_state_update(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) {
        loop {
            match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("the server answers") {
                Some(ServerMessage::StateUpdate(_)) => return,
                Some(_) => continue,
                None => panic!("channel closed before a StateUpdate"),
            }
        }
    }

    // `block_in_place` needs the multi-threaded runtime the binary runs on.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_reconnector_takes_the_seat_back_after_the_socket_drops() {
        let url = start_server().await;
        let joined = connect_remote(&url, Some(Side::Corp), None).await.unwrap();
        assert_eq!(joined.viewer, Viewer::Player(Side::Corp));
        let Joined { session_token, tx, mut rx, .. } = joined;
        let session_token = session_token.expect("a seat has a token");
        first_state_update(&mut rx).await;
        // The socket drops: the bridge tasks end when their channel ends.
        drop(tx);
        drop(rx);

        let mut reconnector = Reconnector::new(url, session_token);
        let resumed = loop {
            match reconnector.try_resume().expect("the seat is still held") {
                Some(joined) => break joined,
                None => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        };
        assert_eq!(resumed.viewer, Viewer::Player(Side::Corp));
        assert_eq!(resumed.session_token, Some(session_token), "the same token keeps working");
        let Joined { tx, mut rx, .. } = resumed;
        first_state_update(&mut rx).await;
        tx.send(ClientMessage::SubmitAction(PlayerAction::KeepHand)).unwrap();
        first_state_update(&mut rx).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_refused_resume_is_final() {
        let url = start_server().await;
        let mut reconnector = Reconnector::new(url, Uuid::new_v4());
        assert!(matches!(reconnector.try_resume(), Err(RemoteError::Rejected(_))));
    }
}
