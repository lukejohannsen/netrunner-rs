//! `--serve` mode: a WebSocket daemon that seats remote clients into
//! `MatchSession`s. Library code rather than part of `main.rs` so a test can
//! bind an ephemeral port and drive real sockets through the whole
//! handshake — connect, drop, resume — which is the only honest test of a
//! reconnection protocol.
//!
//! **Seat tokens.** Every channel seat gets a `Uuid` at `MatchJoined` time
//! and a `SeatTicket` in `Server::seats` for as long as its match runs.
//! `ClientMessage::Resume { session_token }` looks the ticket up, builds a
//! fresh channel pair and bridge for the new socket, sends `MatchJoined`
//! on it and hands the pair to the session through its `ReattachHandle`.
//! The registry is keyed by token, not by match, because a token is a
//! *seat's* credential: the Corp's token can never reseat the Runner.
//! Tickets are removed when the session returns, so a resume after the
//! game ended is refused with `ResumeRejected` rather than answered with a
//! `MatchJoined` for a match that no longer exists.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use clap::ValueEnum;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

use netrunner_bots::{BotAgent, HeuristicAgent, MctsAgent};
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{GameState, Side};

use crate::match_session::{MatchSession, PlayerSlot, ReattachHandle, DEFAULT_RECONNECT_GRACE};
use crate::protocol::{ClientMessage, ServerMessage};
use crate::{fixtures, net};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServeBotKind {
    Heuristic,
    Mcts,
    /// Queue connecting clients and pair the first two together as a
    /// human-vs-human match.
    None,
}

#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// Bot opponent seated against every connecting client; `None` pairs
    /// humans instead.
    pub bot_runner: ServeBotKind,
    /// Base seed each accepted match's seed is derived from. `None` picks
    /// one at random.
    pub seed: Option<u64>,
    /// See `MatchSession::with_reconnect_grace`.
    pub reconnect_grace: Duration,
}

impl Default for ServeOptions {
    fn default() -> Self {
        ServeOptions { bot_runner: ServeBotKind::Heuristic, seed: None, reconnect_grace: DEFAULT_RECONNECT_GRACE }
    }
}

fn make_serve_agent(kind: ServeBotKind, side: Side, seed: u64) -> Box<dyn BotAgent> {
    match kind {
        ServeBotKind::Heuristic => Box::new(HeuristicAgent::new(side, seed)),
        ServeBotKind::Mcts => Box::new(MctsAgent::new(side, seed)),
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

/// A `std` mutex: the map is only ever touched between awaits, and a
/// `tokio::sync::Mutex` would buy nothing but a larger critical section.
type Seats = Arc<StdMutex<HashMap<Uuid, SeatTicket>>>;

/// A connected-but-unmatched human client, waiting in the lobby for a
/// second human to pair against (only populated under `ServeBotKind::None`).
struct PendingHuman {
    preferred_side: Option<Side>,
    tx: mpsc::UnboundedSender<ServerMessage>,
    slot: PlayerSlot,
}

type Lobby = Arc<Mutex<Option<PendingHuman>>>;

/// Everything one accepted connection needs, cloned per connection.
#[derive(Clone)]
struct Shared {
    registry: CardRegistry,
    lobby: Lobby,
    seats: Seats,
    options: ServeOptions,
}

pub struct Server {
    listener: TcpListener,
    shared: Shared,
    base_seed: u64,
}

impl Server {
    /// Binds `addr` (`host:port`; port 0 for an ephemeral one — see
    /// `local_addr`). Accepting starts in `run`.
    pub async fn bind(addr: &str, options: ServeOptions) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        let base_seed = options.seed.unwrap_or_else(rand::random);
        let shared = Shared {
            registry: fixtures::kate_vs_hb_registry(),
            lobby: Arc::new(Mutex::new(None)),
            seats: Arc::new(StdMutex::new(HashMap::new())),
            options,
        };
        Ok(Server { listener, shared, base_seed })
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
            let seed = self.base_seed.wrapping_add(rand::random::<u64>());
            tokio::spawn(async move {
                if let Err(error) = handle_connection(stream, shared, seed).await {
                    tracing::warn!(%peer_addr, ?error, "connection ended with an error");
                }
            });
        }
    }
}

/// The first message a client sends. Anything else is skipped until one
/// of these arrives, as before.
enum Handshake {
    Connect { player_name: String, preferred_side: Option<Side> },
    Resume { session_token: Uuid },
}

