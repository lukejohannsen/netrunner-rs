mod app;
mod bots;
mod cards;
mod config;
mod decks;
mod headless;
mod remote;
mod tui;

use clap::Parser;

use config::{Command, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::parse();
    match config.command {
        Some(Command::Cards { action }) => cards::run(action).await,
        None => {
            if config.headless {
                headless::run(&config).await
            } else {
                tui::run(&config).await
            }
        }
    }
}
