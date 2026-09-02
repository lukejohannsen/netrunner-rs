use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

#[derive(Parser, Debug)]
#[command(name = "netrunner_cli", about = "Ratatui TUI game harness and headless simulator for netrunner_core")]
pub struct Config {
    /// Manage the local NetrunnerDB card catalog cache instead of playing a
    /// game. Omitted: falls through to the existing TUI/headless behavior
    /// below, unaffected by this field's presence.
    #[command(subcommand)]
    pub command: Option<Command>,

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

    /// (headless) Rotate through every sample-deck matchup
    /// (`netrunner_core::decks::matchups()`) by game index instead of
    /// playing `--corp-deck` vs `--runner-deck` every game — the same
    /// rotation the agent-driven sweeps and self-play use.
    #[arg(long)]
    pub all_matchups: bool,

    /// (headless) Write the rules-coverage report as JSON to this path,
    /// in addition to printing the table. Keys are sorted, so two reports
    /// can be `diff`ed to measure a rules fix before and after.
    #[arg(long)]
    pub report: Option<PathBuf>,

    /// (headless) Print one line per game: seed, matchup, steps, outcome.
    #[arg(long)]
    pub verbose: bool,

    /// (headless) Search iterations per decision for `--corp puct` /
    /// `--runner puct` and `mcts`. Low by default because a coverage run
    /// wants many games, not strong ones.
    #[arg(long, default_value_t = 32)]
    pub simulations: usize,

    /// (headless) Seat the bots through the index-based `ActionSpace`
    /// round trip (`netrunner_single_player`, the RL path) instead of as
    /// view-based `Seat::Agent`s. The two reach different code — see
    /// AGENTS.md's Testing Rule — so a coverage report is worth taking in
    /// both shapes.
    #[arg(long)]
    pub index_path: bool,

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
    /// only ever hosts one human seat; the other side must be a bot.
    ///
    /// Local interactive play pumps a `netrunner_session::Session`
    /// synchronously — no `MatchSession`, no channel, no background task
    /// (see `tui::run_local`). `--mode remote` is the path that submits
    /// actions over a channel to a real `netrunner_server` match.
    #[arg(long, value_enum, default_value_t = BotKind::Human)]
    pub corp: BotKind,

    /// Which `netrunner_bots` agent (if any) controls the Runner side. See
    /// `corp`'s doc comment for the `Human`/headless-mode caveats.
    #[arg(long, value_enum, default_value_t = BotKind::Human)]
    pub runner: BotKind,

    /// `Local` runs the match in this process: interactive play on a
    /// `netrunner_session::Session` (`tui::run_local`), `--headless` on
    /// the same `Session` driving two bots and counting what the rules
    /// actually did (`headless::run`). `Remote` instead
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

    /// Path to the trained ONNX policy driving `--corp onnx`/`--runner onnx`
    /// and `puct-onnx` — the artifact `scripts/run_iteration_loop.py`
    /// promotes each iteration. Ignored unless a side is set to one of
    /// those kinds.
    #[arg(long, default_value = "checkpoints/latest_policy.onnx")]
    pub model: String,

    /// Which deck the Corp plays: a built-in deck id (e.g.
    /// `discretion_advised`), the name of a saved deck in the deck
    /// directory, or a path to a deck file. Run with an unknown name to be
    /// shown what is available.
    #[arg(long = "corp-deck", default_value = "discretion_advised")]
    pub corp_deck: String,

    /// Which deck the Runner plays, in the same forms as `--corp-deck`
    /// (e.g. `stolen_goods`).
    #[arg(long = "runner-deck", default_value = "stolen_goods")]
    pub runner_deck: String,

    /// Where saved decks live. Defaults to the OS data directory
    /// (`~/.local/share/netrunner/decks` on Linux); `NETRUNNER_DECKS_DIR`
    /// sets it persistently, and this flag outranks that.
    #[arg(long = "decks-dir", global = true)]
    pub decks_dir: Option<PathBuf>,

