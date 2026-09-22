use std::path::PathBuf;

use netrunner_bots::{Level, Personality};
use netrunner_core::decks::DeckFile;
use clap::{Parser, Subcommand, ValueEnum};

use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

#[derive(Parser, Debug, Clone)]
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

    /// (headless) Record every game's full action/event history as
    /// JSON-Lines under this directory, one `game_NNNNN.jsonl` per game: a
    /// header line naming the seed, both decks and the match rules, then
    /// one `HistoryEntry` per line. Replaying the actions from the header's
    /// setup reproduces the final state exactly. The record is the *raw*
    /// history — what a replay viewer masks per side when rendering — not
    /// anything a seat was sent.
    #[arg(long)]
    pub record: Option<PathBuf>,

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
    /// with exactly one side a bot plays that game straight away; with
    /// neither set it opens the main menu (`tui::menu`), where every
    /// choice these flags make is made in the TUI instead. The CLI only
    /// ever hosts one human seat locally.
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

    /// The Corp bot's personality: `balanced`, or a Corp archetype —
    /// `rush` (score early, protect late), `glacier` (build the fort
    /// first), `trap` (wants the Runner's grip thin). A bias on the
    /// evaluator `heuristic`, `mcts` and `puct` share; `random` and a
    /// network-backed `puct-onnx` ignore it.
    ///
    /// **Unset means the deck's own style** (`DeckFile::style` — every
    /// sample deck names one, and a saved deck may), so the bot playing
    /// *Brick Stack* plays like a glacier without being told to. Pass
    /// `balanced` explicitly to override a deck's style with none.
    #[arg(long)]
    pub corp_personality: Option<Personality>,

    /// The Runner bot's personality: `balanced`, or a Runner archetype —
    /// `aggressive` (runs are worth double, tags and subroutines cost
    /// less), `cautious` (a full rig before a run), `builder` (the rig
    /// first, hand on the table) or `wary` (treats face-down ICE as
    /// real). See `--corp-personality`; unset means the deck's own style.
    #[arg(long)]
    pub runner_personality: Option<Personality>,

    /// Seat the Corp as a rung of the difficulty ladder — `novice`,
    /// `apprentice`, `operator`, `veteran`, `elite`, or `1`-`5` — instead
    /// of assembling one out of `--corp` and `--simulations`, both of
    /// which it overrides. The personality is kept: a rung is a strength
    /// and a style is a style, and the two cross (`--corp-level 4
    /// --corp-personality rush`, or a rung playing its deck's own style).
    ///
    /// A rung is a *calibrated* opponent and the flags are not: they let
    /// you build combinations nobody has measured, which is right for a
    /// measurement and wrong for "give me something I can nearly beat".
    /// The two chairs are separate ladders, because the same rung is a
    /// different bot on each side — see `netrunner_bots::difficulty`.
    #[arg(long)]
    pub corp_level: Option<Level>,

    /// Seat the Runner as a rung of the difficulty ladder. See
    /// `--corp-level`; the Runner ladder tops out lower, which is a fact
    /// about the bots and is stated in `netrunner_bots::difficulty`.
    #[arg(long)]
    pub runner_level: Option<Level>,

    /// The name your local games are recorded under (`record.json` in the
    /// OS data directory, beside the saved decks) and the one you connect
    /// to a server as. Defaults to your login name.
    #[arg(long, global = true)]
    pub player: Option<String>,

    /// Where your record against the bots lives; defaults to
    /// `<data dir>/netrunner/record.json`, or `NETRUNNER_RECORD_FILE`. It
    /// is a win/loss log that suggests the rung to try next, not a
    /// rating: a game against a bot is casual, and a rating is a server's
    /// to keep.
    #[arg(long, global = true)]
    pub record_file: Option<PathBuf>,

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

    /// (remote mode, human-vs-human daemon) Only pair with a player who
    /// named the same room. Omitted: the public queue.
    #[arg(long)]
    pub room: Option<String>,

    /// (remote mode) Bring this deck — a built-in id, a saved deck's name,
    /// or a path — instead of being dealt one. Its side is your seat, so
    /// `--side` may be left off (and must agree if given). The server
    /// checks it against its own format and refuses an illegal one.
    #[arg(long)]
    pub deck: Option<String>,

    /// (remote mode) Watch a running match instead of playing: an id from
    /// `netrunner_cli matches`. A spectator sees what both players can
    /// see and nothing either keeps hidden, and has nothing to submit.
    #[arg(long)]
    pub spectate: Option<uuid::Uuid>,

    /// Path to the trained ONNX policy driving `--corp onnx`/`--runner onnx`
    /// and `puct-onnx` — the artifact `scripts/run_iteration_loop.py`
    /// promotes each iteration. Ignored unless a side is set to one of
    /// those kinds.
    #[arg(long, default_value = "data/checkpoints/latest_policy.onnx")]
    pub model: String,

    /// Which deck the Corp plays: a built-in deck id (e.g.
    /// `discretion_advised`), the name of a saved deck in the deck
    /// directory, or a path to a deck file. Run with an unknown name to be
    /// shown what is available.
    #[arg(long = "corp-deck", default_value = netrunner_client::start::DEFAULT_CORP_DECK)]
    pub corp_deck: String,

    /// Which deck the Runner plays, in the same forms as `--corp-deck`
    /// (e.g. `stolen_goods`).
    #[arg(long = "runner-deck", default_value = netrunner_client::start::DEFAULT_RUNNER_DECK)]
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
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
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

