mod app;
mod bots;
mod config;
mod decks;
mod headless;
mod tui;

use clap::Parser;

use config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::parse();
    if config.headless {
        headless::run(&config).await
    } else {
        tui::run(&config).await
    }
}
