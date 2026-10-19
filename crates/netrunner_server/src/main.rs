use std::time::Duration;

use clap::{Parser, ValueEnum};

use netrunner_bots::{BotAgent, Knowledge, MctsAgent, PlanningAgent, RandomAgent};
use netrunner_core::rules::{GamePhase, GameState, Side};
use netrunner_server::serve::{ServeOptions, Server};
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

    /// (serve mode) Host to bind: an address or a name. `::` is every
    /// IPv6 address, and IPv4 too wherever the OS maps it (Linux and macOS
    /// by default, not Windows); `0.0.0.0` is every IPv4 address.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// (serve mode) Port to bind.
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// (serve mode) The bots this daemon offers: the rungs of the
    /// difficulty ladder — `novice`, `apprentice`, `operator`, `veteran`,
    /// `elite`, or `1`-`5` — comma-separated, or `none`. Every rung if
    /// omitted. **Never where a bot sits** (Phase 4 §9): a bot sits only in
    /// a casual lobby, and only when a player looking for a game asks for
    /// one at a rung offered here, in a style of their choosing or the
    /// style of the deck it is dealt; the Ranked queues never seat one.
    #[arg(long, value_parser = parse_bots, default_value = "novice,apprentice,operator,veteran,elite")]
    bots: Bots,

    /// (serve mode) How many matches may run at once. A client connecting
    /// at the limit is refused rather than queued. Unlimited if omitted.
    #[arg(long)]
    max_matches: Option<usize>,

    /// (serve mode) How many spectators each match takes; 32 if omitted,
    /// 0 for no limit. A watcher past it is refused.
    #[arg(long)]
    max_spectators: Option<usize>,

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

    /// (serve mode) How many minutes a tournament round runs before time
    /// is called: no seat is given at its tables after that, and a game
    /// under way finishes the turn in play and one more, then goes to
    /// agenda points (Organized Play Policies 1.1.5.3). Forty, as
    /// single-sided Swiss is run (1.1.5.2).
    #[arg(long, default_value_t = 40)]
    round_minutes: u64,

    /// (serve mode) Pin the Corp's published decklist by id (e.g.
    /// `fine_print`) instead of rotating the sample pool with each
    /// match's seed. Each side is pinned independently.
    #[arg(long)]
    corp_deck: Option<String>,

    /// (serve mode) Pin the Runner's published decklist by id (e.g.
    /// `sabbatical`). See `--corp-deck`.
    #[arg(long)]
    runner_deck: Option<String>,

    /// (serve mode) The lobbies: the formats this daemon pairs players
    /// in, comma-separated or repeated, each a queue of its own. A client
    /// that names no format joins the first. Every deck the daemon can
    /// deal, pinned or rotating, is checked against each at bind, so an
    /// illegal pool is a startup error rather than a bad match. All five
    /// by default (`serve::DEFAULT_FORMATS`); a format no shipped deck is
    /// legal in — Snapshot, a Fantasy Flight Games pool — is served for
    /// people alone, who bring their own decks, and a seek there waits for
    /// a person rather than being dealt a bot.
    #[arg(long = "format", value_enum, value_delimiter = ',', default_values_t = [ServeFormat::Startup, ServeFormat::Standard, ServeFormat::Eternal, ServeFormat::Snapshot, ServeFormat::Unrestricted])]
    formats: Vec<ServeFormat>,


    /// (serve mode) Where the daemon keeps what outlives it: its own key
    /// (`identity.key`, made on first start), the store (`store.sqlite`,
    /// made on first start unless `--database` names another: the
    /// players it has seen, a Glicko-2 ladder per format, the results,
    /// the tournaments), the results log (`results.jsonl`, the signed
    /// receipts the store is rebuilt from) and a record of every
    /// finished match (`matches/<yyyy-mm>/<id>.jsonl`, which
    /// `netrunner_cli replay` opens).
    /// With it the daemon's key is the same every run, which clients
    /// remember, and every finished match between two identified people
    /// in a Ranked queue is rated — surrenders, disconnects and
    /// timeouts count as losses; stalls are unrated. Without it the key is
    /// new each run, nothing is rated and there are no Ranked queues. A
    /// game against a bot, or with a player who proved no key, is never
    /// rated.
    #[arg(long)]
    data_dir: Option<std::path::PathBuf>,

    /// (serve mode) The store's database, as a URL: `postgres://…` for a
    /// server that has outgrown a file, or a `sqlite://…` elsewhere than
    /// the data directory. Without it the store is `store.sqlite` in the
    /// data directory, and nothing has to be set up.
    #[arg(long, requires = "data_dir")]
    database: Option<String>,

    /// (serve mode) A key that may hold and close an event (Phase 4
    /// §11) — `key:<base32>`, as `netrunner_cli identity` prints it —
    /// repeatable. An event grants titles and badges the profile wears
    /// everywhere on this server, so only an operator's key holds one; a
    /// tournament, whose only prize is its standings, is anyone's.
    #[arg(long = "operator", value_parser = parse_key, requires = "data_dir")]
    operators: Vec<netrunner_identity::PublicKey>,

    /// (serve mode) Rebuild the store's ratings and results from
    /// `results.jsonl` in the data directory before serving. The results
    /// log is the truth and the store its index: a corrupt store or a
    /// change to the rating system is put right by this, every game an
    /// operator voided (`voided.jsonl`, `netrunner_cli admin game void`)
    /// is left out, and every receipt is checked against the daemon's
    /// key as it is folded. Never delete a line to void a game: each is a
    /// receipt the players hold too. The players, the awards, the pairing
    /// memory and the tournaments are not the log's and are kept.
    #[arg(long, requires = "data_dir")]
    rebuild: bool,
}