/// The settings file stores an `NsgFormat` (it is shared with the desktop
/// client, which has no `FormatArg`); applying it to the command line
/// needs the way back.
impl From<NsgFormat> for FormatArg {
    fn from(format: NsgFormat) -> Self {
        match format {
            NsgFormat::Startup => FormatArg::Startup,
            NsgFormat::Standard => FormatArg::Standard,
            NsgFormat::Eternal => FormatArg::Eternal,
            NsgFormat::Snapshot => FormatArg::Snapshot,
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

/// The id a seat is recorded under is decided by
/// `netrunner_client::record::opponent_id`, over its own flag-free kind.
impl From<BotKind> for netrunner_client::record::BotKind {
    fn from(kind: BotKind) -> Self {
        use netrunner_client::record::BotKind as Recorded;
        match kind {
            BotKind::Human => Recorded::Human,
            BotKind::Random => Recorded::Random,
            BotKind::Heuristic => Recorded::Heuristic,
            BotKind::Mcts => Recorded::Mcts,
            BotKind::Puct => Recorded::Puct,
            BotKind::PuctOnnx => Recorded::PuctOnnx,
            BotKind::Onnx => Recorded::Onnx,
        }
    }
}

/// A bot for the benchmark: a kind and the personality it plays with,
/// spelled `kind` or `kind:personality` on the command line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BotSpec {
    pub kind: BotKind,
    pub personality: Personality,
    /// A difficulty rung instead of a hand-assembled bot, spelled
    /// `level:elite` or `level:3`. `Some` overrides `kind` and
    /// `personality`, and the rung resolves differently on each chair —
    /// which is the point of benchmarking one: `elite` as Corp and
    /// `elite` as Runner are different bots, and the rating book already
    /// rates the two roles separately.
    pub level: Option<Level>,
}

impl std::str::FromStr for BotSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // `level:elite:glacier` crosses a rung with a style, the way
        // `--corp-level 5 --corp-personality glacier` does at a seat. Without
        // it the benchmark could only calibrate the `Balanced` rungs, while
        // play seats every rung in its deck's own style.
        if let Some(level) = s.strip_prefix("level:") {
            let (level, personality) = match level.split_once(':') {
                Some((level, personality)) => (level, personality.parse::<Personality>()?),
                None => (level, Personality::Balanced),
            };
            return Ok(BotSpec { kind: BotKind::Heuristic, personality, level: Some(level.parse::<Level>()?) });
        }
        let (kind, personality) = match s.split_once(':') {
            Some((kind, personality)) => (kind, personality.parse::<Personality>()?),
            None => (s, Personality::Balanced),
        };
        let kind = BotKind::from_str(kind, true).map_err(|_| {
            let names: Vec<&str> = BotKind::value_variants().iter().filter_map(|k| k.to_possible_value()).map(|v| v.get_name().to_string()).collect::<Vec<_>>().leak().iter().map(String::as_str).collect();
            format!("unknown bot kind {kind:?}; one of {}", names.join(", "))
        })?;
        Ok(BotSpec { kind, personality, level: None })
    }
}

