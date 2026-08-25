use clap::{Parser, ValueEnum};

use netrunner_bots::{BotAgent, HeuristicAgent, MctsAgent, RandomAgent};
use netrunner_core::rules::{GamePhase, GameState, Side};
use netrunner_server::{fixtures, MatchSession, PlayerSlot};

#[derive(Parser, Debug)]
#[command(name = "netrunner_server", about = "Authoritative match host for netrunner_core — headless bot-vs-bot driver")]
struct Config {
    /// Only mode this binary currently implements — kept as an explicit
    /// flag for symmetry with `netrunner_cli` and forward-compatibility
    /// with a future network-hosted mode.
    #[arg(long)]
    headless: bool,

    #[arg(long, value_enum)]
    corp: BotKind,

    #[arg(long, value_enum)]
    runner: BotKind,

    #[arg(long, default_value_t = 1)]
    games: u32,

    #[arg(long)]
    seed: Option<u64>,
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
    if !config.headless {
        return Err("only --headless is currently supported".into());
    }

    let registry = fixtures::kate_vs_hb_registry();
    let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
    let base_seed = config.seed.unwrap_or_else(rand::random);

    let mut corp_wins = 0u32;
    let mut runner_wins = 0u32;

    for game_index in 0..config.games {
        let seed = base_seed.wrapping_add(u64::from(game_index));
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

        let corp_slot = PlayerSlot::Bot(make_agent(config.corp, Side::Corp, seed));
        let runner_slot = PlayerSlot::Bot(make_agent(config.runner, Side::Runner, seed.wrapping_add(1)));
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
