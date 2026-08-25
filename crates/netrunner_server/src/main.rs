use std::sync::Arc;

use clap::{Parser, ValueEnum};
use futures_util::StreamExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

use netrunner_bots::{BotAgent, HeuristicAgent, MctsAgent, RandomAgent};
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{GamePhase, GameState, Side};
use netrunner_server::protocol::ClientMessage;
use netrunner_server::{fixtures, net, MatchSession, PlayerSlot, ServerMessage};

#[derive(Parser, Debug)]
#[command(name = "netrunner_server", about = "Authoritative match host for netrunner_core")]
struct Config {
    /// Run headless bot-vs-bot self-play instead of hosting a network daemon.
    #[arg(long)]
    headless: bool,

    /// Run a standalone WebSocket daemon that accepts remote clients
    /// (e.g. `netrunner_cli --mode remote`).
    #[arg(long)]
    serve: bool,

    /// (headless mode) Which `netrunner_bots` agent controls the Corp side.
    #[arg(long, value_enum)]
    corp: Option<BotKind>,

    /// (headless mode) Which `netrunner_bots` agent controls the Runner side.
    #[arg(long, value_enum)]
    runner: Option<BotKind>,

    /// (headless mode) Number of games to simulate.
    #[arg(long, default_value_t = 1)]
    games: u32,

    /// Deterministic RNG seed. Headless mode derives each game's seed from
    /// it; serve mode derives each accepted match's seed from it.
    #[arg(long)]
    seed: Option<u64>,

