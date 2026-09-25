//! A match on a server, played over a WebSocket: the one driver around
//! [`connection::Connection`](crate::connection), and the only code in the
//! client that touches a socket for a game (Phase 4 §6 item 2).
//!
//! The driver is a tokio task that does exactly what the machine says —
//! dial, send, read, sleep until the deadline it names — and reports
//! through channels a caller only ever `try_recv`s, so a terminal's render
//! tick and Bevy's main thread consume it the same way and neither blocks.
//! Everything that is a *decision* (when to retry, what to say on
//! reconnect, when to give up) is the machine's and is tested there.
//!
//! **Two stages, like the handshake.** [`spawn`] returns a [`Connecting`]
//! that reports the lobby and ends in a [`Joined`]: the place at the match,
//! the channel pair to play it through, and the [`Link`] to show. A resume
//! after a drop happens *under* the channel pair — the same `rx` keeps
//! delivering, the first message after it is the server's fresh view, and
//! only `link` says anything happened. The terminal client's blocking
//! `Reconnector`, and the channel pair it swapped in, are what this
//! replaces.
//!
//! **Leaving closes the socket properly.** Dropping the [`Connecting`], or
//! every `tx` of a [`Joined`], ends the driver with a WebSocket `Close`, so
//! a player who stops waiting leaves the daemon's lobby rather than sitting
//! in it to be paired after they have gone. That is why the task is never
//! aborted from here.

use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

use netrunner_core::decks::DeckFile;
use netrunner_core::rules::{Side, Viewer};
use netrunner_protocol::{ClientMessage, MatchSummary, ServerMessage};

use crate::connection::{Closed, Connection, ConnectionError, Event, Goal, Link};

/// A place at a match: the perspective it was given, the seat's token
/// (`None` for a spectator), the decks `MatchJoined` named, and the channel
/// pair to play or watch it through. `rx` closes only when the driver has
/// stopped for good; while it reconnects, `link` says so and `rx` waits.
pub struct Joined {
    pub viewer: Viewer,
    pub session_token: Option<Uuid>,
    /// Corp then Runner; empty for a spectator. A player who brought a
    /// deck compares against these, because a daemon older than
    /// `Connect::deck` ignores it and deals.
    pub decks: (String, String),
    /// Messages to the server. Sent while the link is down, they are
    /// dropped: an action chosen from a view before a drop may be stale by
    /// the time the seat is back.
    pub tx: mpsc::UnboundedSender<ClientMessage>,
    /// Messages about the match: views, log entries, clocks, rejections,
    /// the end.
    pub rx: mpsc::UnboundedReceiver<ServerMessage>,
    pub link: watch::Receiver<Link>,
}

/// What a connection in progress has to say.
pub enum ConnectEvent {
    /// In the lobby at this position.
    Queued(usize),
    /// The lobby place's connection dropped and is being taken back, or is
    /// back (`Link::Up`).
    Link(Link),
    Joined(Box<Joined>),
    Failed(ConnectionError),
}

/// What the driver tells a `Connecting`, before the channel pair exists
/// for anyone else.
enum Setup {
    Queued(usize),
    Link(Link),
    Joined { viewer: Viewer, session_token: Option<Uuid>, decks: (String, String) },
    Failed(ConnectionError),
}

/// A connection running in the background, until it has a place.
pub struct Connecting {
    setup: mpsc::UnboundedReceiver<Setup>,
    /// The parts a `Joined` is made of, held here until then so that
    /// dropping this before a place leaves the driver with no `tx` — which
    /// is how it knows to close.
    parts: Option<(mpsc::UnboundedSender<ClientMessage>, mpsc::UnboundedReceiver<ServerMessage>, watch::Receiver<Link>)>,
}

impl Connecting {
    /// The next thing to report, without waiting.
    pub fn poll(&mut self) -> Option<ConnectEvent> {
        let setup = self.setup.try_recv().ok()?;
        Some(self.report(setup))
    }

    /// The next thing to report, waiting for it. `None` once there is
    /// nothing more.
    pub async fn next(&mut self) -> Option<ConnectEvent> {
        let setup = self.setup.recv().await?;
        Some(self.report(setup))
    }

    fn report(&mut self, setup: Setup) -> ConnectEvent {
        match setup {
            Setup::Queued(position) => ConnectEvent::Queued(position),
            Setup::Link(link) => ConnectEvent::Link(link),
            Setup::Failed(error) => ConnectEvent::Failed(error),
            Setup::Joined { viewer, session_token, decks } => match self.parts.take() {
                Some((tx, rx, link)) => ConnectEvent::Joined(Box::new(Joined { viewer, session_token, decks, tx, rx, link })),
                None => ConnectEvent::Failed(ConnectionError::ClosedBeforeSeat),
            },
        }
    }
}

/// The `Connect` a player sends: their name, a seat preference, a room,
/// and the deck they bring, whose side is then their seat.
pub fn connect_message(player_name: &str, preferred_side: Option<Side>, room: Option<String>, deck: Option<DeckFile>) -> ClientMessage {
    ClientMessage::Connect { player_name: player_name.to_string(), preferred_side, room, deck: deck.map(Box::new) }
}

