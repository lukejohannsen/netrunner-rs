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

    /// Which `netrunner_bots` agent (if any) controls the Corp side.
    /// `Human` is the default for both sides. In `--headless` mode, where a
    /// `Human` agent can't make progress, `Human` is treated as `Random`
    /// instead (see `headless::run`). Interactive (non-headless) mode
    /// requires exactly one of `--corp`/`--runner` to be `Human` — the CLI
    /// only ever hosts one human seat, submitting actions through a
    /// `netrunner_server::MatchSession` channel exactly the way a real
    /// (future) remote client would; the other side must be a bot.
    #[arg(long, value_enum, default_value_t = BotKind::Human)]
    pub corp: BotKind,

    /// Which `netrunner_bots` agent (if any) controls the Runner side. See
    /// `corp`'s doc comment for the `Human`/headless-mode caveats.
    #[arg(long, value_enum, default_value_t = BotKind::Human)]
    pub runner: BotKind,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotKind {
    Human,
    Random,
    Heuristic,
    Mcts,
}