/// One ordered benchmark pairing, spelled `CORP/RUNNER` — `heuristic/mcts`,
/// `level:operator/level:elite`. A slash rather than the comma or colon
/// because both of those are already inside a `BotSpec`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BenchPairing {
    pub corp: BotSpec,
    pub runner: BotSpec,
}

impl std::str::FromStr for BenchPairing {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (corp, runner) = s.split_once('/').ok_or_else(|| format!("a pairing is CORP/RUNNER, got {s:?}"))?;
        Ok(BenchPairing { corp: corp.parse()?, runner: runner.parse()? })
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Local,
    Remote,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Play every seating of a set of bots against each other, in
    /// parallel, and rate them on the bot-benchmark track (Glicko-2, a
    /// rating per role). The offline half of Phase 3's rating engine and
    /// the harness for comparing a trained policy against the search it
    /// replaced.
    Bench {
        /// Bots to seat, comma-separated, each a kind with an optional
        /// personality after a colon: `heuristic`, `heuristic:rush`,
        /// `puct:aggressive`. Every ordered pair plays, a bot against
        /// itself included — that pairing is what says whether the Corp
        /// or the Runner chair is the stronger one for a given bot.
        /// `human` and `onnx` cannot be seated here.
        ///
        /// `level:elite` (or `level:5`) seats a rung of the difficulty
        /// ladder instead, which is how the ladder is calibrated: the
        /// rung ignores `--simulations`, and resolves to a different bot
        /// on each chair. `level:elite:glacier` plays the rung in a style.
        #[arg(long, value_delimiter = ',', default_value = "random,heuristic")]
        bots: Vec<BotSpec>,
        /// Games per ordered pairing, rotating through the sample-deck
        /// matchups the way `--headless --all-matchups` does.
        #[arg(long, default_value_t = 12)]
        games: u32,
        /// Base seed; game n of the whole run plays on `seed + n`, so the
        /// same arguments reproduce the same ladder. Random if omitted.
        #[arg(long)]
        seed: Option<u64>,
        /// Search iterations per decision for `mcts`, `puct` and
        /// `puct-onnx`; part of those bots' participant ids (`puct@32`).
        #[arg(long, default_value_t = 32)]
        simulations: usize,
        /// How many independent samples of the hidden state each
        /// decision searches, the `--simulations` budget split evenly
        /// across them: `mcts`'s root-parallel trees and `puct`'s
        /// `samples`, which are one dial under two names. Omitted, each
        /// search keeps its own default (`mcts` four trees, `puct` one
        /// sample) — a fixed number since ROADMAP Phase 2 §5 item 41,
        /// when `mcts`'s stopped coming off the rayon pool. Ignored by
        /// `random` and `heuristic`; use `--label` to keep two settings
        /// apart in a report.
        #[arg(long)]
        determinizations: Option<usize>,
        /// `mcts` only: give every tree the same sample of the hidden
        /// state, so they still search independently but stop disagreeing
        /// about what is behind the ICE. Separates "more samples" from
        /// "the same root action valued several times" — a diagnostic,
        /// not a stronger setting.
        #[arg(long)]
        shared_sample: bool,
        /// `mcts` only: plies from the root to where a playout stops and
        /// is scored, tree descent included (the agent's own is 16). The
        /// reach of a playout into the other side's turn, which is where
        /// a sample of the hidden state starts to matter — see ROADMAP
        /// Phase 2 §5 item 36. Use `--label` to keep two settings apart.
        #[arg(long)]
        mcts_depth: Option<usize>,
        /// How far a position's *stage* may move the evaluator's weights,
        /// from the build archetype toward the pressure one: 0.0 (the
        /// default) is the static evaluator every number here was taken
        /// on, 1.0 is the full travel. ROADMAP Phase 5 §4(c) and §6.
        ///
        /// **One flag for both chairs is the chair isolation**, not a
        /// shortcut: only the Runner arm is staged, so a Corp seat handed
        /// the same gain plays byte-identically to one handed zero.
        #[arg(long, default_value_t = 0.0)]
        stage_gain: f64,
        /// Play only this ordered pairing, `CORP/RUNNER` (repeatable),
        /// each side one of `--bots`. **Every game keeps the index and
        /// seed it has in the full cross product**, so a filtered game is
        /// the same game an unfiltered run plays and the two reports pair
        /// game for game; what is skipped is only the cost. A one-chair
        /// measurement — `--bots heuristic,mcts --pairing heuristic/mcts`
        /// — is a quarter of the work of the whole square.
        #[arg(long = "pairing")]
        pairings: Vec<BenchPairing>,
        /// Worker threads. All cores if omitted.
        #[arg(long)]
        threads: Option<usize>,
        /// Write every game's result, each pairing's tally and the ladder
        /// as JSON here.
        #[arg(long)]
        report: Option<PathBuf>,
        /// A rating book to load before and save after the run, so a
        /// ladder accumulates across runs. Fresh and unsaved if omitted:
        /// a benchmark's point is the code as it stands today.
        #[arg(long)]
        ratings: Option<PathBuf>,
        /// Suffix for every participant id (`heuristic#abc123`), so two
        /// versions of the same bot rate as different participants in a
        /// saved book.
        #[arg(long)]
        label: Option<String>,
    },

