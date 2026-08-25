use clap::{Parser, ValueEnum};

#[derive(Parser, Debug, Clone)]
#[command(name = "netrunner_cli", about = "Ratatui TUI game harness and headless simulator for netrunner_core")]
pub struct Config {
    /// Launch the Ratatui TUI dashboard. This is the default mode — the
    /// flag exists for discoverability/documentation symmetry with
    /// `--headless`, not because anything reads it: mode selection is
    /// simply `if headless { headless } else { interactive }`.
    #[arg(long, default_value_t = true)]
    pub interactive: bool,

    /// Run headless non-rendered match simulation instead of the TUI.
    #[arg(long)]
    pub headless: bool,

    /// Number of headless games to simulate.
    #[arg(long, default_value_t = 100)]
    pub games: u32,

    /// Deterministic RNG seed. Interactive mode seeds its one game with
    /// this value directly; headless mode derives each game's seed from it.
    /// Omitted: a fresh OS-random seed is picked (so re-runs aren't
    /// reproducible unless a seed is pinned explicitly).
    #[arg(long)]
    pub seed: Option<u64>,

    /// Whose perspective the TUI renders the board from. No effect in
    /// headless mode.
    #[arg(long, value_enum, default_value_t = ViewAs::Omniscient)]
    pub view_as: ViewAs,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewAs {
    Corp,
    Runner,
    Omniscient,
}
