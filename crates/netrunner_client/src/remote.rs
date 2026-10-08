//! A player's connection to a server, over a WebSocket: the one driver
//! around [`connection::Connection`](crate::connection), and the only code
//! in the client that touches a socket for a game (Phase 4 §6 item 2).
//!
//! **An address or a ticket.** What a player gives is either a server's
//! address (`ws://…`, dialled over TCP) or a host's ticket (`endpoint…`,
//! dialled over QUIC by `peer`, Phase 4 §6 item 3). Either way the
//! WebSocket is the same and so is everything above it; only the dial
//! differs, so the machine is not told which it was.
//!
//! The driver is a tokio task that does exactly what the machine says —
//! dial, send, read, sleep until the deadline it names — and reports
//! through channels a caller only ever `try_recv`s, so a terminal's render
//! tick and Bevy's main thread consume it the same way and neither blocks.
//! Everything that is a *decision* (when to retry, what to say on
//! reconnect, when to give up) is the machine's and is tested there.
//!
//! **The connection outlives the game** (Phase 4 §7 stage 4b). [`spawn`]
//! returns an [`Attached`]: the connection as a screen holds it, on which
//! lobbies are listed, made, joined and left and a game is looked for,
//! each answered as an [`AttachedEvent`]. A game is handed out as a
//! [`Joined`] — the place at the match and a channel pair of its own to
//! play it through — and when it ends the player is `BackInLobby` on the
//! same `Attached`, free to look for the next with other decks. A resume
//! after a drop happens *under* the game's channel pair — the same `rx`
//! keeps delivering: the `MatchJoined` (or `Spectating`) the server
//! answered the resume with, then its fresh view, which no action produced
//! (`connection::Connection::seated` says why the first is passed on), and
//! `link` says what happened in between.
//!
//! **One game, one connection, is still a shape** ([`Connecting`]): the
//! terminal's `--server` flag path, a hosted game's own seat and a
//! spectator each want exactly one, so [`seek`] gives the machine the
//! lobby and the chair up front — asked for as the answers come, by the
//! rule a reattach uses to put them back, so nothing depends on the handle
//! being polled — and reports the `Joined` it ends in; [`watch`] does the
//! same for a match to watch. The screens play through `Connecting` until
//! stages 4c and 4d give them the lobby browser.
//!
//! **Leaving closes the socket properly.** The driver stays while anyone
//! holds the connection — the `Attached`, or a game's `tx` — and ends
//! with a WebSocket `Close` when the last of them is dropped, so a player
//! who stops waiting leaves the daemon's lobby rather than sitting in it to
//! be paired after they have gone. That is why the task is never aborted
//! from here. **A game left before its end is conceded**: a `Joined` whose
//! `tx` is dropped while the game is live has its seat surrendered by the
//! driver, because an attached socket cannot tell the server the board was
//! closed any other way — the socket is still there.

use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

use netrunner_core::decks::DeckFile;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::{Side, Viewer};
use netrunner_protocol::{Chair, ClientMessage, LobbyInfo, MatchSummary, ServerMessage};

use crate::connection::{Closed, Connection, ConnectionError, Event, Goal, Link, Who};
use crate::identity::{Credentials, KnownServers};
use crate::peer::{self, Dialer, Ticket};

/// A place at a match: the perspective it was given, the seat's token
/// (`None` for a spectator), the decks `MatchJoined` named, and the channel
/// pair to play or watch it through. `rx` closes when the game is over and
/// the connection has let it go, or when the connection has stopped for
/// good; while it reconnects, `link` says so and `rx` waits.
pub struct Joined {
    pub viewer: Viewer,
    pub session_token: Option<Uuid>,
    /// Corp then Runner, and only the seat's own is ever filled in: a
    /// server tells nobody their opponent's deck (Phase 4 §7 stage 2).
    /// Both empty for a spectator.
    pub decks: (String, String),
    /// Messages to the server about this game. Sent while the link is
    /// down, they are dropped: an action chosen from a view before a drop
    /// may be stale by the time the seat is back. Dropping it before the
    /// game ends concedes.
    pub tx: mpsc::UnboundedSender<ClientMessage>,
    /// Messages about the match: views, log entries, clocks, rejections,
    /// the end, the receipt.
    pub rx: mpsc::UnboundedReceiver<ServerMessage>,
    pub link: watch::Receiver<Link>,
}

/// What an attached connection has to say — the machine's events, with a
/// game's place carrying its channel pair.
pub enum AttachedEvent {
    /// Attached, and the open lobbies: first, and again after a reattach.
    Attached(Vec<LobbyInfo>),
    Lobbies(Vec<LobbyInfo>),
    LobbyJoined(LobbyInfo),
    LobbyLeft,
    LobbyRefused(String),
    /// Looking for a game, at this position in the queue.
    Queued(usize),
    SeekRefused(String),
    SeekCancelled,
    /// The answer to `standing`: whether this connection proved a key,
    /// and its standing on the server's book if it has one there.
    Standing { key: Option<netrunner_identity::PublicKey>, standing: Option<netrunner_protocol::Standing> },
    /// A place at a match, with the channels to play it through.
    Joined(Box<Joined>),
    /// The game is over and the connection is in its lobby again.
    BackInLobby(Option<LobbyInfo>),
    /// The link went down, came back, or is down for good
    /// (`Link::Down`, after which nothing more comes).
    Link(Link),
}