    /// Which format to check deck legality against.
    ///
    /// Startup is the default because it is the pool this engine actually
    /// ships: System Gateway plus Elevation. The Core Set cards are not
    /// Startup-legal — correctly so — and need `--format eternal`.
    #[arg(long, value_enum, default_value_t = FormatArg::Startup, global = true)]
    pub format: FormatArg,
}

/// `NsgFormat` as a command-line value.
///
/// A separate enum rather than `ValueEnum` on `netrunner_core::format::NsgFormat`
/// itself: deriving it there would put a `clap` dependency in the engine,
/// which AGENTS.md's decoupled-engine rule forbids.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatArg {
    Startup,
    Standard,
    Eternal,
    Snapshot,
}

impl From<FormatArg> for NsgFormat {
    fn from(arg: FormatArg) -> Self {
        match arg {
            FormatArg::Startup => NsgFormat::Startup,
            FormatArg::Standard => NsgFormat::Standard,
            FormatArg::Eternal => NsgFormat::Eternal,
            FormatArg::Snapshot => NsgFormat::Snapshot,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotKind {
    Human,
    Random,
    Heuristic,
    Mcts,
    /// `netrunner_bots::PuctAgent` over the uniform policy — the search
    /// shape self-play trains with, minus the network. The seating a
    /// coverage report should include, since it is the one that generates
    /// training data.
    Puct,
    /// `netrunner_bots::PuctAgent` over the policy/value network at
    /// `--model` — the search *with* the network it was trained for, which
    /// is what `netrunner_selfplay` runs and therefore the seating that
    /// says whether a training run helped. Distinct from `Onnx` below,
    /// which is the bare policy head with no search. Has a `BotAgent` form
    /// (it is a `PuctAgent`), so it seats anywhere `puct` does, including
    /// `--headless`. Requires building with `--features onnx`.
    PuctOnnx,
    /// A policy network trained by `scripts/run_iteration_loop.py`, loaded
    /// from `--model`. Requires building with `--features onnx`, and is
    /// supported only in local interactive play — not because it sees too
    /// much (`OnnxPolicyEvaluator` encodes through `encode_observation`,
    /// which builds a `ClientView` for its own side, so its features are
    /// masked like every other agent's) but because it has no `BotAgent`
    /// form: it implements the index-based `Agent` shape and so cannot
    /// fill a `netrunner_session::Seat::Agent`. See `bots::make_agent`,
    /// which returns `None` for this kind.
    Onnx,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Local,
    Remote,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List or fetch NetrunnerDB card sets into the local cache.
    Cards {
        #[command(subcommand)]
        action: CardsAction,
    },

    /// Inspect, build and edit saved decks.
    Deck {
        #[command(subcommand)]
        action: DeckAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum DeckAction {
    /// List every deck, built-in and saved.
    List,

    /// Show a deck: its description, how-to-play notes, cards and legality.
    Show {
        /// A built-in deck id, a saved deck name, or a path to a deck file.
        name: String,
    },

    /// Check a deck against both validators and report what it finds.
    Validate { name: String },

    /// Start a new, empty deck.
    New {
        /// Id for the new deck; also its filename.
        name: String,

        #[arg(long, value_enum)]
        side: SideArg,

        /// The deck's identity card, by id (e.g.
        /// `haas_bioroid_precision_design`) or exact title.
        #[arg(long)]
        identity: String,
    },

    /// Add copies of a card to a saved deck.
    Add {
        name: String,

        /// A card id (e.g. `hedge_fund`) or its exact title.
        card: String,

        #[arg(default_value_t = 1)]
        count: u32,
    },

    /// Remove copies of a card from a saved deck.
    Remove {
        name: String,
        card: String,

        #[arg(default_value_t = 1)]
        count: u32,
    },
}

#[derive(Subcommand, Debug)]
pub enum CardsAction {
    /// List NetrunnerDB sets (packs) available to sync.
    ListSets,

    /// Fetch and cache card data from NetrunnerDB.
    Sync {
        /// Sync every set.
        #[arg(long)]
        all: bool,

        /// Sync only these set codes (repeatable), e.g. `--set sg --set elev`.
        #[arg(long = "set")]
        set: Vec<String>,
    },
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