    /// Measurements over the bots' internals whose product is a number
    /// rather than a behaviour change. Each one answers an open ROADMAP
    /// question and stays in the tree so the number can be re-taken.
    Diag {
        #[command(subcommand)]
        action: DiagAction,
    },

    /// List the matches the `netrunner_server --serve` daemon at
    /// `--server` is hosting, with the ids `--spectate` takes.
    Matches,

    /// Your record against the bots: wins and losses against every rung
    /// on each chair, and the rung to try next.
    Record,

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

    /// Learn to play: scripted lessons for each side, then the Null Signal
    /// Games starter game.
    Learn {
        #[command(subcommand)]
        action: LearnAction,
    },

    /// Step through a match recorded with `--headless --record`, from one
    /// side's chair: every position is that seat's masked view, and the
    /// log is that seat's masked copy. `s` swaps chairs while viewing.
    Replay {
        /// A `game_NNNNN.jsonl` written by `--record`.
        file: PathBuf,
        /// Whose chair to watch from.
        #[arg(long, value_enum, default_value_t = SideArg::Corp)]
        side: SideArg,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum DiagAction {
    /// How much of a leaf evaluation is the hidden state? Draws N
    /// determinizations of real decision positions and sweeps the
    /// playout depth between the sample and `evaluate_state_with` — 0 is
    /// `puct`'s leaf, 16 is `mcts`'s — reporting how far a sample moves
    /// an action's score against how far apart the actions are, and
    /// whether it ever changes which action looks best. ROADMAP Phase 2
    /// §5 item 35's open question; no games are played to answer it.
    LeafSensitivity {
        /// Games to draw positions from; each plays a different matchup.
        #[arg(long, default_value_t = 8)]
        games: u32,
        /// Positions kept per chair per game, spread evenly over that
        /// chair's decisions rather than taken from the opening.
        #[arg(long, default_value_t = 8)]
        positions: usize,
        /// Base seed; game n plays on `seed + n`, and every
        /// determinization and rollout is seeded off the position, so a
        /// re-run reproduces every number whatever the thread count.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Samples of the hidden state per position — the dial item 35
        /// measured in games, measured here without any.
        #[arg(long, default_value_t = 16)]
        determinizations: usize,
        /// Rollouts per (determinization, action), to separate the
        /// hidden state's effect from the playout's own noise. Ignored
        /// at depth 0, which is deterministic.
        #[arg(long, default_value_t = 4)]
        rollouts: usize,
        /// Playout plies between the sampled state and
        /// `evaluate_state_with`, comma-separated. 0 is a static leaf.
        #[arg(long, value_delimiter = ',', default_value = "0,2,4,8,16")]
        depths: Vec<usize>,
        /// Which bot plays the games the positions come from.
        #[arg(long, default_value = "heuristic")]
        source: BotSpec,
        /// Worker threads. All cores if omitted.
        #[arg(long)]
        threads: Option<usize>,
        /// Write the full per-chair, per-depth table as JSON here.
        #[arg(long)]
        report: Option<PathBuf>,
    },

    /// When the Runner approaches an ICE the Corp could rez, how often
    /// does it? One observation per approach to an unrezzed ICE, counted
    /// over the predicate `unbreakable_unrezzed_ice` itself uses, so the
    /// answer is the discount that term is missing. ROADMAP Phase 2 §5
    /// item 38: the term assumes the rate is 1.0.
    RezRate {
        /// Games to play; game n plays `matchups[n % len]` on `seed + n`.
        #[arg(long, default_value_t = 96)]
        games: u32,
        /// Base seed.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Which bot takes the Corp chair — the chair whose decision is
        /// being measured.
        #[arg(long, default_value = "heuristic")]
        corp: BotSpec,
        /// Which bot takes the Runner chair. It decides which approaches
        /// ever happen, so it is part of the measurement, not a control.
        #[arg(long, default_value = "heuristic")]
        runner: BotSpec,
        /// Search iterations for a searching bot in either chair.
        #[arg(long, default_value_t = 128)]
        simulations: usize,
        /// Hidden-state samples per decision for a searching bot.
        #[arg(long)]
        determinizations: Option<usize>,
        /// Worker threads. All cores if omitted.
        #[arg(long)]
        threads: Option<usize>,
        /// Write every approach, and the rates over them, as JSON here.
        #[arg(long)]
        report: Option<PathBuf>,
    },

    /// *Where* does the Corp put its ICE, and what stands in front of an
    /// agenda when it goes down? ICE installed per central and per
    /// remote, remotes opened, and the ICE on an agenda's server at the
    /// moment it was installed. Phase 5 §19: a report from play that the
    /// `glacier` Corp spread its ICE over every server and never built the
    /// fort its name promises.
    Fort {
        /// Games to play; game n plays `matchups[n % len]` on `seed + n`.
        #[arg(long, default_value_t = 96)]
        games: u32,
        /// Base seed.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Which bot takes the Corp chair (e.g. `level:operator:glacier`).
        #[arg(long, default_value = "heuristic")]
        corp: BotSpec,
        /// Which bot takes the Runner chair.
        #[arg(long, default_value = "heuristic")]
        runner: BotSpec,
        /// Search iterations for a searching bot in either chair.
        #[arg(long, default_value_t = 128)]
        simulations: usize,
        /// Hidden-state samples per decision for a searching bot.
        #[arg(long)]
        determinizations: Option<usize>,
        /// Play only this sample matchup, `CORP_DECK/RUNNER_DECK` by deck
        /// id (e.g. `discretion_advised/stolen_goods`).
        #[arg(long)]
        matchup: Option<String>,
        /// Worker threads. All cores if omitted.
        #[arg(long)]
        threads: Option<usize>,
        /// Write the summary and every game's record as JSON here.
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// *When* in a game does a bot do each thing? One record per side per
    /// turn — that turn's clicks split by what they bought, on a snapshot
    /// of the board they were spent on — reported as a per-turn profile.
    /// ROADMAP Phase 5 §4(c): the evaluator scores a board the same on
    /// turn 1 and turn 20, and a whole-game mean cannot tell a Corp that
    /// builds then scores from one that alternates.
    Tempo {
        /// Games to play; game n plays `matchups[n % len]` on `seed + n`.
        #[arg(long, default_value_t = 96)]
        games: u32,
        /// Base seed.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Which bot takes the Corp chair.
        #[arg(long, default_value = "heuristic")]
        corp: BotSpec,
        /// Which bot takes the Runner chair. Both chairs are reported, and
        /// each is part of the other's measurement rather than a control.
        #[arg(long, default_value = "heuristic")]
        runner: BotSpec,
        /// Search iterations for a searching bot in either chair.
        #[arg(long, default_value_t = 128)]
        simulations: usize,
        /// Hidden-state samples per decision for a searching bot.
        #[arg(long)]
        determinizations: Option<usize>,
        /// Turn ordinals reported individually; later turns pool into one
        /// tail row, so the profile stays a shape rather than a list of
        /// rows with one game in them.
        #[arg(long, default_value_t = 12)]
        turns: u32,
        /// How far a position's stage may move the evaluator's weights —
        /// `eval::Weights::stage_gain`. 0.0 is the static evaluator, and
        /// the profile taken at 0.0 is the baseline any other is read
        /// against.
        #[arg(long, default_value_t = 0.0)]
        stage_gain: f64,
        /// Worker threads. All cores if omitted.
        #[arg(long)]
        threads: Option<usize>,
        /// Write every turn, and the profiles over them, as JSON here.
        #[arg(long)]
        report: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum LearnAction {
    /// List both lesson tracks.
    List,

    /// Play one lesson by id (see `learn list`).
    Lesson {
        id: String,
    },

    /// Play a side's whole track in order, then its starter game.
    Track {
        #[arg(value_enum)]
        side: SideArg,
    },

    /// The unguided starter game: Null Signal's preset decks, 6 agenda
    /// points to win, against the heuristic bot. `--boosted` adds each
    /// side's booster pack and plays to the Standard 7.
    Game {
        #[arg(value_enum)]
        side: SideArg,
        #[arg(long)]
        boosted: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
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

#[derive(Subcommand, Debug, Clone)]
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

    /// Report the card-image cache: how many printings have a scan, and
    /// which are only at 300 pixels because NetrunnerDB has no 750-pixel
    /// scan of them.
    Images {
        /// Only these set codes (repeatable), e.g. `--set sg --set elev`.
        #[arg(long = "set")]
        set: Vec<String>,

        /// Fetch the missing scans first, at the largest size NetrunnerDB
        /// has. A network call; the desktop's image setting is not read.
        #[arg(long)]
        download: bool,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideArg {
    Corp,
    Runner,
}

impl Config {
    /// The personality the bot in `side`'s chair plays with: the flag if
    /// one was given, else the style its deck names, else balanced.
    ///
    /// One resolver rather than a branch at each seat, so the TUI, the
    /// starter game, the headless runner and (by the same rule, in its own
    /// crate) the daemon cannot disagree about which wins. The flag wins
    /// because it is the more specific request — a deck's style is a
    /// default, and `--corp-personality balanced` has to be able to switch
    /// it off for a measurement.
    pub fn personality_for(&self, side: Side, deck: &DeckFile) -> Result<Personality, String> {
        let flag = match side {
            Side::Corp => self.corp_personality,
            Side::Runner => self.runner_personality,
        };
        netrunner_client::play::personality_for(flag, deck)
    }

    /// Whether `side`'s chair is a bot: a bot kind, or a rung — which
    /// overrides the kind, so `--runner-level 3` alone seats a Runner bot
    /// and leaves the Corp to the human, as its doc says. Reading only the
    /// kind here once sent that invocation to the start screen, where the
    /// rung was silently dropped.
    pub fn seats_bot(&self, side: Side) -> bool {
        let kind = match side {
            Side::Corp => self.corp,
            Side::Runner => self.runner,
        };
        kind != BotKind::Human || self.level_for(side).is_some()
    }

    /// The ladder rung asked for on `side`, if any. `Some` overrides the
    /// kind and simulation count for that seat; the personality crosses it.
    pub fn level_for(&self, side: Side) -> Option<Level> {
        match side {
            Side::Corp => self.corp_level,
            Side::Runner => self.runner_level,
        }
    }
}

impl From<SideArg> for Side {
    fn from(side: SideArg) -> Side {
        match side {
            SideArg::Corp => Side::Corp,
            SideArg::Runner => Side::Runner,
        }
    }
}