/// A connection, as a screen holds it: what the player asks goes in, what
/// the server answers comes out, and dropping it closes the socket — once
/// no game's `tx` holds the connection either.
pub struct Attached {
    commands: mpsc::UnboundedSender<ClientMessage>,
    events: mpsc::UnboundedReceiver<AttachedEvent>,
    link: watch::Receiver<Link>,
}

impl Attached {
    /// The next thing to report, without waiting.
    pub fn poll(&mut self) -> Option<AttachedEvent> {
        self.events.try_recv().ok()
    }

    /// The next thing to report, waiting for it. `None` once there is
    /// nothing more.
    pub async fn next(&mut self) -> Option<AttachedEvent> {
        self.events.recv().await
    }

    /// The link as it stands.
    pub fn link(&self) -> Link {
        self.link.borrow().clone()
    }

    pub fn list_lobbies(&self) {
        self.send(ClientMessage::ListLobbies);
    }

    /// Make a lobby and join it; `casual` for one whose games count for
    /// nothing on a server that would otherwise rate them.
    pub fn create_lobby(&self, name: String, format: NsgFormat, closed: bool, password: Option<String>, casual: bool) {
        self.send(ClientMessage::CreateLobby { name, format, closed, password, casual });
    }

    /// Ask this connection's standing at the server, answered with
    /// `AttachedEvent::Standing`: on attaching and after every game, so
    /// the page shows the number the game just moved.
    pub fn standing(&self) {
        self.send(ClientMessage::MyStanding);
    }

    pub fn join_lobby(&self, lobby: String, password: Option<String>) {
        self.send(ClientMessage::JoinLobby { lobby, password });
    }

    pub fn leave_lobby(&self) {
        self.send(ClientMessage::LeaveLobby);
    }

    /// Look for a game in the lobby joined, in `chair` with its deck or
    /// decks.
    pub fn seek(&self, chair: Chair) {
        self.send(ClientMessage::Seek { chair });
    }

    pub fn cancel_seek(&self) {
        self.send(ClientMessage::CancelSeek);
    }

    /// The machine refuses what makes no sense where the connection is
    /// (`connection::Connection::submit`), and the server refuses the
    /// rest with a reason that comes back as an event; a send into a
    /// driver that has stopped is the `Link::Down` already reported.
    fn send(&self, message: ClientMessage) {
        let _ = self.commands.send(message);
    }
}

/// What a connection in progress has to say, until it has a place.
pub enum ConnectEvent {
    /// Looking for a game at this position.
    Queued(usize),
    /// The connection dropped and is being taken back, or is back
    /// (`Link::Up`).
    Link(Link),
    Joined(Box<Joined>),
    Failed(ConnectionError),
}

/// One game, looked for in `lobby` as `chair` by `player_name` — the
/// script [`seek`] plays over an [`Attached`]. `credentials` is the key
/// proved; `None` plays unrated.
#[derive(Debug, Clone)]
pub struct Seat {
    pub player_name: String,
    pub lobby: String,
    pub password: Option<String>,
    pub chair: Chair,
    pub credentials: Option<Box<Credentials>>,
}

/// A game looked for in `lobby` with one deck, whose side is the chair.
pub fn seat(player_name: &str, lobby: String, password: Option<String>, deck: DeckFile) -> Seat {
    let chair = match deck.side {
        Side::Corp => Chair::Corp(Box::new(deck)),
        Side::Runner => Chair::Runner(Box::new(deck)),
    };
    Seat { player_name: player_name.to_string(), lobby, password, chair, credentials: None }
}

impl Seat {
    /// The seat proving `credentials`' key, or none: `None` plays unrated.
    pub fn with_credentials(mut self, credentials: Option<Credentials>) -> Seat {
        self.credentials = credentials.map(Box::new);
        self
    }

    fn who(&self) -> Who {
        Who { player_name: self.player_name.clone(), credentials: self.credentials.clone() }
    }
}

/// A game looked for in the server's own lobby for `format`, with one deck.
pub fn seat_in_format(player_name: &str, format: NsgFormat, deck: DeckFile) -> Seat {
    seat(player_name, netrunner_protocol::format_lobby_id(format), None, deck)
}

/// A connection running in the background until it has one place: a seat
/// looked for, or a match to watch. Dropping it before the place closes
/// the socket; the `Joined` it ends in holds the connection afterwards.
pub struct Connecting {
    attached: Attached,
}

impl Connecting {
    /// The next thing to report, without waiting.
    pub fn poll(&mut self) -> Option<ConnectEvent> {
        loop {
            let event = self.attached.poll()?;
            if let Some(report) = self.step(event) {
                return Some(report);
            }
        }
    }

    /// The next thing to report, waiting for it. `None` once there is
    /// nothing more.
    pub async fn next(&mut self) -> Option<ConnectEvent> {
        loop {
            let event = self.attached.next().await?;
            if let Some(report) = self.step(event) {
                return Some(report);
            }
        }
    }

    /// What a caller waiting for one place is told: the queue, the link,
    /// the place, or why there will be none — a lobby or a seek refused is
    /// a refused connection here, as a refused `Connect` used to be.
    fn step(&mut self, event: AttachedEvent) -> Option<ConnectEvent> {
        match event {
            AttachedEvent::LobbyRefused(reason) | AttachedEvent::SeekRefused(reason) => Some(ConnectEvent::Failed(ConnectionError::Rejected(reason))),
            AttachedEvent::Queued(position) => Some(ConnectEvent::Queued(position)),
            AttachedEvent::Joined(joined) => Some(ConnectEvent::Joined(joined)),
            AttachedEvent::Link(Link::Down(error)) => Some(ConnectEvent::Failed(error)),
            AttachedEvent::Link(link) => Some(ConnectEvent::Link(link)),
            AttachedEvent::Attached(_) | AttachedEvent::LobbyJoined(_) | AttachedEvent::Lobbies(_) | AttachedEvent::LobbyLeft | AttachedEvent::SeekCancelled | AttachedEvent::BackInLobby(_) | AttachedEvent::Standing { .. } => None,
        }
    }
}