/// Starts a connection to `url` for `goal`. Must be called inside a tokio
/// runtime; the caller polls the result between frames.
pub fn spawn(url: String, goal: Goal) -> Connecting {
    let (setup_tx, setup) = mpsc::unbounded_channel();
    let (tx, commands) = mpsc::unbounded_channel();
    let (messages, rx) = mpsc::unbounded_channel();
    let (link_tx, link) = watch::channel(Link::Up);
    let connection = Connection::new(goal, Instant::now());
    tokio::spawn(drive(url, connection, commands, setup_tx, messages, link_tx));
    Connecting { setup, parts: Some((tx, rx, link)) }
}

/// `spawn`, awaited to a place: for a caller with nothing to draw while it
/// waits (`--mode remote` connects before the terminal is taken).
pub async fn connect(url: &str, goal: Goal, mut on_queued: impl FnMut(usize)) -> Result<Joined, ConnectionError> {
    let mut connecting = spawn(url.to_string(), goal);
    loop {
        match connecting.next().await {
            Some(ConnectEvent::Joined(joined)) => return Ok(*joined),
            Some(ConnectEvent::Queued(position)) => on_queued(position),
            Some(ConnectEvent::Link(_)) => {}
            Some(ConnectEvent::Failed(error)) => return Err(error),
            None => return Err(ConnectionError::ClosedBeforeSeat),
        }
    }
}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type Dial = Pin<Box<dyn Future<Output = Result<Socket, String>> + Send>>;

async fn drive(
    url: String,
    mut conn: Connection,
    mut commands: mpsc::UnboundedReceiver<ClientMessage>,
    setup: mpsc::UnboundedSender<Setup>,
    messages: mpsc::UnboundedSender<ServerMessage>,
    link: watch::Sender<Link>,
) {
    let mut socket: Option<Socket> = None;
    let mut dial: Option<Dial> = None;
    let mut joined = false;
    let mut commands_open = true;
    loop {
        if conn.poll_dial() {
            socket = None;
            let url = url.clone();
            dial = Some(Box::pin(async move {
                tokio_tungstenite::connect_async(url).await.map(|(socket, _)| socket).map_err(|error| error.to_string())
            }));
        }
        while let Some(message) = conn.poll_transmit() {
            let Some(open) = &mut socket else { continue };
            let json = serde_json::to_string(&message).expect("a ClientMessage serializes");
            if open.send(WsMessage::Text(json)).await.is_err() {
                socket = None;
                conn.on_closed(Closed::Dropped, Instant::now());
            }
        }
        while let Some(event) = conn.poll_event() {
            match event {
                Event::Queued(position) => {
                    let _ = setup.send(Setup::Queued(position));
                }
                Event::Joined { viewer, session_token, decks } => {
                    joined = true;
                    let _ = setup.send(Setup::Joined { viewer, session_token, decks });
                }
                Event::Message(message) => {
                    let _ = messages.send(message);
                }
                Event::Link(state) => {
                    if !joined {
                        let _ = setup.send(match &state {
                            Link::Down(error) => Setup::Failed(error.clone()),
                            other => Setup::Link(other.clone()),
                        });
                    }
                    link.send_replace(state);
                }
            }
        }
        if !conn.wants_transport() {
            dial = None;
            if let Some(mut open) = socket.take() {
                let _ = open.close(None).await;
            }
        }
        if conn.is_done() {
            return;
        }
        let deadline = conn.poll_timeout().map(tokio::time::Instant::from_std);
        tokio::select! {
            result = async { dial.as_mut().expect("guarded by is_some").await }, if dial.is_some() => {
                dial = None;
                match result {
                    Ok(open) => {
                        socket = Some(open);
                        conn.on_open(Instant::now());
                    }
                    Err(error) => conn.on_closed(Closed::DialFailed(error), Instant::now()),
                }
            }
            frame = async { socket.as_mut().expect("guarded by is_some").next().await }, if socket.is_some() => match frame {
                Some(Ok(WsMessage::Text(text))) => {
                    if let Ok(message) = serde_json::from_str::<ServerMessage>(&text) {
                        conn.on_message(message, Instant::now());
                    }
                }
                // A ping, pong or binary frame is not the end of anything.
                Some(Ok(WsMessage::Close(_)) | Err(_)) | None => {
                    socket = None;
                    conn.on_closed(Closed::Dropped, Instant::now());
                }
                Some(Ok(_)) => {}
            },
            command = commands.recv(), if commands_open => match command {
                Some(message) => {
                    conn.submit(message);
                }
                None => {
                    commands_open = false;
                    conn.close();
                }
            },
            () = tokio::time::sleep_until(deadline.unwrap_or_else(tokio::time::Instant::now)), if deadline.is_some() => {
                conn.on_timeout(Instant::now());
            }
            else => return,
        }
    }
}

