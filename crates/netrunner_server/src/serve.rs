//! `--serve` mode: a WebSocket daemon that seats remote clients into
//! `MatchSession`s. Library code rather than part of `main.rs` so a test can
//! bind an ephemeral port and drive real sockets through the whole
//! handshake — connect, queue, pair, drop, resume — which is the only
//! honest test of a connection protocol.
//!
//! **One `Registry`, one lock.** Every piece of daemon bookkeeping — the
//! running matches, the seat tokens, the lobby queue and the counter the
//! seed policy reads — sits behind a single `std` mutex. Separate locks
//! (the lobby used to have its own `tokio::sync::Mutex`) were rejected
//! because the operations that matter are compound: a pairing must sweep
//! the lobby, check the cap, claim a match index and register two tickets
//! as one step, or two simultaneous pairings both pass the cap and a
//! resume can find a ticket that is half-registered. A `std` mutex rather
//! than `tokio`'s because nothing awaits while holding it; the longest
//! critical section is a `GameState::setup` (two 45-card shuffles), which
//! is nothing.
//!
//! **Seat tokens.** Every channel seat gets a `Uuid` and a `SeatTicket` in
//! the registry for as long as its match runs. `ClientMessage::Resume {
//! session_token }` looks the ticket up, builds a fresh channel pair and
//! bridge for the new socket, sends `MatchJoined` on it and hands the pair
//! to the session through its `ReattachHandle`. Tickets are keyed by
//! token, not by match, because a token is a *seat's* credential: the
//! Corp's token can never reseat the Runner. They are removed when the
//! session returns, so a resume after the game ended is refused with
//! `ResumeRejected` rather than answered with a `MatchJoined` for a match
//! that no longer exists.
//!
//! **The lobby is a queue, not a slot, and a waiter's token is the same
//! token.** Under `ServeBotKind::None` a `Connect` either pairs with the
//! first waiter in the same room or joins the queue and is told so
//! (`ServerMessage::Queued`). The token issued there is the one
//! `MatchJoined` will carry later, so `Resume` while queued swaps the
//! socket under the queue entry with nothing new for the client to hold.
//! Waiters whose socket has since closed are swept at every pairing and
//! every `ListMatches` — the bridge drops its channel halves together, so
//! a dead waiter's `tx` reports closed — which is what stops the next
//! human pairing with a ghost. A swept waiter simply connects again: a
//! queue position is not a seat with a game in it, so it gets no grace.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard};
use std::time::{Duration, Instant};

use clap::ValueEnum;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

use netrunner_bots::{BotAgent, HeuristicAgent, MctsAgent, Personality};
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{GameState, Side};
use netrunner_rating::{Outcome, RatingBook, Track};

use crate::match_session::{MatchSession, PlayerSlot, ReattachHandle, TurnTimeout, DEFAULT_RECONNECT_GRACE};
use crate::protocol::{ClientMessage, MatchSummary, ServerMessage};
use crate::{fixtures, net};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServeBotKind {
    Heuristic,
    Mcts,
    /// Queue connecting clients and pair them into human-vs-human matches,
    /// first come first served within a room.
    None,
}