/// One game at `url`, looked for as `seat` asks: attaches, joins the
/// lobby, seeks, and reports the place. Must be called inside a tokio
/// runtime; the caller polls the result between frames.
pub fn seek(url: String, seat: Seat) -> Connecting {
    let lobby = Some((seat.lobby.clone(), seat.password.clone()));
    Connecting { attached: start(url, Goal::Attach(seat.who()), lobby, Some(seat.chair)) }
}

/// A running match at `url` to watch.
pub fn watch(url: String, match_id: Uuid) -> Connecting {
    Connecting { attached: spawn(url, Goal::Watch { match_id }) }
}

/// A connection to `url` — a server's address or a host's ticket — for
/// `goal`, started in the background. Must be called inside a tokio
/// runtime; the caller polls the result between frames.
pub fn spawn(url: String, goal: Goal) -> Attached {
    start(url, goal, None, None)
}

/// `spawn`, with a lobby and a chair the first attach asks for
/// (`connection::Connection::with_lobby_and_seek`).
fn start(url: String, goal: Goal, lobby: Option<(String, Option<String>)>, chair: Option<Chair>) -> Attached {
    let (events_tx, events) = mpsc::unbounded_channel();
    let (commands, commands_rx) = mpsc::unbounded_channel();
    let (link_tx, link) = watch::channel(Link::Up);
    let target = Target::of(url);
    let known = target.known_servers(&goal);
    let pinned = known.as_ref().and_then(|(path, address)| KnownServers::load(path).ok()?.get(address));
    let receipts = match &goal {
        Goal::Attach(who) => who.credentials.as_ref().and_then(|credentials| credentials.receipts()),
        Goal::Watch { .. } => None,
    };
    let connection = Connection::new(goal, Instant::now()).with_pinned(pinned).with_lobby_and_seek(lobby, chair);
    tokio::spawn(drive(target, connection, Kept { known, receipts }, commands_rx, events_tx, link_tx));
    Attached { commands, events, link }
}

