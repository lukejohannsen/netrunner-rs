use clap::{Parser, ValueEnum};

use netrunner_core::rules::Side;

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

    /// `Local` spawns an in-process `MatchSession` (the historical
    /// behavior, unaffected by this flag's default). `Remote` instead
    /// connects to a `netrunner_server --serve` daemon over WebSocket at
    /// `--server`; `--corp`/`--runner`/`--headless`/`--games` are ignored
    /// in this mode — use `--side` to request a seat instead.
    #[arg(long, value_enum, default_value_t = Mode::Local)]
    pub mode: Mode,

    /// (remote mode) WebSocket URL of the `netrunner_server --serve` daemon.
    #[arg(long, default_value = "ws://127.0.0.1:8080")]
    pub server: String,

    /// (remote mode) Preferred seat to request from the server. Omitted:
    /// the server assigns whichever side is available.
    #[arg(long, value_enum)]
    pub side: Option<SideArg>,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotKind {
    Human,
    Random,
    Heuristic,
    Mcts,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Local,
    Remote,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideArg {
    Corp,
    Runner,
}

impl From<SideArg> for Side {
    fn from(side: SideArg) -> Side {
        match side {
            SideArg::Corp => Side::Corp,
            SideArg::Runner => Side::Runner,
        }
    }
}