impl ServeBotKind {
    /// What `MatchList` calls a bot seat.
    fn seat_name(self) -> &'static str {
        match self {
            ServeBotKind::Heuristic => "heuristic bot",
            ServeBotKind::Mcts => "mcts bot",
            ServeBotKind::None => unreachable!("a human-vs-human daemon seats no bot"),
        }
    }

    /// The bot's participant id in the rating book. Prefixed so no human
    /// name can collide with it on the human-vs-bot track — a player is
    /// free to call themselves "heuristic".
    fn rating_id(self) -> &'static str {
        match self {
            ServeBotKind::Heuristic => "bot:heuristic",
            ServeBotKind::Mcts => "bot:mcts",
            ServeBotKind::None => unreachable!("a human-vs-human daemon seats no bot"),
        }
    }

    /// Which ladder a match on this daemon counts toward.
    fn track(self) -> Track {
        match self {
            ServeBotKind::None => Track::HumanVsHuman,
            ServeBotKind::Heuristic | ServeBotKind::Mcts => Track::HumanVsBot,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// Bot opponent seated against every connecting client; `None` pairs
    /// humans instead.
    pub bot_runner: ServeBotKind,
    /// The bot's `Personality`; part of its rating id when not balanced,
    /// so a rush Corp and a glacier Corp are different opponents on the
    /// human-vs-bot ladder.
    pub bot_personality: Personality,
    /// Base seed every match's seed is derived from (`base + match
    /// index`, the headless driver's policy). `None` picks one at random.
    pub seed: Option<u64>,
    /// See `MatchSession::with_reconnect_grace`.
    pub reconnect_grace: Duration,
    /// How many matches may run at once; `None` is no limit. At the cap a
    /// `Connect` is answered with `ConnectRejected` and the socket closed,
    /// in both modes — a human-vs-human `Connect` is refused *before* it
    /// is queued, since a queue entry that cannot be paired is a promise
    /// the daemon cannot keep. Queueing past the cap and pairing as
    /// matches end was rejected: it needs a wake-up from the session-exit
    /// task, and nobody has asked to wait.
    pub max_matches: Option<usize>,
    /// See `MatchSession::with_turn_timeout`; `None` runs without a clock.
    pub turn_timeout: TurnTimeout,
    /// Where the daemon keeps its `netrunner_rating::RatingBook`. Loaded
    /// at bind, rewritten after every rated match (temp file plus
    /// rename, like the deck store and the card cache), and the only
    /// thing that makes a rating *persistent*. `None` rates nothing: a
    /// daemon with no file is stateless, which is what every test wants.
    pub ratings_file: Option<PathBuf>,
}

impl Default for ServeOptions {
    fn default() -> Self {
        ServeOptions {
            bot_runner: ServeBotKind::Heuristic,
            bot_personality: Personality::Balanced,
            seed: None,
            reconnect_grace: DEFAULT_RECONNECT_GRACE,
            max_matches: None,
            turn_timeout: None,
            ratings_file: None,
        }
    }
}

fn make_serve_agent(kind: ServeBotKind, side: Side, seed: u64, personality: Personality) -> Box<dyn BotAgent> {
    match kind {
        ServeBotKind::Heuristic => Box::new(HeuristicAgent::with_personality(side, seed, personality)),
        ServeBotKind::Mcts => Box::new(MctsAgent::new(side, seed).with_personality(personality)),
        ServeBotKind::None => unreachable!("caller only invokes this for a bot-backed ServeBotKind"),
    }
}

/// One seat's reattach credentials, held for as long as its match runs.
#[derive(Clone)]
struct SeatTicket {
    match_id: Uuid,
    side: Side,
    handle: ReattachHandle,
}

/// A running match as `MatchList` reports it. Holds the session's
/// `ReattachHandle` so a `Spectate { match_id }` can reach the pump; the
/// seed is deliberately *not* here — with the fixed decklist it
/// reproduces R&D's order, so it must never leave the host.
struct MatchEntry {
    corp: String,
    runner: String,
    started_at: Instant,
    handle: ReattachHandle,
}

/// A connected-but-unmatched human, waiting for another human in the
/// same room (`ServeBotKind::None` only).
struct PendingHuman {
    token: Uuid,
    player_name: String,
    preferred_side: Option<Side>,
    room: Option<String>,
    tx: mpsc::UnboundedSender<ServerMessage>,
    slot: PlayerSlot,
}

impl PendingHuman {
    fn seated(self) -> SeatedPlayer {
        SeatedPlayer { rating_id: self.player_name.clone(), name: self.player_name, token: self.token, slot: self.slot }
    }
}

/// A player about to be seated: the name `MatchList` will show, the id
/// the rating book knows them by (the name itself for a human, a
/// `bot:` id for a bot), the token `MatchJoined` will carry (already
/// issued if they came through the lobby, so one token spans queue and
/// match), and the slot the session plays them through. A bot seat
/// carries a token too, unused — cheaper than a second type for the one
/// case that never resumes.
struct SeatedPlayer {
    name: String,
    rating_id: String,
    token: Uuid,
    slot: PlayerSlot,
}

impl SeatedPlayer {
    fn channel_tx(&self) -> Option<&mpsc::UnboundedSender<ServerMessage>> {
        match &self.slot {
            PlayerSlot::Channel { tx, .. } => Some(tx),
            PlayerSlot::Bot(_) => None,
        }
    }
}

/// The daemon's whole bookkeeping — see the module doc for why it is one
/// struct under one lock.
#[derive(Default)]
struct Registry {
    matches: HashMap<Uuid, MatchEntry>,
    seats: HashMap<Uuid, SeatTicket>,
    /// Every rating the daemon holds; see `ServeOptions::ratings_file`.
    ratings: RatingBook,
    /// Arrival order; pairing takes the first waiter in the newcomer's room.
    lobby: Vec<PendingHuman>,
    /// Claimed by `allocate`, never reused: match `n` plays on
    /// `base_seed + n` whether or not match `n - 1` finished, so a
    /// `--seed` run is reproducible connection for connection.
    next_match_index: u64,
}

impl Registry {
    /// Drops every waiter whose socket has gone. `bridge_websocket` tears
    /// both its halves down together, so a closed socket shows up here as
    /// a closed `tx` without any watcher task.
    fn sweep_lobby(&mut self) {
        self.lobby.retain(|waiter| !waiter.tx.is_closed());
    }

    fn at_cap(&self, options: &ServeOptions) -> bool {
        options.max_matches.is_some_and(|cap| self.matches.len() >= cap)
    }

    /// Claims the next match id and seed. The caller has checked `at_cap`
    /// under the same lock, which is what makes the cap exact.
    fn allocate(&mut self, base_seed: u64) -> (Uuid, u64) {
        let seed = base_seed.wrapping_add(self.next_match_index);
        self.next_match_index += 1;
        (Uuid::new_v4(), seed)
    }

    fn match_list(&self, options: &ServeOptions) -> ServerMessage {
        let now = Instant::now();
        let mut entries: Vec<(&Uuid, &MatchEntry)> = self.matches.iter().collect();
        entries.sort_by_key(|(_, entry)| entry.started_at);
        ServerMessage::MatchList {
            matches: entries
                .into_iter()
                .map(|(match_id, entry)| MatchSummary {
                    match_id: *match_id,
                    corp: entry.corp.clone(),
                    runner: entry.runner.clone(),
                    started_secs_ago: now.saturating_duration_since(entry.started_at).as_secs(),
                })
                .collect(),
            waiting_in_lobby: self.lobby.len(),
            max_matches: options.max_matches,
        }
    }
}

/// Everything one accepted connection needs, cloned per connection.
#[derive(Clone)]
struct Shared {
    cards: CardRegistry,
    registry: Arc<StdMutex<Registry>>,
    options: ServeOptions,
    base_seed: u64,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Registry> {
        self.registry.lock().expect("daemon registry poisoned")
    }

    /// Sweeps first so `waiting_in_lobby` counts players who can actually
    /// be paired — a test (and a client) polls this to learn a dropped
    /// waiter is gone, so it must not report ghosts.
    fn match_list(&self) -> ServerMessage {
        let mut registry = self.lock();
        registry.sweep_lobby();
        registry.match_list(&self.options)
    }

    /// Rates one finished match and rewrites the book. Under the registry
    /// lock, so two matches ending at once serialize their updates; the
    /// write is a few kilobytes and the lock is never held across an
    /// await, so that is fine. A failed write is logged, not fatal: the
    /// ratings are already applied in memory and the next match's write
    /// carries them.
    fn rate(&self, track: Track, corp: &str, runner: &str, outcome: Outcome) {
        let Some(path) = &self.options.ratings_file else { return };
        let mut registry = self.lock();
        let (corp_after, runner_after) = registry.ratings.record(track, corp, runner, outcome);
        tracing::info!(
            ?track, corp, runner, ?outcome,
            corp_rating = corp_after.corp.rating.rating, runner_rating = runner_after.runner.rating.rating,
            "match rated"
        );
        if let Err(error) = save_ratings(path, &registry.ratings) {
            tracing::warn!(path = %path.display(), ?error, "could not save the rating book");
        }
    }
}

fn load_ratings(path: &Path) -> std::io::Result<RatingBook> {
    if !path.exists() {
        return Ok(RatingBook::default());
    }
    let json = std::fs::read_to_string(path)?;
    RatingBook::from_json(&json).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

/// Temp file plus rename, so a crash mid-write leaves the previous book
/// intact rather than half a JSON document.
fn save_ratings(path: &Path, book: &RatingBook) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, book.to_json())?;
    std::fs::rename(tmp, path)
}