/// `seek` or `watch`, awaited to a place: for a caller with nothing to
/// draw while it waits (`--mode remote` connects before the terminal is
/// taken).
pub async fn connect(mut connecting: Connecting, mut on_queued: impl FnMut(usize)) -> Result<Joined, ConnectionError> {
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

/// A byte stream a WebSocket runs over: a TCP socket or a QUIC stream.
trait Io: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> Io for T {}

type Socket = tokio_tungstenite::WebSocketStream<Box<dyn Io>>;
type Dial = Pin<Box<dyn Future<Output = Result<Socket, String>> + Send>>;

/// Where a connection goes, read once from what the player gave.
#[derive(Clone)]
enum Target {
    Url(String),
    Peer(Ticket),
}

impl Target {
    /// Where the server's key is remembered, and under which address:
    /// only for a server dialled by address, by a player who proves a key.
    /// A ticket names its host's key already, and a hosted game is never
    /// rated, so it remembers nothing.
    fn known_servers(&self, goal: &Goal) -> Option<(std::path::PathBuf, String)> {
        let Goal::Attach(who) = goal else { return None };
        let path = who.credentials.as_ref()?.known_servers()?;
        match self {
            Target::Url(url) => Some((path, url.clone())),
            Target::Peer(_) => None,
        }
    }

    fn of(address: String) -> Target {
        match Ticket::parse(&address) {
            Some(ticket) => Target::Peer(ticket),
            None => Target::Url(address),
        }
    }

    /// One dial: the transport, then the WebSocket over it. `dialer`
    /// keeps a ticket's endpoint between dials.
    fn dial(&self, dialer: &Dialer) -> Dial {
        match self {
            Target::Url(url) => Box::pin(dial_url(url.clone())),
            Target::Peer(ticket) => {
                let stream = dialer.dial(ticket);
                Box::pin(async move {
                    let stream: Box<dyn Io> = Box::new(stream.await?);
                    let (socket, _) = tokio_tungstenite::client_async(peer::WS_URL, stream).await.map_err(|error| error.to_string())?;
                    Ok(socket)
                })
            }
        }
    }
}

/// A WebSocket over TCP to `url`. Its own dial rather than
/// `connect_async`, because that hands back a socket type a QUIC stream
/// cannot share; what it did is what this does — resolve, connect, turn
/// Nagle off, handshake. Plain `ws://` only, as before: no TLS is built in.
async fn dial_url(url: String) -> Result<Socket, String> {
    let request = url.as_str().into_client_request().map_err(|error| error.to_string())?;
    let uri = request.uri();
    if uri.scheme_str() != Some("ws") {
        return Err(format!("{url}: only ws:// addresses are supported"));
    }
    let host = uri.host().ok_or_else(|| format!("{url}: no host"))?.trim_start_matches('[').trim_end_matches(']').to_string();
    let port = uri.port_u16().unwrap_or(80);
    let tcp = tokio::net::TcpStream::connect((host.as_str(), port)).await.map_err(|error| error.to_string())?;
    let _ = tcp.set_nodelay(true);
    let stream: Box<dyn Io> = Box::new(tcp);
    let (socket, _) = tokio_tungstenite::client_async(request, stream).await.map_err(|error| error.to_string())?;
    Ok(socket)
}

/// What the driver writes for the player: a server's key the first time
/// it is met, and every receipt a rated game ends in.
struct Kept {
    known: Option<(std::path::PathBuf, String)>,
    receipts: Option<std::path::PathBuf>,
}

/// The game under way, as the driver holds its end of the channel pair.
struct Game {
    /// The match's messages, to the `Joined`'s `rx`.
    messages: mpsc::UnboundedSender<ServerMessage>,
    /// What the `Joined`'s `tx` sends; `None` once that was dropped.
    commands: Option<mpsc::UnboundedReceiver<ClientMessage>>,
    /// A player's seat, which leaving concedes; a spectator's place is
    /// simply left.
    player: bool,
    /// `GameEnded` has passed, or the player surrendered: nothing to
    /// concede.
    over: bool,
}

/// The game's next command, or never when there is no game or its `tx`
/// has gone.
async fn from_game(game: &mut Option<Game>) -> Option<ClientMessage> {
    match game.as_mut().and_then(|game| game.commands.as_mut()) {
        Some(commands) => commands.recv().await,
        None => std::future::pending().await,
    }
}

async fn drive(
    target: Target,
    mut conn: Connection,
    kept: Kept,
    mut commands: mpsc::UnboundedReceiver<ClientMessage>,
    events: mpsc::UnboundedSender<AttachedEvent>,
    link: watch::Sender<Link>,
) {
    let dialer = Dialer::default();
    let mut socket: Option<Socket> = None;
    let mut dial: Option<Dial> = None;
    let mut game: Option<Game> = None;
    let mut commands_open = true;
    loop {
        if conn.poll_dial() {
            socket = None;
            dial = Some(target.dial(&dialer));
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
            let report = match event {
                // A failed write costs a warning next time, not this game.
                Event::ServerKey(key) => {
                    if let Some((path, address)) = &kept.known {
                        let mut servers = KnownServers::load(path).unwrap_or_default();
                        servers.insert(address, key);
                        let _ = servers.save(path);
                    }
                    continue;
                }
                Event::Joined { viewer, session_token, decks } => {
                    let (messages, rx) = mpsc::unbounded_channel();
                    let (tx, game_commands) = mpsc::unbounded_channel();
                    game = Some(Game { messages, commands: Some(game_commands), player: matches!(viewer, Viewer::Player(_)), over: false });
                    AttachedEvent::Joined(Box::new(Joined { viewer, session_token, decks, tx, rx, link: link.subscribe() }))
                }
                Event::Message(message) => {
                    if let (ServerMessage::Rated { receipt, .. }, Some(path)) = (&message, &kept.receipts) {
                        let _ = crate::identity::keep_receipt(path, receipt);
                    }
                    if let Some(game) = &mut game {
                        if matches!(message, ServerMessage::GameEnded { .. }) {
                            game.over = true;
                        }
                        let _ = game.messages.send(message);
                    }
                    continue;
                }
                // The game's channel closes with the game: its reader sees
                // the end, and nothing of the next game reaches it.
                Event::BackInLobby(lobby) => {
                    game = None;
                    AttachedEvent::BackInLobby(lobby)
                }
                Event::Link(state) => {
                    link.send_replace(state.clone());
                    AttachedEvent::Link(state)
                }
                Event::Attached(lobbies) => AttachedEvent::Attached(lobbies),
                Event::Lobbies(lobbies) => AttachedEvent::Lobbies(lobbies),
                Event::LobbyJoined(lobby) => AttachedEvent::LobbyJoined(lobby),
                Event::LobbyLeft => AttachedEvent::LobbyLeft,
                Event::LobbyRefused(reason) => AttachedEvent::LobbyRefused(reason),
                Event::Queued(position) => AttachedEvent::Queued(position),
                Event::SeekRefused(reason) => AttachedEvent::SeekRefused(reason),
                Event::SeekCancelled => AttachedEvent::SeekCancelled,
                Event::Standing { key, standing } => AttachedEvent::Standing { key, standing },
            };
            let _ = events.send(report);
        }
        // Nobody holds the connection any more: the screen has let the
        // `Attached` go and no game's `tx` is alive.
        let held_by_a_game = game.as_ref().is_some_and(|game| game.commands.is_some());
        if !commands_open && !held_by_a_game {
            conn.close();
        }
        if !conn.wants_transport() {
            dial = None;
            if let Some(mut open) = socket.take() {
                let _ = open.close(None).await;
            }
        }
        if conn.is_done() {
            dialer.close().await;
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
                None => commands_open = false,
            },
            command = from_game(&mut game) => match command {
                Some(message) => {
                    if let (ClientMessage::Surrender, Some(game)) = (&message, &mut game) {
                        game.over = true;
                    }
                    conn.submit(message);
                }
                // The board was closed on a live game: concede it, since
                // the socket stays and the server cannot tell otherwise.
                None => {
                    if let Some(game) = &mut game {
                        game.commands = None;
                        if game.player && !game.over {
                            game.over = true;
                            conn.submit(ClientMessage::Surrender);
                        }
                    }
                }
            },
            () = tokio::time::sleep_until(deadline.unwrap_or_else(tokio::time::Instant::now)), if deadline.is_some() => {
                conn.on_timeout(Instant::now());
            }
            else => {
                dialer.close().await;
                return;
            }
        }
    }
}

