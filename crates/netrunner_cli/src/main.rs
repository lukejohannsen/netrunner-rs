mod app;
mod bots;
mod cards;
mod config;
mod deck;
mod deck_store;
mod decks;
mod headless;
mod learn;
mod remote;
mod replay;
mod tui;

use clap::Parser;

use config::{Command, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::parse();
    // Taken out of `config` rather than matched in place, so the subcommand
    // arms can still borrow the global flags (`--decks-dir`, `--format`)
    // alongside their own action.
    match config.command.take() {
        Some(Command::Cards { action }) => cards::run(action).await,
        Some(Command::Deck { action }) => deck::run(action, &config),
        Some(Command::Learn { action }) => learn::run(action, &config),
        Some(Command::Replay { file, side }) => {
            let replay = replay::Replay::open(&file, decks::sample_deck_registry(), side.into())?;
            tui::run_replay(replay)
        }
        None => {
            if config.headless {
                headless::run(&config)
            } else {
                tui::run(&config).await
            }
        }
    }
}