pub struct Server {
    listener: TcpListener,
    shared: Shared,
}

impl Server {
    /// Binds `addr` (`host:port`; port 0 for an ephemeral one — see
    /// `local_addr`). Accepting starts in `run`.
    pub async fn bind(addr: &str, options: ServeOptions) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        let base_seed = options.seed.unwrap_or_else(rand::random);
        let ratings = match &options.ratings_file {
            Some(path) => load_ratings(path)?,
            None => RatingBook::default(),
        };
        let shared = Shared {
            cards: fixtures::kate_vs_hb_registry(),
            registry: Arc::new(StdMutex::new(Registry { ratings, ..Registry::default() })),
            options,
            base_seed,
        };
        Ok(Server { listener, shared })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// The accept loop. Returns only if `accept` itself fails.
    pub async fn run(self) -> std::io::Result<()> {
        tracing::info!(addr = %self.local_addr()?, bot_runner = ?self.shared.options.bot_runner, "netrunner_server listening");
        loop {
            let (stream, peer_addr) = self.listener.accept().await?;
            let shared = self.shared.clone();
            tokio::spawn(async move {
                if let Err(error) = handle_connection(stream, shared).await {
                    tracing::warn!(%peer_addr, ?error, "connection ended with an error");
                }
            });
        }
    }
}