async fn handle_connection(stream: TcpStream, shared: Shared, seed: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut ws_stream = tokio_tungstenite::accept_async(stream).await?;

    let handshake = loop {
        match ws_stream.next().await {
            Some(Ok(WsMessage::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Connect { player_name, preferred_side }) => {
                    break Handshake::Connect { player_name, preferred_side };
                }
                Ok(ClientMessage::Resume { session_token }) => break Handshake::Resume { session_token },
                _ => continue,
            },
            Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => return Err("connection closed before handshake".into()),
            Some(Ok(_)) => continue,
        }
    };

    match handshake {
        Handshake::Connect { player_name, preferred_side } => {
            tracing::info!(%player_name, ?preferred_side, "client connected");
            let (session_tx, bridge_rx) = mpsc::unbounded_channel::<ServerMessage>();
            let (bridge_tx, session_rx) = mpsc::unbounded_channel::<ClientMessage>();
            tokio::spawn(net::bridge_websocket(ws_stream, bridge_tx, bridge_rx));
            let slot = PlayerSlot::Channel { tx: session_tx.clone(), rx: session_rx };

            if shared.options.bot_runner == ServeBotKind::None {
                pair_with_human(shared, preferred_side, session_tx, slot, seed).await;
            } else {
                spawn_vs_bot(shared, preferred_side, session_tx, slot, seed);
            }
        }
        Handshake::Resume { session_token } => {
            let ticket = shared.seats.lock().expect("seat registry poisoned").get(&session_token).cloned();
            let Some(ticket) = ticket.filter(|ticket| ticket.handle.is_live()) else {
                tracing::info!(%session_token, "resume refused: no such seat");
                let refusal = ServerMessage::ResumeRejected { reason: "no live match holds that session token".into() };
                let _ = ws_stream.send(WsMessage::Text(serde_json::to_string(&refusal)?)).await;
                let _ = ws_stream.close(None).await;
                return Ok(());
            };
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
                // liveness check and here. The bridge is already up, so
                // the refusal goes down the same channel.
                let _ = session_tx.send(ServerMessage::ResumeRejected { reason: "the match ended".into() });
            }
        }
    }
    Ok(())
}

/// Builds the session, registers a ticket per channel seat, tells each
/// seat it has joined, and runs the match; tickets are dropped when it
/// returns. One place for all of it so the bot and lobby paths cannot
/// disagree on the order — `MatchJoined` must precede the session's first
/// `StateUpdate`, and a ticket must exist before a client can possibly
/// present it.
fn start_match(shared: Shared, state: GameState, corp: PlayerSlot, runner: PlayerSlot, match_id: Uuid) {
    let seats: Vec<(Side, mpsc::UnboundedSender<ServerMessage>)> = [(&corp, Side::Corp), (&runner, Side::Runner)]
        .into_iter()
        .filter_map(|(slot, side)| match slot {
            PlayerSlot::Channel { tx, .. } => Some((side, tx.clone())),
            PlayerSlot::Bot(_) => None,
        })
        .collect();

    let session = MatchSession::new(state, shared.registry, corp, runner).with_reconnect_grace(shared.options.reconnect_grace);
    let handle = session.reattach_handle();

    let mut tokens = Vec::with_capacity(seats.len());
    for (side, tx) in seats {
        let session_token = Uuid::new_v4();
        shared
            .seats
            .lock()
            .expect("seat registry poisoned")
            .insert(session_token, SeatTicket { match_id, side, handle: handle.clone() });
        let _ = tx.send(ServerMessage::MatchJoined { match_id, assigned_side: side, session_token });
        tokens.push(session_token);
    }

    let registry = shared.seats;
    tokio::spawn(async move {
        session.run().await;
        let mut registry = registry.lock().expect("seat registry poisoned");
        for token in tokens {
            registry.remove(&token);
        }
    });
}

fn spawn_vs_bot(shared: Shared, preferred_side: Option<Side>, tx: mpsc::UnboundedSender<ServerMessage>, slot: PlayerSlot, seed: u64) {
    let human_side = preferred_side.unwrap_or(Side::Corp);
    let bot_side = human_side.other();

    let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
    let (state, _events) = match GameState::setup(&corp_deck, &runner_deck, &shared.registry, seed) {
        Ok(setup) => setup,
        Err(error) => {
            let _ = tx.send(ServerMessage::ActionRejected { reason: format!("match setup failed: {error:?}") });
            return;
        }
    };

    let bot_agent = make_serve_agent(shared.options.bot_runner, bot_side, seed.wrapping_add(1));
    let (corp_slot, runner_slot) = match human_side {
        Side::Corp => (slot, PlayerSlot::Bot(bot_agent)),
        Side::Runner => (PlayerSlot::Bot(bot_agent), slot),
    };
    start_match(shared, state, corp_slot, runner_slot, Uuid::new_v4());
}

/// First connection under `ServeBotKind::None` waits in the lobby; the
/// second connection pairs with it and starts the match for both.
async fn pair_with_human(
    shared: Shared,
    preferred_side: Option<Side>,
    tx: mpsc::UnboundedSender<ServerMessage>,
    slot: PlayerSlot,
    seed: u64,
) {
    let mut guard = shared.lobby.lock().await;
    let Some(first) = guard.take() else {
        *guard = Some(PendingHuman { preferred_side, tx, slot });
        return;
    };
    drop(guard);

    let second = PendingHuman { preferred_side, tx, slot };
    let (corp, runner) = assign_sides(first, second);

    let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
    let (state, _events) = match GameState::setup(&corp_deck, &runner_deck, &shared.registry, seed) {
        Ok(setup) => setup,
        Err(error) => {
            let reason = format!("match setup failed: {error:?}");
            let _ = corp.tx.send(ServerMessage::ActionRejected { reason: reason.clone() });
            let _ = runner.tx.send(ServerMessage::ActionRejected { reason });
            return;
        }
    };
    start_match(shared, state, corp.slot, runner.slot, Uuid::new_v4());
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