/// One `ListMatches` round trip on its own socket, closed afterwards:
/// the running matches, how many wait in the lobby, and the match cap.
/// `url` is an address or a ticket, as for `spawn`.
pub async fn list_matches(url: &str) -> Result<(Vec<MatchSummary>, usize, Option<usize>), ConnectionError> {
    let dialer = Dialer::default();
    let reply = list_over(Target::of(url.to_string()), &dialer).await;
    dialer.close().await;
    reply
}

async fn list_over(target: Target, dialer: &Dialer) -> Result<(Vec<MatchSummary>, usize, Option<usize>), ConnectionError> {
    let transport = |error: tokio_tungstenite::tungstenite::Error| ConnectionError::Transport(error.to_string());
    let mut socket = target.dial(dialer).await.map_err(ConnectionError::Transport)?;
    let hello = serde_json::to_string(&ClientMessage::ListMatches).expect("a ClientMessage serializes");
    socket.send(WsMessage::Text(hello)).await.map_err(transport)?;
    let reply = loop {
        match socket.next().await {
            Some(Ok(WsMessage::Text(text))) => {
                if let Ok(ServerMessage::MatchList { matches, waiting_in_lobby, max_matches, .. }) = serde_json::from_str(&text) {
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

/// This player's standing at the server at `address`: a socket of its own
/// that proves `credentials`' key and asks (`ClientMessage::MyStanding`),
/// closed afterwards. Fetched each time it is shown and never stored,
/// because the server's book is the truth (`docs/identity-and-rating.md`
/// stage 4).
///
/// **Held to the remembered key like a game is** (`KnownServers`): a
/// server at this address that proves another key is refused before this
/// player's key is proved to it, and one met for the first time is
/// remembered.
pub async fn standing(address: &str, credentials: &Credentials) -> Result<Option<netrunner_protocol::Standing>, ConnectionError> {
    let dialer = Dialer::default();
    let reply = standing_over(Target::of(address.to_string()), address, credentials, &dialer).await;
    dialer.close().await;
    reply
}

async fn standing_over(target: Target, address: &str, credentials: &Credentials, dialer: &Dialer) -> Result<Option<netrunner_protocol::Standing>, ConnectionError> {
    let transport = |error: tokio_tungstenite::tungstenite::Error| ConnectionError::Transport(error.to_string());
    let mut socket = target.dial(dialer).await.map_err(ConnectionError::Transport)?;
    let send = |message: ClientMessage| WsMessage::Text(serde_json::to_string(&message).expect("a ClientMessage serializes"));
    socket.send(send(ClientMessage::Identify { key: credentials.identity.public_key() })).await.map_err(transport)?;
    let known_path = credentials.known_servers().filter(|_| matches!(target, Target::Url(_)));
    let reply = loop {
        let message = match socket.next().await {
            Some(Ok(WsMessage::Text(text))) => match serde_json::from_str::<ServerMessage>(&text) {
                Ok(message) => message,
                Err(_) => continue,
            },
            Some(Ok(_)) => continue,
            Some(Err(error)) => return Err(transport(error)),
            None => return Err(ConnectionError::ClosedBeforeSeat),
        };
        match message {
            ServerMessage::Challenge { nonce, server_key, lasting } => {
                if let (true, Some(path)) = (lasting, &known_path) {
                    let mut known = KnownServers::load(path).unwrap_or_default();
                    match known.get(address) {
                        Some(remembered) if remembered != server_key => return Err(ConnectionError::ServerKeyChanged { remembered, found: server_key }),
                        Some(_) => {}
                        None => {
                            known.insert(address, server_key);
                            let _ = known.save(path);
                        }
                    }
                }
                socket.send(send(ClientMessage::Prove { signature: credentials.identity.prove(&server_key, &nonce) })).await.map_err(transport)?;
            }
            ServerMessage::Identified { .. } => socket.send(send(ClientMessage::MyStanding)).await.map_err(transport)?,
            ServerMessage::IdentifyRefused { reason } => return Err(ConnectionError::Rejected(reason)),
            ServerMessage::Standing { standing, .. } => break standing,
            _ => {}
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
    use netrunner_protocol::GameEndReason;
    use netrunner_server::serve::{ServeBotKind, ServeOptions, Server};

    async fn start_server() -> std::net::SocketAddr {
        start_server_with(ServeBotKind::Planner).await
    }

    async fn start_server_with(bot_runner: ServeBotKind) -> std::net::SocketAddr {
        let options = ServeOptions { bot_runner, seed: Some(1), ..ServeOptions::default() };
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

    async fn ended(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) -> (Side, GameEndReason) {
        loop {
            match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("the game ends in time") {
                Some(ServerMessage::GameEnded { winner, reason }) => return (winner, reason),
                Some(_) => continue,
                None => panic!("the channel closed before GameEnded"),
            }
        }
    }

    async fn link_is(link: &mut watch::Receiver<Link>, wanted: impl Fn(&Link) -> bool) {
        tokio::time::timeout(Duration::from_secs(10), link.wait_for(|state| wanted(state))).await.expect("the link settles").unwrap();
    }

    /// The next event `wanted` picks out, within ten seconds.
    async fn next_where<T>(attached: &mut Attached, wanted: impl Fn(AttachedEvent) -> Option<T>) -> T {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let event = attached.next().await.expect("the connection is alive");
                if let Some(found) = wanted(event) {
                    return found;
                }
            }
        })
        .await
        .expect("the server answers within 10s")
    }

    fn deck(id: &str) -> DeckFile {
        netrunner_core::decks::by_id(id).expect("a built-in deck")
    }

    fn corp(url: &str) -> Connecting {
        seek(url.to_string(), seat_in_format("tester", NsgFormat::Startup, deck("brick_stack")))
    }

    #[tokio::test]
    async fn a_seat_plays_through_the_driver() {
        let url = format!("ws://{}", start_server().await);
        let mut joined = connect(corp(&url), |_| {}).await.unwrap();
        assert_eq!(joined.viewer, Viewer::Player(Side::Corp));
        assert!(joined.session_token.is_some());
        next_view(&mut joined.rx).await;
        joined.tx.send(ClientMessage::SubmitAction(PlayerAction::KeepHand)).unwrap();
        next_view(&mut joined.rx).await;
        assert_eq!(*joined.link.borrow(), Link::Up);
    }

    /// Two players on a human daemon, each attached once: the lobbies
    /// listed with no deck, a lobby joined, a game found and played to its
    /// end, both back in the lobby, and the next game found on the same
    /// two connections in the other chairs — while a game's channel pair
    /// closes with its game and carries nothing of the next.
    #[tokio::test]
    async fn the_next_game_is_found_on_the_same_connection() {
        let url = format!("ws://{}", start_server_with(ServeBotKind::None).await);
        let who = |name: &str| Goal::Attach(Who { player_name: name.into(), credentials: None });
        let mut one = spawn(url.clone(), who("one"));
        let mut two = spawn(url.clone(), who("two"));
        for attached in [&mut one, &mut two] {
            let lobbies = next_where(attached, |event| match event {
                AttachedEvent::Attached(lobbies) => Some(lobbies),
                _ => None,
            })
            .await;
            assert!(lobbies.iter().any(|lobby| lobby.id == "startup"), "{lobbies:?}");
            attached.join_lobby("startup".into(), None);
            let lobby = next_where(attached, |event| match event {
                AttachedEvent::LobbyJoined(lobby) => Some(lobby),
                _ => None,
            })
            .await;
            assert_eq!(lobby.format, NsgFormat::Startup);
        }
        one.seek(Chair::Corp(Box::new(deck("brick_stack"))));
        assert!(matches!(next_where(&mut one, Some).await, AttachedEvent::Queued(1)));
        two.seek(Chair::Runner(Box::new(deck("stolen_goods"))));
        let joined = |event| match event {
            AttachedEvent::Joined(joined) => Some(joined),
            _ => None,
        };
        let mut first = next_where(&mut one, joined).await;
        let mut second = next_where(&mut two, joined).await;
        assert_eq!((first.viewer, second.viewer), (Viewer::Player(Side::Corp), Viewer::Player(Side::Runner)));
        assert_eq!(first.decks, ("brick_stack".to_string(), String::new()), "told its own deck, never the other's");
        next_view(&mut first.rx).await;
        next_view(&mut second.rx).await;
        first.tx.send(ClientMessage::Surrender).unwrap();
        assert_eq!(ended(&mut first.rx).await, (Side::Runner, GameEndReason::Surrender));
        assert_eq!(ended(&mut second.rx).await, (Side::Runner, GameEndReason::Surrender));
        let back = |event| match event {
            AttachedEvent::BackInLobby(lobby) => Some(lobby),
            _ => None,
        };
        assert_eq!(next_where(&mut one, back).await.map(|lobby| lobby.id).as_deref(), Some("startup"));
        assert_eq!(next_where(&mut two, back).await.map(|lobby| lobby.id).as_deref(), Some("startup"));
        assert!(tokio::time::timeout(Duration::from_secs(5), async { while first.rx.recv().await.is_some() {} }).await.is_ok(), "a game's channel closes with its game");

        one.seek(Chair::Runner(Box::new(deck("dashing_mad"))));
        two.seek(Chair::Corp(Box::new(deck("glyph_of_warding"))));
        let mut third = next_where(&mut one, joined).await;
        let fourth = next_where(&mut two, joined).await;
        assert_eq!((third.viewer, fourth.viewer), (Viewer::Player(Side::Runner), Viewer::Player(Side::Corp)));
        assert_eq!(fourth.decks.0, "glyph_of_warding");
        next_view(&mut third.rx).await;
        assert!(second.rx.try_recv().is_err(), "the first game's channel carries nothing of the second");
    }

    /// The board closed on a live game — its `tx` dropped with the
    /// connection still held — concedes it: the opponent is told at once.
    #[tokio::test]
    async fn dropping_a_games_channel_while_attached_concedes_it() {
        let url = format!("ws://{}", start_server_with(ServeBotKind::None).await);
        let who = |name: &str| Goal::Attach(Who { player_name: name.into(), credentials: None });
        let mut one = spawn(url.clone(), who("one"));
        let mut two = spawn(url.clone(), who("two"));
        for (attached, chair) in [(&mut one, Chair::Corp(Box::new(deck("brick_stack")))), (&mut two, Chair::Runner(Box::new(deck("stolen_goods"))))] {
            next_where(attached, |event| matches!(event, AttachedEvent::Attached(_)).then_some(())).await;
            attached.join_lobby("startup".into(), None);
            next_where(attached, |event| matches!(event, AttachedEvent::LobbyJoined(_)).then_some(())).await;
            attached.seek(chair);
        }
        let joined = |event| match event {
            AttachedEvent::Joined(joined) => Some(joined),
            _ => None,
        };
        let first = next_where(&mut one, joined).await;
        let mut second = next_where(&mut two, joined).await;
        next_view(&mut second.rx).await;
        drop(first);
        assert_eq!(ended(&mut second.rx).await, (Side::Runner, GameEndReason::Surrender));
        next_where(&mut one, |event| matches!(event, AttachedEvent::BackInLobby(_)).then_some(())).await;
        assert_eq!(one.link(), Link::Up, "the connection stays");
    }

    /// Against a server that keeps its key: the player proves their own,
    /// the server's is remembered for the address, and when another key
    /// answers at that address later the connection is refused before
    /// anything is proved to it.
    #[tokio::test]
    async fn a_server_key_is_remembered_by_address_and_a_new_one_refused() {
        let dir = std::env::temp_dir().join(format!("netrunner_remote_pin_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let options = ServeOptions { bot_runner: ServeBotKind::Planner, seed: Some(1), data_dir: Some(dir.join("server")), ..ServeOptions::default() };
        let server = Server::bind("127.0.0.1:0", options).await.unwrap();
        let (addr, server_key) = (server.local_addr().unwrap(), server.public_key());
        tokio::spawn(server.run());
        let url = format!("ws://{addr}");
        let credentials = Credentials::in_dir(&dir.join("client")).unwrap();
        let signed_in = || seek(url.clone(), seat_in_format("tester", NsgFormat::Startup, deck("brick_stack")).with_credentials(Some(credentials.clone())));

        let mut joined = connect(signed_in(), |_| {}).await.unwrap();
        next_view(&mut joined.rx).await;
        let known_path = dir.join("client").join(crate::identity::KNOWN_SERVERS_FILE);
        assert_eq!(KnownServers::load(&known_path).unwrap().get(&url), Some(server_key), "remembered on first contact");
        drop(joined);

        let mut known = KnownServers::default();
        let impostor = netrunner_identity::Identity::from_secret([7; 32]).public_key();
        known.insert(&url, impostor);
        known.save(&known_path).unwrap();
        let refused = connect(signed_in(), |_| {}).await.err().expect("a server with another key is refused");
        assert!(matches!(refused, ConnectionError::ServerKeyChanged { remembered, found } if remembered == impostor && found == server_key), "{refused}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A standing is asked for with the key proved, and held to the
    /// server's remembered key like a game is: a new key with no rated game
    /// has none, and a server proving another key is refused.
    #[tokio::test]
    async fn a_standing_is_fetched_with_the_key_and_held_to_the_remembered_server() {
        let dir = std::env::temp_dir().join(format!("netrunner_remote_standing_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let options = ServeOptions { bot_runner: ServeBotKind::None, seed: Some(1), data_dir: Some(dir.join("server")), ..ServeOptions::default() };
        let server = Server::bind("127.0.0.1:0", options).await.unwrap();
        let (addr, server_key) = (server.local_addr().unwrap(), server.public_key());
        tokio::spawn(server.run());
        let url = format!("ws://{addr}");
        let credentials = Credentials::in_dir(&dir.join("client")).unwrap();
        assert_eq!(standing(&url, &credentials).await.unwrap(), None, "no rated game yet");
        let known_path = credentials.known_servers().unwrap();
        assert_eq!(KnownServers::load(&known_path).unwrap().get(&url), Some(server_key), "remembered on first contact");

        let mut known = KnownServers::default();
        known.insert(&url, netrunner_identity::Identity::from_secret([7; 32]).public_key());
        known.save(&known_path).unwrap();
        assert!(matches!(standing(&url, &credentials).await, Err(ConnectionError::ServerKeyChanged { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The socket is cut mid-match: the link says so, the seat is taken
    /// back with its token under the same channel pair, and the next view
    /// arrives on the `rx` the screen already holds.
    #[tokio::test]
    async fn a_cut_socket_is_resumed_under_the_same_channels() {
        let (proxy, url) = Proxy::start(start_server().await).await;
        let mut joined = connect(corp(&url), |_| {}).await.unwrap();
        next_view(&mut joined.rx).await;

        proxy.cut();
        link_is(&mut joined.link, |state| matches!(state, Link::Reconnecting { .. })).await;
        link_is(&mut joined.link, |state| *state == Link::Up).await;
        next_view(&mut joined.rx).await;
        joined.tx.send(ClientMessage::SubmitAction(PlayerAction::KeepHand)).unwrap();
        next_view(&mut joined.rx).await;
    }

    /// The socket is cut while attached in a lobby, with no game: the
    /// connection attaches again and is back in its lobby by itself, and
    /// a game is then found on it.
    #[tokio::test]
    async fn a_cut_socket_while_in_a_lobby_rejoins_it() {
        let (proxy, url) = Proxy::start(start_server().await).await;
        let mut attached = spawn(url, Goal::Attach(Who { player_name: "tester".into(), credentials: None }));
        next_where(&mut attached, |event| matches!(event, AttachedEvent::Attached(_)).then_some(())).await;
        attached.join_lobby("startup".into(), None);
        next_where(&mut attached, |event| matches!(event, AttachedEvent::LobbyJoined(_)).then_some(())).await;

        proxy.cut();
        next_where(&mut attached, |event| matches!(event, AttachedEvent::Link(Link::Reconnecting { .. })).then_some(())).await;
        next_where(&mut attached, |event| matches!(event, AttachedEvent::Link(Link::Up)).then_some(())).await;
        next_where(&mut attached, |event| matches!(event, AttachedEvent::Attached(_)).then_some(())).await;
        let lobby = next_where(&mut attached, |event| match event {
            AttachedEvent::LobbyJoined(lobby) => Some(lobby),
            _ => None,
        })
        .await;
        assert_eq!(lobby.id, "startup", "rejoined without being asked");
        // The standing is asked over the same connection; this one proved
        // no key, and the daemon keeps no book.
        attached.standing();
        let standing = next_where(&mut attached, |event| match event {
            AttachedEvent::Standing { key, standing } => Some((key, standing)),
            _ => None,
        })
        .await;
        assert_eq!(standing, (None, None));
        attached.seek(Chair::Corp(Box::new(deck("brick_stack"))));
        let mut joined = next_where(&mut attached, |event| match event {
            AttachedEvent::Joined(joined) => Some(joined),
            _ => None,
        })
        .await;
        next_view(&mut joined.rx).await;
    }

    /// A host by ticket, with no relay: the ticket names the host's own
    /// addresses, which is all a test on one machine needs and keeps it
    /// off the network.
    async fn start_peer_host() -> (crate::peer::PeerHost, String) {
        let (host, ticket, relayed) = start_peer_host_with(crate::peer::Relay::Off).await;
        assert!(!relayed, "no relay was asked for");
        (host, ticket)
    }

    async fn start_peer_host_with(relay: crate::peer::Relay) -> (crate::peer::PeerHost, String, bool) {
        let options = ServeOptions { bot_runner: ServeBotKind::Planner, seed: Some(1), ..ServeOptions::default() };
        let acceptor = Server::bind("127.0.0.1:0", options).await.unwrap().acceptor();
        let mut host = crate::peer::PeerHost::start(relay, move |stream, who| {
            let acceptor = acceptor.clone();
            tokio::spawn(async move { acceptor.serve(stream, &who).await });
        });
        let crate::peer::Offer::Ready { ticket, relayed } = host.ready().await else { panic!("the host never offered a ticket") };
        (host, ticket.to_string(), relayed)
    }

    /// The real thing, by hand: a ticket through n0's public relays.
    /// Ignored because it needs the internet, which CI's tests must not.
    /// `cargo test -p netrunner_client -- --ignored public_relay`.
    #[tokio::test]
    #[ignore = "needs the internet and n0's public relays"]
    async fn a_seat_plays_by_ticket_through_the_public_relay() {
        let (_host, ticket, relayed) = start_peer_host_with(crate::peer::Relay::Public).await;
        assert!(relayed, "a public relay answered");
        let ticket = crate::peer::Ticket::parse(&ticket).unwrap().relay_only();
        assert!(ticket.relay().is_some(), "the ticket names the relay: {ticket}");
        let ticket = ticket.to_string();
        let mut joined = connect(corp(&ticket), |_| {}).await.unwrap();
        next_view(&mut joined.rx).await;
        joined.tx.send(ClientMessage::SubmitAction(PlayerAction::KeepHand)).unwrap();
        next_view(&mut joined.rx).await;
    }

    /// The same seat as `a_seat_plays_through_the_driver`, reached by
    /// ticket over QUIC instead of by address: the server's handshake and
    /// the driver above the dial are the ones TCP uses.
    #[tokio::test]
    async fn a_seat_plays_by_ticket() {
        let (_host, ticket) = start_peer_host().await;
        let (matches, waiting, _) = list_matches(&ticket).await.unwrap();
        assert_eq!((matches.len(), waiting), (0, 0), "a ticket answers ListMatches too");
        let mut joined = connect(corp(&ticket), |_| {}).await.unwrap();
        assert_eq!(joined.viewer, Viewer::Player(Side::Corp));
        next_view(&mut joined.rx).await;
        joined.tx.send(ClientMessage::SubmitAction(PlayerAction::KeepHand)).unwrap();
        next_view(&mut joined.rx).await;
        assert_eq!(list_matches(&ticket).await.unwrap().0.len(), 1, "the match is the server's, whoever carried it");
    }

    /// A host that has stopped hosting: its ticket names a key nobody
    /// answers for, and the first dial fails rather than hangs.
    #[tokio::test]
    async fn a_ticket_whose_host_has_gone_fails_the_first_connection() {
        let (host, ticket) = start_peer_host().await;
        drop(host);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let result = tokio::time::timeout(Duration::from_secs(30), connect(corp(&ticket), |_| {})).await.expect("the machine bounds the first dial");
        assert!(matches!(result, Err(ConnectionError::Transport(_))), "{:?}", result.err());
    }

    #[tokio::test]
    async fn an_address_with_nothing_there_fails_the_first_connection() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        drop(listener);
        assert!(matches!(connect(corp(&url), |_| {}).await, Err(ConnectionError::Transport(_))));
    }

    #[tokio::test]
    async fn a_match_that_is_not_running_refuses_a_spectator() {
        let url = format!("ws://{}", start_server().await);
        let result = connect(watch(url, Uuid::new_v4()), |_| {}).await;
        assert!(matches!(result, Err(ConnectionError::Rejected(_))), "{:?}", result.err());
    }

    /// A deck the lobby's format refuses is a failed one-shot connection
    /// with the server's reason, as a refused `Connect` used to be.
    #[tokio::test]
    async fn a_refused_seek_fails_the_one_shot_connection() {
        let url = format!("ws://{}", start_server().await);
        let wrong = seat("tester", "startup".into(), None, deck("stolen_goods"));
        let wrong = Seat { chair: Chair::Corp(Box::new(deck("stolen_goods"))), ..wrong };
        let result = connect(seek(url, wrong), |_| {}).await;
        assert!(matches!(&result, Err(ConnectionError::Rejected(reason)) if reason.contains("Runner deck")), "{:?}", result.err());
    }
}
