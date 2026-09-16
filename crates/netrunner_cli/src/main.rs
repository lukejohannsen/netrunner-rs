mod app;
mod bench;
mod bots;
mod cards;
mod config;
mod deck;
mod diag;
mod headless;
mod learn;
use netrunner_client::prose;
mod ratings;
mod remote;
mod replay;
mod settings;
mod tui;

use clap::{CommandFactory, FromArgMatches};
use netrunner_client::decks;

use config::{Command, Config, DiagAction};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Config::command().get_matches();
    let mut config = Config::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    // What the player set in the menu's Settings, under any flag they typed.
    // A settings file that will not parse is reported and ignored rather
    // than fatal: it holds defaults, and every flag still works without it.
    if settings::applies_to(&config) {
        match settings::resolve_settings_file().and_then(|path| settings::Settings::load(&path)) {
            Ok(saved) => settings::apply(&saved, &mut config, |id| settings::was_flagged(&matches, id)),
            Err(error) => eprintln!("warning: settings ignored: {error}"),
        }
    }
    // Taken out of `config` rather than matched in place, so the subcommand
    // arms can still borrow the global flags (`--decks-dir`, `--format`)
    // alongside their own action.
    match config.command.take() {
        Some(Command::Bench { bots, games, seed, simulations, determinizations, shared_sample, mcts_depth, pairings, threads, report, ratings, label }) => {
            let args =
                bench::BenchArgs { bots, games, seed, simulations, determinizations, shared_sample, mcts_depth, pairings, threads, report, ratings, label };
            bench::run(&args, &config)
        }
        Some(Command::Diag {
            action: DiagAction::LeafSensitivity { games, positions, seed, determinizations, rollouts, depths, source, threads, report },
        }) => {
            let args = diag::leaf_sensitivity::LeafSensitivityArgs {
                games,
                positions,
                seed,
                determinizations,
                rollouts,
                depths,
                source,
                threads,
                report,
            };
            diag::leaf_sensitivity::run(&args, &config)
        }
        Some(Command::Diag {
            action: DiagAction::RezRate { games, seed, corp, runner, simulations, determinizations, threads, report },
        }) => {
            let args =
                diag::rez_rate::RezRateArgs { corp, runner, games, seed, simulations, determinizations, threads, report };
            diag::rez_rate::run(&args, &config)
        }
        Some(Command::Diag {
            action: DiagAction::Tempo { games, seed, corp, runner, simulations, determinizations, turns, threads, report },
        }) => {
            let args =
                diag::tempo::TempoArgs { corp, runner, games, seed, simulations, determinizations, turns, threads, report };
            diag::tempo::run(&args, &config)
        }
        Some(Command::Matches) => remote::print_matches(&config.server).await,
        Some(Command::Ratings) => ratings::print(&config),
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
                tui::run(&mut config).await
            }
        }
    }
}