/// The first message that commits a socket to something. `ListMatches` is
/// answered inline without leaving this loop, so a client can look before
/// it joins; anything else is skipped until one of these arrives.
enum Handshake {
    Connect { player_name: String, preferred_side: Option<Side>, room: Option<String> },
    Resume { session_token: Uuid },
    Spectate { match_id: Uuid },
}

async fn handle_connection(stream: TcpStream, shared: Shared) -> Result<(), Box<dyn std::error::Error>> {
    let mut ws_stream = tokio_tungstenite::accept_async(stream).await?;

    let handshake = loop {
        match ws_stream.next().await {
            Some(Ok(WsMessage::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Connect { player_name, preferred_side, room }) => {
                    break Handshake::Connect { player_name, preferred_side, room };
                }
                Ok(ClientMessage::Resume { session_token }) => break Handshake::Resume { session_token },
                Ok(ClientMessage::Spectate { match_id }) => break Handshake::Spectate { match_id },
                Ok(ClientMessage::ListMatches) => {
                    ws_stream.send(WsMessage::Text(serde_json::to_string(&shared.match_list())?)).await?;
                }
                _ => continue,
            },
            Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => return Err("connection closed before handshake".into()),
            Some(Ok(_)) => continue,
        }
    };

    match handshake {
        Handshake::Connect { player_name, preferred_side, room } => {
            tracing::info!(%player_name, ?preferred_side, ?room, "client connected");
            let (session_tx, bridge_rx) = mpsc::unbounded_channel::<ServerMessage>();
            let (bridge_tx, session_rx) = mpsc::unbounded_channel::<ClientMessage>();
            tokio::spawn(net::bridge_websocket(ws_stream, bridge_tx, bridge_rx));
            let slot = PlayerSlot::Channel { tx: session_tx.clone(), rx: session_rx };

            match shared.options.bot_runner {
                ServeBotKind::None => enqueue_or_pair(&shared, player_name, preferred_side, room, session_tx, slot),
                kind => seat_vs_bot(&shared, kind, player_name, preferred_side, session_tx, slot),
            }
        }
        Handshake::Resume { session_token } => {
            let ticket = shared.lock().seats.get(&session_token).cloned();
            if let Some(ticket) = ticket.filter(|ticket| ticket.handle.is_live()) {
                tracing::info!(%session_token, match_id = %ticket.match_id, side = ?ticket.side, "client resumed");
                let (session_tx, bridge_rx) = mpsc::unbounded_channel::<ServerMessage>();
                let (bridge_tx, session_rx) = mpsc::unbounded_channel::<ClientMessage>();
                tokio::spawn(net::bridge_websocket(ws_stream, bridge_tx, bridge_rx));
                // `MatchJoined` before the reattach, so it precedes the
                // `StateUpdate` the session answers with: the client is
                // waiting for its seat back before it renders anything.
                let _ = session_tx.send(ServerMessage::MatchJoined {
                    match_id: ticket.match_id,
                    assigned_side: ticket.side,
                    session_token,
                });
                if ticket.handle.reattach(ticket.side, session_tx.clone(), session_rx).is_err() {
                    // Lost the race with the match ending between the
                    // liveness check and here. The bridge is already up,
                    // so the refusal goes down the same channel.
                    let _ = session_tx.send(ServerMessage::ResumeRejected { reason: "the match ended".into() });
                }
                return Ok(());
            }

            // Not a seat: perhaps a queue position. The new halves replace
            // the waiter's under the lock; the old ones drop with it,
            // which closes the old socket — newest connection wins, as it
            // does for a seat.
            let (session_tx, bridge_rx) = mpsc::unbounded_channel::<ServerMessage>();
            let (bridge_tx, session_rx) = mpsc::unbounded_channel::<ClientMessage>();
            let position = {
                let mut registry = shared.lock();
                match registry.lobby.iter().position(|waiter| waiter.token == session_token) {
                    Some(index) => {
                        let waiter = &mut registry.lobby[index];
                        waiter.tx = session_tx.clone();
                        waiter.slot = PlayerSlot::Channel { tx: session_tx.clone(), rx: session_rx };
                        Some(index + 1)
                    }
                    None => None,
                }
            };
            match position {
                Some(position) => {
                    tracing::info!(%session_token, position, "client resumed its place in the lobby");
                    tokio::spawn(net::bridge_websocket(ws_stream, bridge_tx, bridge_rx));
                    let _ = session_tx.send(ServerMessage::Queued { session_token, position });
                }
                None => {
                    tracing::info!(%session_token, "resume refused: no such seat");
                    let refusal = ServerMessage::ResumeRejected { reason: "no live match holds that session token".into() };
                    let _ = ws_stream.send(WsMessage::Text(serde_json::to_string(&refusal)?)).await;
                    let _ = ws_stream.close(None).await;
                }
            }
        }
        Handshake::Spectate { match_id } => {
            let handle = shared.lock().matches.get(&match_id).map(|entry| entry.handle.clone());
            let Some(handle) = handle.filter(ReattachHandle::is_live) else {
                tracing::info!(%match_id, "spectate refused: no such match");
                let refusal = ServerMessage::ConnectRejected { reason: "no live match has that id".into() };
                let _ = ws_stream.send(WsMessage::Text(serde_json::to_string(&refusal)?)).await;
                let _ = ws_stream.close(None).await;
                return Ok(());
            };
            tracing::info!(%match_id, "spectator joined");
            let (session_tx, bridge_rx) = mpsc::unbounded_channel::<ServerMessage>();
            let (bridge_tx, mut session_rx) = mpsc::unbounded_channel::<ClientMessage>();
            tokio::spawn(net::bridge_websocket(ws_stream, bridge_tx, bridge_rx));
            // The session never reads a spectator's messages, but the
            // receiving half must stay open: the bridge's recv task ends
            // when its send into a dropped receiver fails, and the select
            // in `bridge_websocket` then closes the socket — so a dropped
            // `rx` would kick the spectator on its first keypress. Drain
            // and discard instead.
            tokio::spawn(async move { while session_rx.recv().await.is_some() {} });
            // `Spectating` before the control message, so it precedes the
            // `StateUpdate` the session answers with.
            let _ = session_tx.send(ServerMessage::Spectating { match_id });
            if handle.add_spectator(session_tx.clone()).is_err() {
                let _ = session_tx.send(ServerMessage::ConnectRejected { reason: "the match ended".into() });
            }
        }
    }
    Ok(())
}

