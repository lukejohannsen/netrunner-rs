//! `netrunner_cli diag tempo`: *when* in a game does a bot do each thing?
//!
//! ROADMAP Phase 5 §4(c) opens with the observation that
//! `eval::evaluate_state_with` is a static function of the position — it
//! scores a board identically on turn 1 and turn 20 — so the Corp
//! "interleaves building and scoring every turn instead of doing one and
//! then the other". The number quoted there, 13.0 installs and 6.3
//! advancements spread over 11.4 turns, is a whole-game mean, and a
//! whole-game mean is exactly the statistic that cannot tell the two
//! stories apart: a Corp that installs five times and then advances five
//! times has the same mean as one that alternates ten times.
//!
//! **So this reports the shape rather than the total.** One record per
//! side per turn, with that turn's clicks split by what they bought and a
//! snapshot of the board they were spent on, aggregated into a per-turn
//! profile: installs by turn, advancements by turn, runs by turn.
//!
//! **Why it exists before the change it measures.** Every figure in Phase
//! 5 §3 and §4 is a win rate, and a win rate cannot distinguish "plays in
//! phases now" from "plays the same and wins slightly more". A phase
//! change that moves the profile and not the win rate is a different
//! finding from one that moves neither, and only the second says the idea
//! was wrong. This is the instrument that tells them apart, and it has to
//! be taken on an unmodified binary first or there is no before.
//!
//! **A turn, not an action, is the unit.** The clicks in a turn are the
//! scarce resource a stance actually allocates — "bank now, push later" is
//! a claim about which of four clicks went where — so a click action is
//! attributed to the turn it was spent in and the no-click actions that
//! still show tempo (a rez, a score, a steal) are counted beside them
//! rather than in the same total. `HistoryEntry` already carries
//! `turn_number` and `side`, which is why nothing here reconstructs either.
//!
//! **Both chairs name a personality, always.** `--corp`/`--runner` take a
//! `BotSpec`, whose personality defaults to `Balanced` rather than to the
//! deck's own style — so every profile here is chair-isolated by
//! construction, and the trap Phase 5 §3 recorded (an unset
//! `--corp-personality` *is* the deck's style, which made a like-for-like
//! comparison an unlike one) cannot be sprung through this command. A rung
//! is spelled `level:elite` and is the bot the ladder calibrated.
//!
//! Reading `GameState` directly is allowed here for the reason in `diag`'s
//! module doc: a diagnostic is not a client.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::Serialize;

use netrunner_bots::breaker_coverage;
use netrunner_core::cards::CardRegistry;
use netrunner_core::decks as core_decks;
use netrunner_core::rules::{GameState, InstallSlot, PlayerAction, ServerId, Side};
use netrunner_session::{Seat, Session, SessionStep};

use crate::bots;
use crate::config::{BotSpec, Config};
use netrunner_client::decks;

pub struct TempoArgs {
    pub corp: BotSpec,
    pub runner: BotSpec,
    pub games: u32,
    pub seed: u64,
    pub simulations: usize,
    pub determinizations: Option<usize>,
    pub threads: Option<usize>,
    /// Turn ordinals reported individually; everything past it is pooled
    /// into a tail row. A profile is a shape, and twenty rows of one game
    /// each is not one.
    pub turns: u32,
    /// `eval::Weights::stage_gain` for the Runner seat and the Corp seat,
    /// one each, which is what keeps the two chairs' profiles separable.
    pub stage_gain: f64,
    pub corp_stage_gain: f64,
    pub report: Option<PathBuf>,
}

/// What a click bought. Deliberately coarser than `PlayerAction`: the
/// question is which *stance* a turn was spent in, and `InstallHardware`
/// against `InstallProgram` is not a stance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spend {
    Credit,
    Draw,
    Install,
    Advance,
    Play,
    Run,
    /// No click: the rez window is the Corp's, and it is the decision Phase
    /// 5 §3 spent itself on.
    Rez,
    /// No click, and the two ways a game ends on points.
    Score,
    Steal,
    /// A click for the Corp (`TrashResource`), an access decision for the
    /// Runner (`TrashAccessedCard`) — kept as one row because both are
    /// "spend to take something off the other board".
    Trash,
    /// A click that bought none of the above: `RemoveTag`,
    /// `PurgeVirusCounters`, `BreakSubroutineWithClick`.
    OtherClick,
}

