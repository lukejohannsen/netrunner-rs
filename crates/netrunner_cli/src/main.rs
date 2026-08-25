mod app;
mod config;
mod decks;
mod headless;
mod tui;

use clap::Parser;

use config::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::parse();
    if config.headless {
        headless::run(&config)
    } else {
        tui::run(&config)
    }
}