const AT_CAP: &str = "the host is at its match limit";

/// A `Connect` the daemon will not honour. Dropping the caller's channel
/// halves afterwards is what closes the socket: the bridge's send task
/// ends when every sender is gone and closes the stream with a `Close`
/// frame, so a refusal is one message and a clean disconnect, not a
/// socket left waiting for a `MatchJoined` that will never come.
fn refuse(tx: &mpsc::UnboundedSender<ServerMessage>, reason: &str) {
    let _ = tx.send(ServerMessage::ConnectRejected { reason: reason.to_string() });
}

fn seat_vs_bot(
    shared: &Shared,
    kind: ServeBotKind,
    player_name: String,
    preferred_side: Option<Side>,
    tx: mpsc::UnboundedSender<ServerMessage>,
    slot: PlayerSlot,
) {
    let mut registry = shared.lock();
    if registry.at_cap(&shared.options) {
        refuse(&tx, AT_CAP);
        return;
    }
    let (match_id, seed) = registry.allocate(shared.base_seed);

    let human_side = preferred_side.unwrap_or(Side::Corp);
    let human = SeatedPlayer { rating_id: player_name.clone(), name: player_name, token: Uuid::new_v4(), slot };
    let bot = SeatedPlayer {
        name: kind.seat_name().to_string(),
        rating_id: match shared.options.bot_personality {
            Personality::Balanced => kind.rating_id().to_string(),
            personality => format!("{}:{personality}", kind.rating_id()),
        },
        token: Uuid::new_v4(),
        slot: PlayerSlot::Bot(make_serve_agent(kind, human_side.other(), seed.wrapping_add(1), shared.options.bot_personality)),
    };
    let (corp, runner) = match human_side {
        Side::Corp => (human, bot),
        Side::Runner => (bot, human),
    };
    start_match(shared, &mut registry, match_id, seed, corp, runner);
}