/// `NsgFormat` as a command-line value — a separate enum for the same
/// reason `netrunner_cli`'s `FormatArg` is one: deriving `ValueEnum` on the
/// core type would put a `clap` dependency in `netrunner_core`, which the
/// decoupled-engine rule forbids.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
enum ServeFormat {
    Startup,
    Standard,
    Eternal,
    Snapshot,
    Unrestricted,
}

impl From<ServeFormat> for netrunner_core::format::NsgFormat {
    fn from(arg: ServeFormat) -> Self {
        use netrunner_core::format::NsgFormat;
        match arg {
            ServeFormat::Startup => NsgFormat::Startup,
            ServeFormat::Standard => NsgFormat::Standard,
            ServeFormat::Eternal => NsgFormat::Eternal,
            ServeFormat::Snapshot => NsgFormat::Snapshot,
            ServeFormat::Unrestricted => NsgFormat::Unrestricted,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum BotKind {
    Random,
    Planner,
    Mcts,
}

/// A bot that knows the deck it is dealt and the match's format — the
/// narrowest both dealt decks are legal in (`decks::match_format`).
fn make_agent(kind: BotKind, side: Side, seed: u64, deck: &netrunner_core::rules::Deck, format: netrunner_core::format::NsgFormat) -> Box<dyn BotAgent> {
    let knowledge = Knowledge::new(format, Some(deck.clone()));
    match kind {
        BotKind::Random => Box::new(RandomAgent::new(seed)),
        BotKind::Planner => Box::new(PlanningAgent::new(side, seed).with_knowledge(knowledge)),
        BotKind::Mcts => Box::new(MctsAgent::new(side, seed).with_knowledge(knowledge)),
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

    let registry = fixtures::sample_registry();
    let base_seed = config.seed.unwrap_or_else(rand::random);

    let mut corp_wins = 0u32;
    let mut runner_wins = 0u32;

    for game_index in 0..config.games {
        let seed = base_seed.wrapping_add(u64::from(game_index));
        // The matchup rotates with the seed, as it does in `--serve` and
        // in `netrunner_cli --all-matchups`: a driver that plays one fixed
        // pairing measures that pairing, not the pool.
        let dealt = fixtures::sample_decks_for_seed(seed);
        let (state, _events) = GameState::setup(&dealt.corp, &dealt.runner, &registry, seed)?;

        let format = netrunner_core::decks::match_format(&dealt.corp, &dealt.runner, &registry).ok_or("the dealt decks are legal in no published format")?;
        let corp_slot = PlayerSlot::Bot(make_agent(corp_kind, Side::Corp, seed, &dealt.corp, format));
        let runner_slot = PlayerSlot::Bot(make_agent(runner_kind, Side::Runner, seed.wrapping_add(1), &dealt.runner, format));
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
    if let (true, Some(dir)) = (config.rebuild, &config.data_dir) {
        let folded = netrunner_server::serve::rebuild(dir, config.database.as_deref()).await?;
        tracing::info!(folded, "the store's ratings and results were rebuilt from the results log");
    }
    let options = ServeOptions {
        bots: config.bots.0.clone(),
        seed: config.seed,
        reconnect_grace: Duration::from_secs(config.reconnect_grace_secs),
        max_matches: config.max_matches,
        max_spectators: match config.max_spectators {
            Some(0) => None,
            Some(cap) => Some(cap),
            None => Some(netrunner_server::match_session::DEFAULT_MAX_SPECTATORS),
        },
        turn_timeout: config.turn_timeout_secs.map(Duration::from_secs),
        round_length: Duration::from_secs(config.round_minutes * 60),
        corp_deck: config.corp_deck.clone(),
        runner_deck: config.runner_deck.clone(),
        formats: config.formats.iter().map(|&format| format.into()).collect(),
        data_dir: config.data_dir.clone(),
        operators: config.operators.clone(),
        database: config.database.clone(),
    };
    let server = Server::bind(&bind_address(&config.host, config.port), options).await?;
    server.run().await?;
    Ok(())
}

/// `host:port` as `TcpListener::bind` reads it. An IP address goes through
/// `SocketAddr`, which brackets an IPv6 one — `format!("{host}:{port}")`
/// made `--host ::` into `::::8080`, which does not parse. Anything else is
/// a name and is left to the resolver.
fn bind_address(host: &str, port: u16) -> String {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => std::net::SocketAddr::new(ip, port).to_string(),
        Err(_) => format!("{host}:{port}"),
    }
}

#[cfg(test)]
mod tests {
    use super::bind_address;

    #[test]
    fn an_ipv6_host_is_bracketed_and_a_name_is_left_alone() {
        assert_eq!(bind_address("::", 8080), "[::]:8080");
        assert_eq!(bind_address("[::1]", 8080), "[::1]:8080");
        assert_eq!(bind_address("127.0.0.1", 8080), "127.0.0.1:8080");
        assert_eq!(bind_address("localhost", 8080), "localhost:8080");
    }
}

/// `--bots`: the rungs a daemon offers.
#[derive(Debug, Clone)]
struct Bots(Vec<netrunner_bots::Level>);

fn parse_key(text: &str) -> Result<netrunner_identity::PublicKey, String> {
    text.trim().parse::<netrunner_identity::PublicKey>().map_err(|error| error.to_string())
}

fn parse_bots(text: &str) -> Result<Bots, String> {
    if text.trim().eq_ignore_ascii_case("none") {
        return Ok(Bots(Vec::new()));
    }
    let mut rungs = text.split(',').map(|rung| rung.trim().parse::<netrunner_bots::Level>().map_err(|error| error.to_string())).collect::<Result<Vec<_>, _>>()?;
    rungs.sort();
    rungs.dedup();
    Ok(Bots(rungs))
}
