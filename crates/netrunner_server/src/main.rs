use std::time::Duration;

use clap::{Parser, ValueEnum};

use netrunner_bots::{BotAgent, HeuristicAgent, MctsAgent, RandomAgent};
use netrunner_core::rules::{GamePhase, GameState, Side};
use netrunner_server::serve::{ServeBotKind, ServeOptions, Server};
use netrunner_server::{fixtures, MatchSession, PlayerSlot, DEFAULT_RECONNECT_GRACE};

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

    /// Deterministic RNG seed. Both modes derive the n-th game's seed as
    /// `seed + n`, so a `--seed` daemon plays the same opening for its
    /// n-th match every time it is started.
    #[arg(long)]
    seed: Option<u64>,

    /// (serve mode) Host to bind.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// (serve mode) Port to bind.
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// (serve mode) Bot opponent seated against every connecting client
    /// (on whichever side the client did not ask for). `none` instead
    /// queues connecting clients and pairs each with the first waiter in
    /// its room as a human-vs-human match.
    #[arg(long, value_enum, default_value_t = ServeBotKind::Heuristic)]
    bot_runner: ServeBotKind,

    /// (serve mode) The bot's personality: `balanced`, or `rush`,
    /// `glacier`, `trap` for a Corp bot and `aggressive`, `cautious` for
    /// a Runner bot. See `netrunner_cli --corp-personality`.
    #[arg(long, default_value_t = netrunner_bots::Personality::Balanced)]
    bot_personality: netrunner_bots::Personality,

    /// (serve mode) How many matches may run at once. A client connecting
    /// at the limit is refused rather than queued. Unlimited if omitted.
    #[arg(long)]
    max_matches: Option<usize>,

    /// (serve mode) How many seconds a match waits for a disconnected
    /// player whose action it needs before awarding the game to the other
    /// side. A client resumes with the session token from `MatchJoined`.
    #[arg(long, default_value_t = DEFAULT_RECONNECT_GRACE.as_secs())]
    reconnect_grace_secs: u64,

    /// (serve mode) How many seconds a player has to answer each decision
    /// it is offered before forfeiting the match — per decision, not per
    /// turn, and neither a rejected action nor a reconnect restarts it.
    /// No clock if omitted.
    #[arg(long)]
    turn_timeout_secs: Option<u64>,

    /// (serve mode) Rate every finished match — surrenders, disconnects
    /// and timeouts count as losses; stalls are unrated — into the
    /// Glicko-2 rating book at this path, on the human-vs-bot track under
    /// a bot opponent and the human-vs-human track under `--bot-runner
    /// none`, a rating per side. Nothing is rated without it.
    #[arg(long)]
    ratings_file: Option<std::path::PathBuf>,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum BotKind {
    Random,
    Heuristic,
    Mcts,
}

fn make_agent(kind: BotKind, side: Side, seed: u64) -> Box<dyn BotAgent> {
    match kind {
        BotKind::Random => Box::new(RandomAgent::new(seed)),
        BotKind::Heuristic => Box::new(HeuristicAgent::new(side, seed)),
        BotKind::Mcts => Box::new(MctsAgent::new(side, seed)),
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

async fn run_serve(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let options = ServeOptions {
        bot_runner: config.bot_runner,
        bot_personality: config.bot_personality,
        seed: config.seed,
        reconnect_grace: Duration::from_secs(config.reconnect_grace_secs),
        max_matches: config.max_matches,
        turn_timeout: config.turn_timeout_secs.map(Duration::from_secs),
        ratings_file: config.ratings_file.clone(),
    };
    let server = Server::bind(&format!("{}:{}", config.host, config.port), options).await?;
    server.run().await?;
    Ok(())
}