/// One `ListMatches` round trip on its own socket, closed afterwards:
/// the running matches, how many wait in the lobby, and the match cap.
pub async fn list_matches(url: &str) -> Result<(Vec<MatchSummary>, usize, Option<usize>), ConnectionError> {
    let transport = |error: tokio_tungstenite::tungstenite::Error| ConnectionError::Transport(error.to_string());
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.map_err(transport)?;
    let hello = serde_json::to_string(&ClientMessage::ListMatches).expect("a ClientMessage serializes");
    socket.send(WsMessage::Text(hello)).await.map_err(transport)?;
    let reply = loop {
        match socket.next().await {
            Some(Ok(WsMessage::Text(text))) => {
                if let Ok(ServerMessage::MatchList { matches, waiting_in_lobby, max_matches }) = serde_json::from_str(&text) {
                    break (matches, waiting_in_lobby, max_matches);
                }
            }
            Some(Ok(_)) => continue,
            Some(Err(error)) => return Err(transport(error)),
            None => return Err(ConnectionError::ClosedBeforeSeat),
        }
    };
    let _ = socket.close(None).await;
    Ok(reply)
}

/// The driver against a real `netrunner_server` on an ephemeral port, with
/// a proxy between them that the test can cut — which is how a socket
/// drops in the middle of a match without the client or the server being
/// told.
#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;
    use netrunner_core::rules::PlayerAction;
    use netrunner_server::serve::{ServeBotKind, ServeOptions, Server};

    async fn start_server() -> std::net::SocketAddr {
        let options = ServeOptions { bot_runner: ServeBotKind::Heuristic, seed: Some(1), ..ServeOptions::default() };
        let server = Server::bind("127.0.0.1:0", options).await.unwrap();
        let addr = server.local_addr().unwrap();
        tokio::spawn(server.run());
        addr
    }

    /// Forwards every connection to `upstream`; `cut` drops them all.
    #[derive(Clone, Default)]
    struct Proxy {
        pipes: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    }

    impl Proxy {
        async fn start(upstream: std::net::SocketAddr) -> (Proxy, String) {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("ws://{}", listener.local_addr().unwrap());
            let proxy = Proxy::default();
            let pipes = proxy.pipes.clone();
            tokio::spawn(async move {
                while let Ok((mut client, _)) = listener.accept().await {
                    let pipe = tokio::spawn(async move {
                        if let Ok(mut server) = tokio::net::TcpStream::connect(upstream).await {
                            let _ = tokio::io::copy_bidirectional(&mut client, &mut server).await;
                        }
                    });
                    pipes.lock().unwrap().push(pipe);
                }
            });
            (proxy, url)
        }

        fn cut(&self) {
            for pipe in self.pipes.lock().unwrap().drain(..) {
                pipe.abort();
            }
        }
    }

    async fn next_view(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) {
        loop {
            match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("the server answers") {
                Some(ServerMessage::StateUpdate(_)) => return,
                Some(_) => continue,
                None => panic!("the channel closed before a StateUpdate"),
            }
        }
    }

    async fn link_is(link: &mut watch::Receiver<Link>, wanted: impl Fn(&Link) -> bool) {
        tokio::time::timeout(Duration::from_secs(10), link.wait_for(|state| wanted(state))).await.expect("the link settles").unwrap();
    }

    fn corp() -> Goal {
        Goal::Play(connect_message("tester", Some(Side::Corp), None, None))
    }

    #[tokio::test]
    async fn a_seat_plays_through_the_driver() {
        let url = format!("ws://{}", start_server().await);
        let mut joined = connect(&url, corp(), |_| {}).await.unwrap();
        assert_eq!(joined.viewer, Viewer::Player(Side::Corp));
        assert!(joined.session_token.is_some());
        next_view(&mut joined.rx).await;
        joined.tx.send(ClientMessage::SubmitAction(PlayerAction::KeepHand)).unwrap();
        next_view(&mut joined.rx).await;
        assert_eq!(*joined.link.borrow(), Link::Up);
    }

    /// The socket is cut mid-match: the link says so, the seat is taken
    /// back with its token under the same channel pair, and the next view
    /// arrives on the `rx` the screen already holds.
    #[tokio::test]
    async fn a_cut_socket_is_resumed_under_the_same_channels() {
        let (proxy, url) = Proxy::start(start_server().await).await;
        let mut joined = connect(&url, corp(), |_| {}).await.unwrap();
        next_view(&mut joined.rx).await;

        proxy.cut();
        link_is(&mut joined.link, |state| matches!(state, Link::Reconnecting { .. })).await;
        link_is(&mut joined.link, |state| *state == Link::Up).await;
        next_view(&mut joined.rx).await;
        joined.tx.send(ClientMessage::SubmitAction(PlayerAction::KeepHand)).unwrap();
        next_view(&mut joined.rx).await;
    }

    #[tokio::test]
    async fn an_address_with_nothing_there_fails_the_first_connection() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        drop(listener);
        assert!(matches!(connect(&url, corp(), |_| {}).await, Err(ConnectionError::Transport(_))));
    }

    #[tokio::test]
    async fn a_match_that_is_not_running_refuses_a_spectator() {
        let url = format!("ws://{}", start_server().await);
        let result = connect(&url, Goal::Watch { match_id: Uuid::new_v4() }, |_| {}).await;
        assert!(matches!(result, Err(ConnectionError::Rejected(_))), "{:?}", result.err());
    }
}