/// `ServeBotKind::None`: pair with the first waiter in the same room, or
/// join the queue. `room: None` is the public queue; a named room pairs
/// only with itself, which is the whole of "play against my friend" — an
/// explicit create/join protocol was rejected because a match only ever
/// comes into being by pairing two waiters, so there is no open-match
/// object for a second message to join.
fn enqueue_or_pair(
    shared: &Shared,
    player_name: String,
    preferred_side: Option<Side>,
    room: Option<String>,
    tx: mpsc::UnboundedSender<ServerMessage>,
    slot: PlayerSlot,
) {
    let mut registry = shared.lock();
    registry.sweep_lobby();
    if registry.at_cap(&shared.options) {
        refuse(&tx, AT_CAP);
        return;
    }
    let newcomer = PendingHuman { token: Uuid::new_v4(), player_name, preferred_side, room, tx, slot };

    let Some(index) = registry.lobby.iter().position(|waiter| waiter.room == newcomer.room) else {
        let (token, tx) = (newcomer.token, newcomer.tx.clone());
        registry.lobby.push(newcomer);
        let position = registry.lobby.len();
        let _ = tx.send(ServerMessage::Queued { session_token: token, position });
        return;
    };
    let waiter = registry.lobby.remove(index);
    let (match_id, seed) = registry.allocate(shared.base_seed);
    let (corp, runner) = assign_sides(waiter, newcomer);
    start_match(shared, &mut registry, match_id, seed, corp.seated(), runner.seated());
}