/// The tempo-bearing actions. `None` for the flow and prompt actions —
/// `ContinueRun`, `PassPriority`, `ResolvePendingChoice` and the rest —
/// which cost nothing and say nothing about stance.
fn spend(action: &PlayerAction) -> Option<Spend> {
    match action {
        PlayerAction::GainCreditClick { .. } => Some(Spend::Credit),
        PlayerAction::DrawCardClick { .. } => Some(Spend::Draw),
        PlayerAction::InstallCard { .. }
        | PlayerAction::InstallHardware { .. }
        | PlayerAction::InstallProgram { .. }
        | PlayerAction::InstallResource { .. }
        | PlayerAction::InstallProgramOnIce { .. } => Some(Spend::Install),
        PlayerAction::AdvanceCard { .. } => Some(Spend::Advance),
        PlayerAction::PlayOperation { .. } | PlayerAction::PlayEvent { .. } => Some(Spend::Play),
        PlayerAction::InitiateRun { .. } => Some(Spend::Run),
        PlayerAction::RezIce { .. } => Some(Spend::Rez),
        PlayerAction::ScoreAgenda { .. } => Some(Spend::Score),
        PlayerAction::StealAgenda { .. } => Some(Spend::Steal),
        PlayerAction::TrashResource { .. } | PlayerAction::TrashAccessedCard { .. } => Some(Spend::Trash),
        PlayerAction::RemoveTag | PlayerAction::PurgeVirusCounters | PlayerAction::BreakSubroutineWithClick { .. } => {
            Some(Spend::OtherClick)
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Counts {
    pub credit: u32,
    pub draw: u32,
    pub install: u32,
    pub advance: u32,
    pub play: u32,
    pub run: u32,
    pub rez: u32,
    pub score: u32,
    pub steal: u32,
    pub trash: u32,
    pub other_click: u32,
}

impl Counts {
    fn add(&mut self, spend: Spend) {
        let slot = match spend {
            Spend::Credit => &mut self.credit,
            Spend::Draw => &mut self.draw,
            Spend::Install => &mut self.install,
            Spend::Advance => &mut self.advance,
            Spend::Play => &mut self.play,
            Spend::Run => &mut self.run,
            Spend::Rez => &mut self.rez,
            Spend::Score => &mut self.score,
            Spend::Steal => &mut self.steal,
            Spend::Trash => &mut self.trash,
            Spend::OtherClick => &mut self.other_click,
        };
        *slot += 1;
    }
}

/// The board a turn's clicks were spent on, read once at the turn's start.
///
/// Both sides' fields are on one struct, and the irrelevant half is left at
/// zero rather than split into two types: the whole point of the report is
/// to lay the two chairs' profiles side by side, and a column that is
/// always zero on one chair says so plainly.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Board {
    pub credits: u32,
    pub hand: usize,
    /// Corp: installs still face down — what §3 found it could not afford
    /// to rez. Runner: always zero.
    pub face_down: usize,
    pub ice_installed: usize,
    pub ice_rezzed: usize,
    /// Corp: a remote with ICE in front of it and an empty root — a scoring
    /// window, built before there is anything to put in it.
    pub scoring_remote: bool,
    pub rig: usize,
    /// Runner: how many of the three ICE subtypes the rig can break, from
    /// the evaluator's own `breaker_coverage`.
    pub rig_coverage: usize,
    pub own_points: u32,
    pub opponent_points: u32,
}

impl Board {
    fn read(state: &GameState, side: Side, registry: &CardRegistry) -> Board {
        let ice = |predicate: fn(&&netrunner_core::rules::InstalledCard) -> bool| {
            state.corp.installed.iter().filter(|card| card.slot == InstallSlot::Ice).filter(predicate).count()
        };
        Board {
            credits: state.resources(side).credits.0,
            hand: match side {
                Side::Corp => state.corp.hq.len(),
                Side::Runner => state.runner.grip.len(),
            },
            face_down: match side {
                Side::Corp => state.corp.installed.iter().filter(|card| !card.rezzed).count(),
                Side::Runner => 0,
            },
            ice_installed: ice(|_| true),
            ice_rezzed: ice(|card| card.rezzed),
            scoring_remote: has_scoring_remote(state),
            rig: state.runner.rig.len(),
            rig_coverage: match side {
                Side::Runner => breaker_coverage(state, registry),
                Side::Corp => 0,
            },
            own_points: state.resources(side).agenda_points.0,
            opponent_points: state.resources(side.other()).agenda_points.0,
        }
    }
}

/// A remote with at least one piece of ICE and nothing in its root.
///
/// The Corp builds one of these *before* it has an agenda to put there, or
/// it does not build one at all — which is the single clearest board
/// signature of "build now, score later", and the reason it is measured
/// here before it is ever read by an evaluator.
fn has_scoring_remote(state: &GameState) -> bool {
    let remotes: Vec<ServerId> = state
        .corp
        .installed
        .iter()
        .filter_map(|card| matches!(card.server, ServerId::Remote(_)).then_some(card.server))
        .collect();
    remotes.iter().any(|server| {
        let on = |slot: InstallSlot| state.corp.installed.iter().filter(|c| c.server == *server && c.slot == slot).count();
        on(InstallSlot::Ice) > 0 && on(InstallSlot::Root) == 0
    })
}

/// One side's one turn.
#[derive(Debug, Clone, Serialize)]
pub struct Turn {
    pub game: u32,
    pub side: String,
    /// `GameState::turn`, which counts each side's turn separately.
    pub turn: u32,
    /// This side's *n*th turn, 1-based — what the profile is indexed by,
    /// since `turn` alternates chairs.
    pub ordinal: u32,
    pub board: Board,
    pub spent: Counts,
}

/// The per-ordinal mean of every column, over the games that reached it.
#[derive(Debug, Clone, Serialize)]
pub struct Row {
    /// `"3"`, or `"12+"` for the pooled tail.
    pub ordinal: String,
    /// Turns pooled into this row — the denominator of every mean below,
    /// and the reason the tail is pooled at all.
    pub turns: usize,
    pub credit: f64,
    pub draw: f64,
    pub install: f64,
    pub advance: f64,
    pub play: f64,
    pub run: f64,
    pub rez: f64,
    pub credits_at_start: f64,
    pub face_down: f64,
    pub ice_rezzed: f64,
    pub rig: f64,
    pub rig_coverage: f64,
    pub scoring_remote: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SideProfile {
    pub side: String,
    pub bot: String,
    pub turns: usize,
    /// Turns per game — §3's 11.4, and the denominator its whole-game means
    /// were taken over.
    pub turns_per_game: f64,
    /// The whole-game totals §3 quoted, kept so this report can be read
    /// against that entry directly.
    pub per_game: BTreeMap<String, f64>,
    /// The shape those totals hide.
    pub by_turn: Vec<Row>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TempoReport {
    pub corp: String,
    pub runner: String,
    pub games: u32,
    pub seed: u64,
    pub simulations: usize,
    pub determinizations: Option<usize>,
    /// Turns recorded against turns the games actually took
    /// (`GameState::turn` at the end of each). They must agree: a
    /// disagreement means a turn passed without ever being observed and
    /// every profile below is over an unknown subset.
    pub turns_recorded: usize,
    pub turns_played: usize,
    pub profiles: Vec<SideProfile>,
}

pub fn run(args: &TempoArgs, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let registry = decks::sample_deck_registry();
    let matchups = core_decks::matchups();
    let pool = match args.threads {
        Some(threads) => rayon::ThreadPoolBuilder::new().num_threads(threads).build()?,
        None => rayon::ThreadPoolBuilder::new().build()?,
    };

    let corp = describe(args.corp);
    let runner = describe(args.runner);
    println!("playing {} games, {corp} Corp vs {runner} Runner, seed {}...", args.games, args.seed);

    let games: Vec<Game> = pool.install(|| {
        (0..args.games)
            .into_par_iter()
            .map(|game| play(game, args, &registry, &matchups, config))
            .collect::<Result<Vec<_>, String>>()
    })?;
    let played: usize = games.iter().map(|game| game.turns_played).sum();
    let mut turns: Vec<Turn> = games.into_iter().flat_map(|game| game.turns).collect();
    turns.sort_by_key(|turn| (turn.game, turn.turn));

    let report = TempoReport {
        corp: corp.clone(),
        runner: runner.clone(),
        games: args.games,
        seed: args.seed,
        simulations: args.simulations,
        determinizations: args.determinizations,
        turns_recorded: turns.len(),
        turns_played: played,
        profiles: vec![
            profile(Side::Corp, &corp, &turns, args),
            profile(Side::Runner, &runner, &turns, args),
        ],
    };

    print_report(&report);
    if let Some(path) = &args.report {
        fs::write(path, serde_json::to_string_pretty(&report)?)?;
        println!("\nreport written to {}", path.display());
    }
    Ok(())
}

fn profile(side: Side, bot: &str, turns: &[Turn], args: &TempoArgs) -> SideProfile {
    let name = side_name(side);
    let mine: Vec<&Turn> = turns.iter().filter(|turn| turn.side == name).collect();
    let games = f64::from(args.games.max(1));
    let mut per_game: BTreeMap<String, f64> = BTreeMap::new();
    let total = |get: fn(&Counts) -> u32| mine.iter().map(|turn| u64::from(get(&turn.spent))).sum::<u64>() as f64 / games;
    for (label, get) in COLUMNS {
        per_game.insert((*label).to_string(), total(*get));
    }

    // Pooled rather than truncated: the tail is where a "score late" stance
    // would show, and dropping it would hide exactly the half the change is
    // meant to move.
    let mut buckets: BTreeMap<u32, Vec<&Turn>> = BTreeMap::new();
    for turn in &mine {
        buckets.entry(turn.ordinal.min(args.turns + 1)).or_default().push(turn);
    }
    let by_turn = buckets
        .into_iter()
        .map(|(ordinal, turns)| {
            let n = turns.len() as f64;
            let mean = |get: fn(&Turn) -> f64| turns.iter().map(|turn| get(turn)).sum::<f64>() / n;
            Row {
                ordinal: if ordinal > args.turns { format!("{}+", args.turns + 1) } else { ordinal.to_string() },
                turns: turns.len(),
                credit: mean(|t| f64::from(t.spent.credit)),
                draw: mean(|t| f64::from(t.spent.draw)),
                install: mean(|t| f64::from(t.spent.install)),
                advance: mean(|t| f64::from(t.spent.advance)),
                play: mean(|t| f64::from(t.spent.play)),
                run: mean(|t| f64::from(t.spent.run)),
                rez: mean(|t| f64::from(t.spent.rez)),
                credits_at_start: mean(|t| f64::from(t.board.credits)),
                face_down: mean(|t| t.board.face_down as f64),
                ice_rezzed: mean(|t| t.board.ice_rezzed as f64),
                rig: mean(|t| t.board.rig as f64),
                rig_coverage: mean(|t| t.board.rig_coverage as f64),
                scoring_remote: mean(|t| f64::from(u8::from(t.board.scoring_remote))),
            }
        })
        .collect();

    SideProfile {
        side: name.to_string(),
        bot: bot.to_string(),
        turns: mine.len(),
        turns_per_game: mine.len() as f64 / games,
        per_game,
        by_turn,
    }
}

/// One column of `Counts`, by the name it is reported under.
type Column = (&'static str, fn(&Counts) -> u32);

const COLUMNS: &[Column] = &[
    ("advance", |c| c.advance),
    ("credit", |c| c.credit),
    ("draw", |c| c.draw),
    ("install", |c| c.install),
    ("other_click", |c| c.other_click),
    ("play", |c| c.play),
    ("rez", |c| c.rez),
    ("run", |c| c.run),
    ("score", |c| c.score),
    ("steal", |c| c.steal),
    ("trash", |c| c.trash),
];

fn play(
    game: u32,
    args: &TempoArgs,
    registry: &CardRegistry,
    matchups: &[(core_decks::DeckFile, core_decks::DeckFile)],
    config: &Config,
) -> Result<Game, String> {
    let seed = args.seed.wrapping_add(u64::from(game));
    let (corp_deck, runner_deck) = &matchups[game as usize % matchups.len()];
    let (state, _events) =
        GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), registry, seed).map_err(|e| format!("{e:?}"))?;
    let setup = |spec: BotSpec| bots::AgentSetup {
        simulations: args.simulations,
        determinizations: args.determinizations,
        shared_sample: false,
        mcts_depth: None,
        personality: spec.personality,
        runner_stage_gain: args.stage_gain,
        corp_stage_gain: args.corp_stage_gain,
    };
    // `make_seat_agent` rather than `make_agent_with_model`, so a chair
    // spelled `level:elite` is the rung the ladder calibrated and not a
    // silently balanced heuristic. Profiling a rung is what Phase 5 §4(a)
    // and §4(b) will want from this.
    let corp = bots::make_seat_agent(args.corp.level, args.corp.kind, Side::Corp, seed, setup(args.corp), &config.model)?
        .ok_or("the Corp seat must be a bot that can take one")?;
    let runner = bots::make_seat_agent(
        args.runner.level,
        args.runner.kind,
        Side::Runner,
        seed.wrapping_add(1),
        setup(args.runner),
        &config.model,
    )?
    .ok_or("the Runner seat must be a bot that can take one")?;
    let mut session = Session::new(state, registry.clone(), Seat::Agent(corp), Seat::Agent(runner));

    let mut watcher = Watcher::default();
    loop {
        // Before the step, so a turn's board is read as its first action
        // sees it rather than after that action has changed it.
        watcher.open(game, session.state(), registry);
        match session.step() {
            SessionStep::Applied { .. } => {
                if let Some(entry) = session.last_entry() {
                    watcher.record(entry.side, &entry.action);
                }
                continue;
            }
            _ => break,
        }
    }
    let turns_played = session.state().turn as usize;
    Ok(Game { turns: watcher.finish(), turns_played })
}

struct Game {
    turns: Vec<Turn>,
    /// `GameState::turn` at the end — every turn the game actually had,
    /// against the number this diagnostic opened a record for.
    turns_played: usize,
}

/// Turns a sequence of authoritative states and applied entries into one
/// record per turn, on the timeline of the side whose turn it is.
///
/// **A turn belongs to one side, but tempo does not.** The Corp's rezzes
/// happen during the *Runner's* turn, at an ICE approach, and they are the
/// decision Phase 5 §3 spent itself on — so an action is attributed to the
/// acting side's own most recent turn rather than to the turn it fell in.
/// A rez during the Runner's second turn lands on the Corp's second row,
/// which is where a reader looking for "when does this Corp start rezzing"
/// will look for it. Keying by the turn number instead silently dropped
/// every ICE rez in the game, since no Corp-owned record exists for an even
/// turn; the test below is that bug.
#[derive(Default)]
struct Watcher {
    turns: Vec<Turn>,
    /// Index into `turns` of each side's current record, and that side's
    /// turns opened so far for the 1-based ordinal.
    corp: Option<usize>,
    runner: Option<usize>,
    corp_turns: u32,
    runner_turns: u32,
}

impl Watcher {
    /// Opens a record for whichever side's turn `state` is in, if that turn
    /// has none yet. A no-op through the mulligans, where `turn` is 0 and
    /// nobody's turn is underway.
    fn open(&mut self, game: u32, state: &GameState, registry: &CardRegistry) {
        if state.turn == 0 {
            return;
        }
        // Odd turns are the Corp's, even the Runner's — `GameState::turn`
        // is 1 for the Corp's opening turn and alternates from there.
        let side = if state.turn % 2 == 1 { Side::Corp } else { Side::Runner };
        let current = match side {
            Side::Corp => &mut self.corp,
            Side::Runner => &mut self.runner,
        };
        if current.is_some_and(|index| self.turns[index].turn == state.turn) {
            return;
        }
        let ordinal = match side {
            Side::Corp => {
                self.corp_turns += 1;
                self.corp_turns
            }
            Side::Runner => {
                self.runner_turns += 1;
                self.runner_turns
            }
        };
        self.turns.push(Turn {
            game,
            side: side_name(side).to_string(),
            turn: state.turn,
            ordinal,
            board: Board::read(state, side, registry),
            spent: Counts::default(),
        });
        let index = self.turns.len() - 1;
        match side {
            Side::Corp => self.corp = Some(index),
            Side::Runner => self.runner = Some(index),
        }
    }

    fn record(&mut self, side: Side, action: &PlayerAction) {
        let Some(spend) = spend(action) else { return };
        let current = match side {
            Side::Corp => self.corp,
            Side::Runner => self.runner,
        };
        if let Some(index) = current {
            self.turns[index].spent.add(spend);
        }
    }

    fn finish(self) -> Vec<Turn> {
        self.turns
    }
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Corp => "Corp",
        Side::Runner => "Runner",
    }
}

fn describe(spec: BotSpec) -> String {
    match spec.level {
        Some(level) => format!("level:{level:?}:{:?}", spec.personality).to_lowercase(),
        None => format!("{:?}:{:?}", spec.kind, spec.personality).to_lowercase(),
    }
}


fn print_report(report: &TempoReport) {
    println!(
        "\n{} turns recorded over {} games ({} played{})",
        report.turns_recorded,
        report.games,
        report.turns_played,
        if report.turns_recorded == report.turns_played { "; every turn seen" } else { " — MISSED SOME" }
    );
    for profile in &report.profiles {
        println!("\n{} ({}), {:.1} turns a game", profile.side, profile.bot, profile.turns_per_game);
        let per_game: Vec<String> =
            profile.per_game.iter().filter(|(_, v)| **v > 0.0).map(|(k, v)| format!("{k} {v:.1}")).collect();
        println!("  per game: {}", per_game.join(", "));
        println!(
            "  {:>5}  {:>6} | {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} | {:>6} {:>5} {:>5} {:>5} {:>5} {:>5}",
            "turn", "n", "cred", "draw", "inst", "adv", "play", "run", "rez", "creds", "fdown", "rezd", "rig", "cov", "rem"
        );
        for row in &profile.by_turn {
            println!(
                "  {:>5}  {:>6} | {:>5.2} {:>5.2} {:>5.2} {:>5.2} {:>5.2} {:>5.2} {:>5.2} | {:>6.1} {:>5.2} {:>5.2} {:>5.2} {:>5.2} {:>5.2}",
                row.ordinal,
                row.turns,
                row.credit,
                row.draw,
                row.install,
                row.advance,
                row.play,
                row.run,
                row.rez,
                row.credits_at_start,
                row.face_down,
                row.ice_rezzed,
                row.rig,
                row.rig_coverage,
                row.scoring_remote,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::CardId;
    use netrunner_core::rules::{InstallId, InstalledCard, ServerTarget, TargetZone};

    fn installed(server: ServerId, slot: InstallSlot) -> InstalledCard {
        InstalledCard { card: CardId("x".to_string()), install_id: InstallId(0), server, slot, ..Default::default() }
    }

    /// Every click action lands in the row that says what it bought, and
    /// the flow actions land in none. The second half is the load-bearing
    /// one: `ContinueRun` and `PassPriority` outnumber the click actions
    /// several times over in a real game, and counting either as tempo
    /// would swamp every profile in the report.
    #[test]
    fn a_click_is_attributed_to_what_it_bought_and_run_flow_to_nothing() {
        let card = || CardId("x".to_string());
        assert_eq!(spend(&PlayerAction::GainCreditClick { side: Side::Corp }), Some(Spend::Credit));
        assert_eq!(spend(&PlayerAction::DrawCardClick { side: Side::Runner }), Some(Spend::Draw));
        assert_eq!(spend(&PlayerAction::AdvanceCard { target: InstallId(0) }), Some(Spend::Advance));
        assert_eq!(spend(&PlayerAction::InitiateRun { server: ServerTarget::Hq }), Some(Spend::Run));
        assert_eq!(spend(&PlayerAction::RezIce { ice: InstallId(0) }), Some(Spend::Rez));
        assert_eq!(spend(&PlayerAction::ScoreAgenda { target: InstallId(0) }), Some(Spend::Score));
        assert_eq!(spend(&PlayerAction::StealAgenda { card_id: card() }), Some(Spend::Steal));
        // Both chairs' installs are one row: the question is whether the
        // turn went on building, not what was built.
        assert_eq!(spend(&PlayerAction::InstallProgram { card_id: card(), trash_first: false }), Some(Spend::Install));
        assert_eq!(
            spend(&PlayerAction::InstallCard { card_id: card(), zone: TargetZone::Hq, slot: InstallSlot::Ice, trash_first: false }),
            Some(Spend::Install)
        );

        assert_eq!(spend(&PlayerAction::ContinueRun), None);
        assert_eq!(spend(&PlayerAction::CompleteRun), None);
        assert_eq!(spend(&PlayerAction::PassPriority { side: Side::Corp }), None);
        assert_eq!(spend(&PlayerAction::EndTurn), None);
    }

    /// A scoring remote is ICE in front of an empty root — the board
    /// signature of "build now, score later". A central is never one
    /// however much ICE it has, and a remote stops being one the moment
    /// something is installed in it.
    #[test]
    fn a_scoring_remote_is_ice_in_front_of_an_empty_root() {
        let mut state = GameState::new(0);
        assert!(!has_scoring_remote(&state), "an empty board has no remote at all");

        state.corp.installed = vec![installed(ServerId::Hq, InstallSlot::Ice)];
        assert!(!has_scoring_remote(&state), "a defended central is not a scoring remote");

        state.corp.installed.push(installed(ServerId::Remote(0), InstallSlot::Root));
        assert!(!has_scoring_remote(&state), "a naked remote with a card in it is not one either");

        state.corp.installed.push(installed(ServerId::Remote(1), InstallSlot::Ice));
        assert!(has_scoring_remote(&state), "ICE in front of an empty remote is the window");

        state.corp.installed.push(installed(ServerId::Remote(1), InstallSlot::Root));
        assert!(!has_scoring_remote(&state), "filling it closes it");
    }

    /// `GameState::turn` counts each side's turn separately from the
    /// Corp's opening turn, so odd turns are the Corp's. The ordinal is
    /// that side's own count, which is what a profile is indexed by —
    /// turn 5 is the Corp's third.
    #[test]
    fn odd_turns_are_the_corps_and_the_ordinal_counts_that_chairs_own_turns() {
        let registry = CardRegistry::from_cards(vec![]);
        let mut watcher = Watcher::default();
        let mut state = GameState::new(0);

        // Turn 0 is the mulligans: nobody's turn is underway, so no record
        // is opened and the first real turn is still the Corp's first.
        watcher.open(0, &state, &registry);
        assert!(watcher.turns.is_empty(), "the mulligan window is nobody's turn");

        for turn in 1..=5 {
            state.turn = turn;
            watcher.open(0, &state, &registry);
            // Re-observing the same turn must not open a second record or
            // advance the ordinal: `open` is called on every step.
            watcher.open(0, &state, &registry);
        }
        let turns = watcher.finish();
        let seen: Vec<(u32, &str, u32)> =
            turns.iter().map(|turn| (turn.turn, turn.side.as_str(), turn.ordinal)).collect();
        assert_eq!(
            seen,
            vec![(1, "Corp", 1), (2, "Runner", 1), (3, "Corp", 2), (4, "Runner", 2), (5, "Corp", 3)],
            "turn 5 is the Corp's third"
        );
    }

    /// A rez is the Corp's tempo and happens on the *Runner's* turn, during
    /// an approach. It lands on the Corp's own current row rather than on
    /// the row of the turn it fell in — attributing it by turn number
    /// instead dropped every ICE rez in the game, because the Corp owns no
    /// record for an even turn.
    #[test]
    fn a_rez_on_the_runners_turn_is_the_corps_tempo() {
        let registry = CardRegistry::from_cards(vec![]);
        let mut watcher = Watcher::default();
        let mut state = GameState::new(0);
        for turn in 1..=4 {
            state.turn = turn;
            watcher.open(0, &state, &registry);
        }

        watcher.record(Side::Corp, &PlayerAction::RezIce { ice: InstallId(0) });
        watcher.record(Side::Runner, &PlayerAction::InitiateRun { server: ServerTarget::Hq });

        let turns = watcher.finish();
        let row = |side: &str, ordinal: u32| {
            turns.iter().find(|turn| turn.side == side && turn.ordinal == ordinal).expect("the row")
        };
        // Turn 3 is the Corp's second, turn 4 the Runner's second: the rez
        // taken during the Runner's second turn is the Corp's second row.
        assert_eq!(row("Corp", 2).spent.rez, 1, "the rez is the Corp's, on its own timeline");
        assert_eq!(row("Runner", 2).spent.rez, 0, "and not the Runner's, whose turn it was");
        assert_eq!(row("Runner", 2).spent.run, 1, "the run is the Runner's");
        assert_eq!(row("Corp", 1).spent.rez, 0, "and neither lands on an earlier row");
    }
}