    /// (serve mode) Host to bind.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// (serve mode) Port to bind.
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// (serve mode) Bot opponent seated against every connecting client.
    /// `none` instead queues connecting clients and pairs the first two
    /// together as a human-vs-human match.
    #[arg(long, value_enum, default_value_t = ServeBotKind::Heuristic)]
    bot_runner: ServeBotKind,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum BotKind {
    Random,
    Heuristic,
    Mcts,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum ServeBotKind {
    Heuristic,
    Mcts,
    None,
}

fn make_agent(kind: BotKind, side: Side, seed: u64) -> Box<dyn BotAgent> {
    match kind {
        BotKind::Random => Box::new(RandomAgent::new(seed)),
        BotKind::Heuristic => Box::new(HeuristicAgent::new(side, seed)),
        BotKind::Mcts => Box::new(MctsAgent::new(side, seed)),
    }
}

fn make_serve_agent(kind: ServeBotKind, side: Side, seed: u64) -> Box<dyn BotAgent> {
    match kind {
        ServeBotKind::Heuristic => Box::new(HeuristicAgent::new(side, seed)),
        ServeBotKind::Mcts => Box::new(MctsAgent::new(side, seed)),
        ServeBotKind::None => unreachable!("caller only invokes this for a bot-backed ServeBotKind"),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::parse();
    match (config.headless, config.serve) {
        (true, true) => Err("--headless and --serve are mutually exclusive".into()),
        (false, false) => Err("pass either --headless or --serve".into()),
        (true, false) => run_headless(&config).await,
        (false, true) => run_serve(&config).await,
    }
}

async fn run_headless(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let corp_kind = config.corp.ok_or("--headless requires --corp")?;
    let runner_kind = config.runner.ok_or("--headless requires --runner")?;

    let registry = fixtures::kate_vs_hb_registry();
    let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
    let base_seed = config.seed.unwrap_or_else(rand::random);

    let mut corp_wins = 0u32;
    let mut runner_wins = 0u32;

    for game_index in 0..config.games {
        let seed = base_seed.wrapping_add(u64::from(game_index));
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

        let corp_slot = PlayerSlot::Bot(make_agent(corp_kind, Side::Corp, seed));
        let runner_slot = PlayerSlot::Bot(make_agent(runner_kind, Side::Runner, seed.wrapping_add(1)));
        let session = MatchSession::new(state, registry.clone(), corp_slot, runner_slot);

        let final_state = session.run().await;
        match final_state.phase {
            GamePhase::GameOver(Side::Corp) => corp_wins += 1,
            GamePhase::GameOver(Side::Runner) => runner_wins += 1,
            _ => return Err(format!("game {game_index} did not reach GameOver").into()),
        }
    }

    println!("{} games completed without panics ({corp_wins} Corp wins / {runner_wins} Runner wins)", config.games);
    Ok(())
}

/// A connected-but-unmatched human client, waiting in the lobby for a
/// second human to pair against (only populated when `--bot-runner none`).
struct PendingHuman {
    preferred_side: Option<Side>,
    tx: mpsc::UnboundedSender<ServerMessage>,
    slot: PlayerSlot,
}

type Lobby = Arc<Mutex<Option<PendingHuman>>>;

async fn run_serve(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(%addr, bot_runner = ?config.bot_runner, "netrunner_server listening");

    let registry = fixtures::kate_vs_hb_registry();
    let lobby: Lobby = Arc::new(Mutex::new(None));
    let bot_runner = config.bot_runner;
    let base_seed = config.seed.unwrap_or_else(rand::random);

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let registry = registry.clone();
        let lobby = lobby.clone();
        let seed = base_seed.wrapping_add(rand::random::<u64>());
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, registry, lobby, bot_runner, seed).await {
                tracing::warn!(%peer_addr, ?error, "connection ended with an error");
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    registry: CardRegistry,
    lobby: Lobby,
    bot_runner: ServeBotKind,
    seed: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ws_stream = tokio_tungstenite::accept_async(stream).await?;

    let (player_name, preferred_side) = loop {
        match ws_stream.next().await {
            Some(Ok(WsMessage::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Connect { player_name, preferred_side }) => break (player_name, preferred_side),
                _ => continue,
            },
            Some(Ok(_)) => continue,
            _ => return Err("connection closed before handshake".into()),
        }
    };
    tracing::info!(%player_name, ?preferred_side, "client connected");

    let (session_tx, bridge_rx) = mpsc::unbounded_channel::<ServerMessage>();
    let (bridge_tx, session_rx) = mpsc::unbounded_channel::<ClientMessage>();
    tokio::spawn(net::bridge_websocket(ws_stream, bridge_tx, bridge_rx));

    let slot = PlayerSlot::Channel { tx: session_tx.clone(), rx: session_rx };

    if bot_runner == ServeBotKind::None {
        pair_with_human(lobby, registry, preferred_side, session_tx, slot, seed).await;
    } else {
        spawn_vs_bot(registry, preferred_side, bot_runner, session_tx, slot, seed);
    }
    Ok(())
}

fn spawn_vs_bot(
    registry: CardRegistry,
    preferred_side: Option<Side>,
    bot_runner: ServeBotKind,
    tx: mpsc::UnboundedSender<ServerMessage>,
    slot: PlayerSlot,
    seed: u64,
) {
    let human_side = preferred_side.unwrap_or(Side::Corp);
    let bot_side = human_side.other();
    let match_id = Uuid::new_v4();
    let _ = tx.send(ServerMessage::MatchJoined { match_id, assigned_side: human_side });

    let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
    let (state, _events) = match GameState::setup(&corp_deck, &runner_deck, &registry, seed) {
        Ok(setup) => setup,
        Err(error) => {
            let _ = tx.send(ServerMessage::ActionRejected { reason: format!("match setup failed: {error:?}") });
            return;
        }
    };

    let bot_agent = make_serve_agent(bot_runner, bot_side, seed.wrapping_add(1));
    let (corp_slot, runner_slot) = match human_side {
        Side::Corp => (slot, PlayerSlot::Bot(bot_agent)),
        Side::Runner => (PlayerSlot::Bot(bot_agent), slot),
    };

    tokio::spawn(async move {
        MatchSession::new(state, registry, corp_slot, runner_slot).run().await;
    });
}

/// First connection under `--bot-runner none` waits in the lobby; the
/// second connection pairs with it and starts the match for both.
async fn pair_with_human(
    lobby: Lobby,
    registry: CardRegistry,
    preferred_side: Option<Side>,
    tx: mpsc::UnboundedSender<ServerMessage>,
    slot: PlayerSlot,
    seed: u64,
) {
    let mut guard = lobby.lock().await;
    let Some(first) = guard.take() else {
        *guard = Some(PendingHuman { preferred_side, tx, slot });
        return;
    };
    drop(guard);

    let second = PendingHuman { preferred_side, tx, slot };
    let (corp, runner) = assign_sides(first, second);

    let match_id = Uuid::new_v4();
    let _ = corp.tx.send(ServerMessage::MatchJoined { match_id, assigned_side: Side::Corp });
    let _ = runner.tx.send(ServerMessage::MatchJoined { match_id, assigned_side: Side::Runner });

    let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
    let (state, _events) = match GameState::setup(&corp_deck, &runner_deck, &registry, seed) {
        Ok(setup) => setup,
        Err(error) => {
            let reason = format!("match setup failed: {error:?}");
            let _ = corp.tx.send(ServerMessage::ActionRejected { reason: reason.clone() });
            let _ = runner.tx.send(ServerMessage::ActionRejected { reason });
            return;
        }
    };

    tokio::spawn(async move {
        MatchSession::new(state, registry, corp.slot, runner.slot).run().await;
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