/// Sets up the state, builds the session, records the match and a ticket
/// per channel seat, tells each seat it has joined, and runs the match;
/// the match and its tickets are dropped when it returns. One place for
/// all of it so the bot and lobby paths cannot disagree on the order —
/// `MatchJoined` must precede the session's first `StateUpdate`, and a
/// ticket must exist before a client can possibly present it. Runs under
/// the caller's registry lock so the cap it was admitted under still
/// holds when the entry lands.
fn start_match(shared: &Shared, registry: &mut Registry, match_id: Uuid, seed: u64, corp: SeatedPlayer, runner: SeatedPlayer) {
    let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
    let state = match GameState::setup(&corp_deck, &runner_deck, &shared.cards, seed) {
        Ok((state, _events)) => state,
        Err(error) => {
            let reason = format!("match setup failed: {error:?}");
            for player in [&corp, &runner] {
                if let Some(tx) = player.channel_tx() {
                    refuse(tx, &reason);
                }
            }
            return;
        }
    };

    let seats: Vec<(Side, Uuid, mpsc::UnboundedSender<ServerMessage>)> = [(&corp, Side::Corp), (&runner, Side::Runner)]
        .into_iter()
        .filter_map(|(player, side)| player.channel_tx().map(|tx| (side, player.token, tx.clone())))
        .collect();
    let (corp_name, runner_name) = (corp.name, runner.name);
    let (corp_rating_id, runner_rating_id) = (corp.rating_id, runner.rating_id);

    let session = MatchSession::new(state, shared.cards.clone(), corp.slot, runner.slot)
        .with_reconnect_grace(shared.options.reconnect_grace)
        .with_turn_timeout(shared.options.turn_timeout);
    let handle = session.reattach_handle();
    registry.matches.insert(match_id, MatchEntry { corp: corp_name, runner: runner_name, started_at: Instant::now(), handle: handle.clone() });

    let mut tokens = Vec::with_capacity(seats.len());
    for (side, session_token, tx) in seats {
        registry.seats.insert(session_token, SeatTicket { match_id, side, handle: handle.clone() });
        let _ = tx.send(ServerMessage::MatchJoined { match_id, assigned_side: side, session_token });
        tokens.push(session_token);
    }

    let shared = shared.clone();
    tokio::spawn(async move {
        let (_state, outcome) = session.run_with_outcome().await;
        {
            let mut registry = shared.lock();
            registry.matches.remove(&match_id);
            for token in tokens {
                registry.seats.remove(&token);
            }
        }
        // A forfeit — surrender, disconnect, clock — is a loss like any
        // other; a stall (`None`) is nobody's and goes unrated.
        let outcome = match outcome {
            Some((Side::Corp, _)) => Outcome::CorpWin,
            Some((Side::Runner, _)) => Outcome::RunnerWin,
            None => return,
        };
        shared.rate(shared.options.bot_runner.track(), &corp_rating_id, &runner_rating_id, outcome);
    });
}

/// First player's explicit side preference wins; otherwise the second
/// player's; otherwise the first connection defaults to Corp.
fn assign_sides(a: PendingHuman, b: PendingHuman) -> (PendingHuman, PendingHuman) {
    match (a.preferred_side, b.preferred_side) {
        (Some(Side::Corp), _) => (a, b),
        (Some(Side::Runner), _) => (b, a),
        (_, Some(Side::Corp)) => (b, a),
        (_, Some(Side::Runner)) => (a, b),
        _ => (a, b),
    }
}
