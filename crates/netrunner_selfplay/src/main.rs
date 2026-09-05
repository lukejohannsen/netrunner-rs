//! Runs `PuctAgent` vs. `PuctAgent` self-play games over Null Signal Games'
//! System Gateway sample decks (`fixtures`) and writes one JSONL trajectory
//! file per game into `--output-dir` for the training pipeline to consume.
//!
//! Games rotate through every Corp/Runner pairing by default so a
//! training set covers the whole implemented card pool rather than one
//! matchup's slice of it; `--matchup` pins a single pairing.
//!
//! `--arena-candidate` switches the binary into **arena** mode: no
//! trajectories, just `-n` games of a candidate network against the
//! incumbent (`--arena-incumbent`, or the uniform search without one),
//! each side taking both chairs, summarised as one JSON line. This is the
//! evaluator step AlphaZero's loop has and ours did not: without it every
//! checkpoint was promoted, and one bad network drove six iterations of
//! self-play into "the Runner wins" (ROADMAP Phase 2 §5).

mod fixtures;
mod schema;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Parser;
use rand::rngs::StdRng;
use rand::SeedableRng;
use rayon::prelude::*;

use netrunner_bots::{
    encode_observation, pick_action, ActionStat, CycleGuard, PolicyEvaluator, PuctAgent, PuctConfig,
    MixedPriorEvaluator, SplitEvaluator, UniformPolicyEvaluator, OBS_SIZE,
};
#[cfg(feature = "onnx")]
use netrunner_bots::OnnxPolicyEvaluator;
use netrunner_core::rules::{ActionSpace, GamePhase, GameState, PlayerAction, RulesError, Side};
use netrunner_session::{GameEndReason, Seat, Session, SessionStep, StallReason, SubmitError};

use schema::{sparse, GameTrajectory, SelfPlayStep};
use serde::Serialize;

#[derive(Parser)]
#[command(about = "Runs PuctAgent vs. PuctAgent self-play games and writes training trajectories to disk.")]
struct Cli {
    /// Number of self-play games to run.
    #[arg(short = 'n', long = "num-games")]
    num_games: usize,
    /// PUCT search iterations per decision.
    #[arg(short = 's', long = "simulations")]
    simulations: usize,
    /// Directory to write one `game_NNNNN.jsonl` file per game into.
    /// Not used in arena mode.
    #[arg(short = 'o', long = "output-dir", required_unless_present = "arena_candidate")]
    output_dir: Option<PathBuf>,
    /// Optional ONNX policy/value model to drive search priors/values with
    /// (requires building with `--features onnx`); omit for the
    /// no-network `UniformPolicyEvaluator` baseline.
    #[arg(short = 'm', long = "model-path")]
    model_path: Option<PathBuf>,
    /// Number of recorded decisions (per game) sampled proportionally to
    /// visit counts before switching to greedy (argmax-visits) selection.
    #[arg(long = "temp-plies", default_value_t = 10)]
    temp_plies: usize,
    /// Which sample-deck pairing to play, as `<corp_deck>_vs_<runner_deck>`
    /// (e.g. `discretion_advised_vs_stolen_goods`). Omit to rotate through
    /// every pairing, which is what a general training set wants.
    #[arg(long = "matchup")]
    matchup: Option<String>,
    /// Added to every self-play game's index to form its seed, so that
    /// successive iterations of a training run play different games.
    ///
    /// A game's seed used to be its index alone. Once `determinize`'s pools
    /// were sorted (September 2026) self-play became bit-reproducible, and
    /// with it every iteration whose network was not promoted regenerated
    /// the *identical* corpus: "cumulative" training was then the same 96
    /// games repeated, with copies of one game on both sides of the
    /// per-game validation split. `scripts/run_iteration_loop.py` passes
    /// `(iteration − 1) × games`. Not applied in arena mode, where the
    /// fixed seeds make one iteration's verdict comparable with the next
    /// on the same openings.
    #[arg(long = "seed-offset", default_value_t = 0)]
    seed_offset: u64,
    /// Arena mode: pit this ONNX model against `--arena-incumbent` for
    /// `-n` games (Corp in even-numbered games, Runner in odd) and print
    /// one JSON summary line instead of writing trajectories.
    #[arg(long = "arena-candidate")]
    arena_candidate: Option<PathBuf>,
    /// The model the candidate has to beat. Omit for the no-network
    /// `UniformPolicyEvaluator` — what the first candidate of a training
    /// run is distilled from, and so the first thing it has to beat.
    #[arg(long = "arena-incumbent")]
    arena_incumbent: Option<PathBuf>,
    /// Which halves of the candidate's network to actually use, with the
    /// uniform evaluator supplying the other. `both` is an ordinary arena;
    /// the two ablations answer "is it the priors or the value that loses"
    /// when a whole network scores below the search it was distilled from.
    /// See `netrunner_bots::SplitEvaluator`.
    #[arg(long = "candidate-uses", value_enum, default_value_t = CandidateUses::Both)]
    candidate_uses: CandidateUses,
    /// Mixes the candidate's priors toward uniform over the legal set
    /// before the search sees them: `0.0` leaves the network's prior
    /// alone, `1.0` replaces it with the uniform search's own. A dial for
    /// "is the policy head's loss ranking or calibration", never something
    /// to seat in a real match. See `netrunner_bots::MixedPriorEvaluator`.
    #[arg(long = "candidate-prior-mix", default_value_t = 0.0)]
    candidate_prior_mix: f32,
}

/// Which halves of a candidate network the arena seats. An ablation over
/// `--arena-candidate`, never over the incumbent — the incumbent is the
/// bar, and moving it would make two runs incomparable.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum CandidateUses {
    /// The whole network: its priors and its value.
    Both,
    /// The network's value at the leaves, uniform priors at every node.
    ValueOnly,
    /// The network's priors at every node, `evaluate_state`'s value at the
    /// leaves.
    PriorsOnly,
}

impl CandidateUses {
    fn label(self) -> &'static str {
        match self {
            CandidateUses::Both => "both",
            CandidateUses::ValueOnly => "value-only",
            CandidateUses::PriorsOnly => "priors-only",
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum SelfPlayError {
    #[error(transparent)]
    Rules(#[from] RulesError),
    /// A `Session::submit` that the driver refused. Distinct from `Rules`
    /// because it also covers "the match already ended" and "nobody has a
    /// decision pending" — neither of which is an engine rejection.
    #[error(transparent)]
    Submit(#[from] SubmitError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[cfg(not(feature = "onnx"))]
    #[error("--model-path was given but this binary was built without the `onnx` feature")]
    OnnxFeatureDisabled,
    #[cfg(feature = "onnx")]
    #[error(transparent)]
    Onnx(#[from] netrunner_bots::OnnxPolicyError),
    #[error("no sample-deck matchup named {0:?}; expected one of: {1}")]
    UnknownMatchup(String, String),
    #[error("an arena session with two Agent seats yielded {0:?} instead of ending or stalling")]
    ArenaUnexpectedStep(String),
}

/// One arena game: how it went, and which chair the candidate sat in.
/// The chair is carried because the aggregate hides the number that has
/// historically been the most diagnostic — an earlier trained net was 3–16
/// as Corp and 11–4 as Runner, an asymmetry no single score can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArenaGame {
    candidate_side: Side,
    result: ArenaResult,
}

/// How one arena game went, from the candidate's side of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArenaResult {
    CandidateWin,
    IncumbentWin,
    /// The session stalled at `MAX_STEPS` — nobody won, and it counts as
    /// half a point each in `ArenaSummary::candidate_score`.
    Draw,
}

/// The candidate's record from one chair. Both chairs are reported
/// because a candidate that is strong from one and hopeless from the other
/// averages to "mediocre", which is the least actionable thing a verdict
/// can say.
#[derive(Debug, Serialize, PartialEq)]
struct ChairSummary {
    games: usize,
    wins: usize,
    losses: usize,
    draws: usize,
    score: f64,
}

/// The one line arena mode prints: what `scripts/run_iteration_loop.py`
/// reads to decide a promotion.
#[derive(Debug, Serialize, PartialEq)]
struct ArenaSummary {
    /// `--candidate-prior-mix`, recorded for the same reason
    /// `candidate_uses` is: a result line should say what produced it.
    prior_mix: f32,
    /// Which halves of the candidate's network were seated — `"both"` for
    /// an ordinary arena. Recorded so a run's own output says what it
    /// measured, rather than the reader having to remember the flags.
    candidate_uses: &'static str,
    games: usize,
    candidate_wins: usize,
    incumbent_wins: usize,
    draws: usize,
    /// `(wins + draws / 2) / games` — 1.0 is a clean sweep, 0.5 is parity.
    candidate_score: f64,
    /// The same record split by the chair the candidate sat in. Games are
    /// paired one per chair, so these two counts differ by at most one.
    as_corp: ChairSummary,
    as_runner: ChairSummary,
}

fn summarize(games: &[ArenaGame], candidate_uses: CandidateUses, prior_mix: f32) -> ArenaSummary {
    let chair = |side: Option<Side>| -> ChairSummary {
        let rows: Vec<ArenaResult> =
            games.iter().filter(|g| side.is_none_or(|s| g.candidate_side == s)).map(|g| g.result).collect();
        let count = |wanted: ArenaResult| rows.iter().filter(|r| **r == wanted).count();
        let (wins, losses, draws) =
            (count(ArenaResult::CandidateWin), count(ArenaResult::IncumbentWin), count(ArenaResult::Draw));
        let played = rows.len();
        let score = if played == 0 { 0.0 } else { (wins as f64 + draws as f64 / 2.0) / played as f64 };
        ChairSummary { games: played, wins, losses, draws, score }
    };

    let overall = chair(None);
    ArenaSummary {
        prior_mix,
        candidate_uses: candidate_uses.label(),
        games: overall.games,
        candidate_wins: overall.wins,
        incumbent_wins: overall.losses,
        draws: overall.draws,
        candidate_score: overall.score,
        as_corp: chair(Some(Side::Corp)),
        as_runner: chair(Some(Side::Runner)),
    }
}

/// The candidate takes the Corp chair in even-numbered games and the
/// Runner chair in odd ones. Paired with `arena_matchup_index`, which
/// advances every *two* games, that means each matchup is played once
/// from each chair off the same deal.
///
/// **This used to be a bias worth 0.0755 to the candidate, measured.**
/// `matchup_for` indexed on `game_index` directly, so the matchup advanced
/// on every game while the chair alternated on every game too. `matchups()`
/// builds the cross product as `corp_index * runners + runner_index`, and
/// with an even number of Runner decks (12) a matchup's index has the
/// parity of its *Runner* deck — so the candidate played Corp against the
/// even-indexed Runner decks and Runner as the odd-indexed ones, and never
/// the reverse. A candidate provably identical to the incumbent scored
/// **0.5755** over the full 192-matchup cross product, clearing the 0.55
/// promotion gate on seating alone (ROADMAP Phase 2 §5).
fn candidate_side(game_index: usize) -> Side {
    if game_index.is_multiple_of(2) { Side::Corp } else { Side::Runner }
}

/// Which matchup an arena game plays: one per *pair* of games, so games
/// `2k` and `2k + 1` are the same pairing from opposite chairs.
///
/// The arena's seed follows this too, so the two halves of a pair are the
/// same deal played both ways round rather than two different games. That
/// is what makes the null-candidate property exact rather than approximate:
/// when candidate and incumbent are the same evaluator, the pair is one
/// game scored from both sides, so it contributes exactly one win and one
/// loss and the arena returns exactly 0.5.
///
/// Self-play does not use this — `play_one_game` still indexes matchups on
/// the game index directly, because there is no chair to balance when both
/// seats are the same searcher and a training corpus wants breadth.
fn arena_matchup_index(game_index: usize) -> usize {
    game_index / 2
}

/// One arena game. `candidate`/`incumbent` are model paths, `None` meaning
/// the uniform evaluator; both seats are ordinary `Seat::Agent`s, because
/// here only the chosen action matters, not the visit counts self-play
/// records.
fn play_arena_game(
    game_index: usize,
    cli: &Cli,
    candidate: &Option<PathBuf>,
    incumbent: &Option<PathBuf>,
) -> Result<ArenaGame, SelfPlayError> {
    let registry = fixtures::registry();
    let pair = arena_matchup_index(game_index);
    let matchup = matchup_for(pair, cli)?;
    let (corp_deck, runner_deck) = matchup.decks();
    // The pair index, not the game index: both chairs play the same deal.
    let seed = pair as u64;
    let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

    let candidate_side = candidate_side(game_index);
    let config = PuctConfig { iterations: cli.simulations.max(1), ..PuctConfig::default() };
    let seat = |side: Side, seat_seed: u64| -> Result<Seat, SelfPlayError> {
        let is_candidate = side == candidate_side;
        let model = if is_candidate { candidate } else { incumbent };
        let evaluator = make_evaluator(side, model)?;
        // Only the candidate is ever ablated: the incumbent is the bar,
        // and moving it would make two ablation runs incomparable with
        // each other and with the ordinary arena.
        let evaluator = if is_candidate { ablate(evaluator, side, cli.candidate_uses) } else { evaluator };
        // After the ablation, so `--candidate-prior-mix` dials whichever
        // priors the ablation left in place rather than a discarded set.
        let evaluator: Box<dyn PolicyEvaluator> = if is_candidate && cli.candidate_prior_mix > 0.0 {
            Box::new(MixedPriorEvaluator::new(evaluator, cli.candidate_prior_mix))
        } else {
            evaluator
        };
        Ok(Seat::Agent(Box::new(PuctAgent::with_config(side, seat_seed, evaluator, config))))
    };
    let corp = seat(Side::Corp, seed)?;
    let runner = seat(Side::Runner, seed.wrapping_add(1))?;
    let mut session = Session::new(state, registry, corp, runner).without_history();
    let result = match session.run() {
        SessionStep::Ended { winner, .. } => {
            if winner == candidate_side { ArenaResult::CandidateWin } else { ArenaResult::IncumbentWin }
        }
        SessionStep::Stalled(_) => ArenaResult::Draw,
        other => return Err(SelfPlayError::ArenaUnexpectedStep(format!("{other:?}"))),
    };
    Ok(ArenaGame { candidate_side, result })
}

/// Wraps `evaluator` so only the requested halves of it survive, the
/// uniform evaluator supplying the rest. `Both` returns it untouched, so
/// an ordinary arena pays nothing for this existing.
fn ablate(evaluator: Box<dyn PolicyEvaluator>, side: Side, uses: CandidateUses) -> Box<dyn PolicyEvaluator> {
    match uses {
        CandidateUses::Both => evaluator,
        CandidateUses::ValueOnly => {
            Box::new(SplitEvaluator::new(Box::new(UniformPolicyEvaluator::new(side)), evaluator))
        }
        CandidateUses::PriorsOnly => {
            Box::new(SplitEvaluator::new(evaluator, Box::new(UniformPolicyEvaluator::new(side))))
        }
    }
}

fn make_evaluator(side: Side, model_path: &Option<PathBuf>) -> Result<Box<dyn PolicyEvaluator>, SelfPlayError> {
    match model_path {
        Some(path) => {
            #[cfg(feature = "onnx")]
            {
                let path = path.to_str().expect("--model-path must be valid UTF-8");
                Ok(Box::new(OnnxPolicyEvaluator::new(path, side)?))
            }
            #[cfg(not(feature = "onnx"))]
            {
                let _ = path;
                Err(SelfPlayError::OnnxFeatureDisabled)
            }
        }
        None => Ok(Box::new(UniformPolicyEvaluator::new(side))),
    }
}

/// Root visit counts normalized into a probability distribution over
/// `ActionSpace::SIZE`. All-zero (rather than dividing by zero) if nothing
/// was ever visited — shouldn't happen for a search that ran at least one
/// iteration, but keeps this total rather than panicking.
fn normalized_policy_target(visit_counts: &[u32]) -> Vec<f32> {
    let total: u32 = visit_counts.iter().sum();
    if total == 0 {
        return vec![0.0; visit_counts.len()];
    }
    visit_counts.iter().map(|&visits| visits as f32 / total as f32).collect()
}

/// Picks which searched action to actually play: visit-weighted while
/// `ply < temp_plies` (the "temperature" phase), greedy afterward, and —
/// once the same action has been chosen `MAX_GREEDY_REPEATS` times in a
/// row — anything *but* that action. The mechanism is `netrunner_bots::
/// pick_action`, shared with `PuctAgent::select_action` so the arena and
/// self-play cannot disagree about what a stall is; this wrapper only adds
/// the temperature schedule, which is a training concern.
///
/// `None` only if `actions` is empty, which `PuctAgent::search` no longer
/// produces. The caller plays a legal action and records nothing for that
/// ply instead of ending the run.
fn choose_action<'a>(
    actions: &'a [ActionStat],
    ply: usize,
    temp_plies: usize,
    avoid: &[PlayerAction],
    rng: &mut StdRng,
) -> Option<&'a ActionStat> {
    pick_action(actions, ply < temp_plies, avoid, rng)
}

/// Re-keys a search's visit counts into `state`'s `ActionSpace` encoding.
///
/// `PuctSearchStats::visit_counts` is indexed in the *determinized
/// sample's* space (see `ActionStat::index`), but a recorded policy target
/// is consumed alongside an observation encoded from the real state, and
/// `netrunner_gym` decodes indices against the real state too. Those spaces
/// do not generally agree — `determinize` resamples hidden zones and
/// rebuilds `corp.installed` in view order rather than install order — so
/// reusing the search's own vector would label the target with slots that
/// mean something else to every consumer.
///
/// Done here rather than inside `PuctAgent` deliberately: an agent sees
/// only its `ClientView`, never the real `GameState`, and that boundary is
/// what makes the bots honest. The harness legitimately holds both.
///
/// An action that doesn't encode against `state` contributes nothing; it
/// cannot be named in a target whose vocabulary is `state`'s.
fn policy_target_for(state: &GameState, actions: &[ActionStat]) -> Vec<f32> {
    let mut visit_counts = vec![0u32; ActionSpace::SIZE];
    for stat in actions {
        if let Some(index) = ActionSpace::index_of(state, &stat.action) {
            visit_counts[index] = stat.visits;
        }
    }
    normalized_policy_target(&visit_counts)
}

/// The matchup this game plays: the one `--matchup` names, or the
/// `game_index`th of the sample pairings so a run spreads evenly over all of
/// them regardless of `--num-games`.
fn matchup_for(game_index: usize, cli: &Cli) -> Result<fixtures::Matchup, SelfPlayError> {
    let all = fixtures::matchups();
    match &cli.matchup {
        Some(id) => fixtures::matchup_by_id(id).ok_or_else(|| {
            let known = all.iter().map(fixtures::Matchup::id).collect::<Vec<_>>().join(", ");
            SelfPlayError::UnknownMatchup(id.clone(), known)
        }),
        None => Ok(all[game_index % all.len()].clone()),
    }
}

fn play_one_game(game_index: usize, cli: &Cli) -> Result<GameTrajectory, SelfPlayError> {
    let registry = fixtures::registry();
    let matchup = matchup_for(game_index, cli)?;
    let (corp_deck, runner_deck) = matchup.decks();
    let seed = cli.seed_offset.wrapping_add(game_index as u64);

    let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;
    // Both seats are `External`: a self-play seat needs `PuctAgent::search`
    // for its visit counts, not `select_action`'s single chosen action, so
    // the session cannot resolve either side itself. `without_history`
    // because the trajectory below is the record that matters here, and
    // this runs over millions of games.
    let mut session =
        Session::new(state, registry.clone(), Seat::External, Seat::External).without_history();

    let config = PuctConfig { iterations: cli.simulations.max(1), ..PuctConfig::default() };
    let mut corp_agent = PuctAgent::with_config(Side::Corp, seed, make_evaluator(Side::Corp, &cli.model_path)?, config);
    let mut runner_agent =
        PuctAgent::with_config(Side::Runner, seed.wrapping_add(1), make_evaluator(Side::Runner, &cli.model_path)?, config);
    let mut rng = StdRng::seed_from_u64(seed.wrapping_add(2));

    let mut steps: Vec<SelfPlayStep> = Vec::new();
    let mut ply = 0usize;
    // One cycle guard per side, the same `CycleGuard` `PuctAgent` and
    // `MctsAgent` use — self-play seats are `Seat::External`, so the
    // agents' own guards never run here and these are the only ones in
    // play. Per side because a chooser's repetition is what says *that*
    // chooser is stuck.
    let mut corp_cycle = CycleGuard::default();
    let mut runner_cycle = CycleGuard::default();

    // The session's own `MAX_STEPS` bounds this now; the local copy this
    // loop used to carry is gone. The terminal step is kept rather than
    // dropped: `end_reason` is what lets the trainer tell a game that
    // ended from one that ran out of budget, and at iteration 8 of the
    // second volume run those were 24% of every recorded decision (see
    // `GameTrajectory::end_reason`).
    let ending = loop {
        let step = session.step();
        let SessionStep::Awaiting { side, view } = step else {
            break step;
        };
        if view.legal_actions.len() == 1 {
            // No real decision was made, so there's nothing meaningful to
            // record — mirrors `PuctAgent::select_action`'s own
            // single-action short-circuit.
            session.submit(view.legal_actions[0].clone())?;
            continue;
        }

        let observation = encode_observation(session.state(), &registry, side);
        let agent = match side {
            Side::Corp => &mut corp_agent,
            Side::Runner => &mut runner_agent,
        };
        let stats = agent.search(&view, &registry);
        // Both the target and `action_taken` are keyed in the real state's
        // `ActionSpace`, matching `observation` above and what
        // `netrunner_gym` decodes against — not the sample the search ran
        // on. See `policy_target_for`.
        let policy_target = policy_target_for(session.state(), &stats.actions);

        let cycle = match side {
            Side::Corp => &mut corp_cycle,
            Side::Runner => &mut runner_cycle,
        };
        let avoid = cycle.cycling();
        let Some(chosen) = choose_action(&stats.actions, ply, cli.temp_plies, &avoid, &mut rng) else {
            // Unreachable by `search`'s contract. Play on rather than end
            // the run; the ply simply contributes no training example.
            session.submit(view.legal_actions[0].clone())?;
            continue;
        };
        let chosen_index = ActionSpace::index_of(session.state(), &chosen.action);
        cycle.record(&chosen.action, !avoid.is_empty());

        // A step whose action has no slot in the real state's encoding
        // can't be labelled, so it is played but not recorded.
        if let Some(action_taken) = chosen_index {
            steps.push(SelfPlayStep {
                observation: sparse(&observation),
                policy_target: sparse(&policy_target),
                search_value: stats.root_value,
                action_taken,
                active_side: side as u8,
            });
        }

        session.submit(chosen.action.clone())?;
        ply += 1;
    };

    let outcome_corp = match session.state().phase {
        GamePhase::GameOver(Side::Corp) => 1.0,
        GamePhase::GameOver(Side::Runner) => -1.0,
        _ => 0.0,
    };

    Ok(GameTrajectory {
        observation_size: OBS_SIZE,
        action_space_size: ActionSpace::SIZE,
        seed,
        steps,
        outcome_corp,
        matchup: matchup.id(),
        pool_fingerprint: netrunner_core::pool_fingerprint(),
        end_reason: end_reason_of(&ending),
    })
}

/// The recorded name of a terminal `SessionStep`.
///
/// Snake case rather than `{:?}`, and every stall behind a `stall_` prefix
/// that the trainer keys on: a stalled game is not a draw, it is `MAX_STEPS`
/// cycling decisions with a zero value target, and the trainer drops them.
/// A step that is neither `Ended` nor `Stalled` cannot occur here — both
/// seats are `External`, so `step` returns `Awaiting` until it does not —
/// but it is named rather than panicked on, since losing a game's label is
/// not worth aborting a 2,400-game batch over.
fn end_reason_of(step: &SessionStep) -> String {
    match step {
        SessionStep::Ended { reason, .. } => match reason {
            GameEndReason::AgendaThreshold => "agenda_threshold",
            GameEndReason::Flatline => "flatline",
            GameEndReason::Deckout => "deckout",
            GameEndReason::Surrender => "surrender",
            GameEndReason::Disconnected => "disconnected",
            GameEndReason::TimedOut => "timed_out",
        },
        SessionStep::Stalled(StallReason::BudgetExhausted) => "stall_budget_exhausted",
        SessionStep::Stalled(StallReason::NoLegalActions { .. }) => "stall_no_legal_actions",
        SessionStep::Stalled(StallReason::NoCurrentActor) => "stall_no_current_actor",
        SessionStep::Applied { .. } | SessionStep::Awaiting { .. } => "unterminated",
    }
    .to_string()
}

fn write_trajectory(output_dir: &Path, game_index: usize, trajectory: &GameTrajectory) -> Result<(), SelfPlayError> {
    let path = output_dir.join(format!("game_{game_index:05}.jsonl"));
    let mut file = fs::File::create(path)?;
    serde_json::to_writer(&mut file, trajectory)?;
    writeln!(file)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.arena_candidate.is_some() {
        let results = (0..cli.num_games)
            .into_par_iter()
            .map(|game_index| play_arena_game(game_index, &cli, &cli.arena_candidate, &cli.arena_incumbent))
            .collect::<Result<Vec<_>, _>>()?;
        println!("{}", serde_json::to_string(&summarize(&results, cli.candidate_uses, cli.candidate_prior_mix))?);
        return Ok(());
    }

    let output_dir = cli.output_dir.as_ref().expect("clap requires --output-dir outside arena mode");
    fs::create_dir_all(output_dir)?;

    (0..cli.num_games).into_par_iter().try_for_each(|game_index| -> Result<(), SelfPlayError> {
        let trajectory = play_one_game(game_index, &cli)?;
        write_trajectory(output_dir, game_index, &trajectory)
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::ActionStat;
    use netrunner_core::cards::CardRegistry;
    use netrunner_core::dsl::{CardDefinition, CardId, CardType};
    use netrunner_core::rules::{InstallId, InstallSlot, InstalledCard, PlayerAction, ServerId};

    fn corp_card(id: &str) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Asset,
            is_playable: true,
            ..Default::default()
        }
    }

    fn stat(action: PlayerAction, index: Option<usize>, visits: u32) -> ActionStat {
        ActionStat { index, action, visits, total_value: 0.0 }
    }

    /// The policy target must be keyed in the *real* state's `ActionSpace`,
    /// not the sample's. `ActionStat::index` is the sample's slot and the
    /// two spaces disagree in general — `determinize` resamples hidden
    /// zones and rebuilds `corp.installed` in view order — so reusing it
    /// would label a training example with a slot meaning something else
    /// to `netrunner_gym` and to the observation beside it.
    #[test]
    fn the_policy_target_is_keyed_in_the_real_states_action_space() {
        let mut registry = CardRegistry::new();
        registry.insert(corp_card("asset_a"));
        registry.insert(corp_card("asset_b"));

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.installed = vec![
            InstalledCard {
                card: CardId("asset_a".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                slot: InstallSlot::Root,
                ..Default::default()
            },
            InstalledCard {
                card: CardId("asset_b".to_string()),
                install_id: InstallId(2),
                server: ServerId::Remote(1),
                slot: InstallSlot::Root,
                ..Default::default()
            },
        ];

        let action = PlayerAction::RezIce { ice: InstallId(2) };
        let real_index = ActionSpace::index_of(&state, &action).expect("encodes against the real state");
        // A deliberately wrong slot, standing in for the sample's own
        // encoding of the same action.
        let sample_index = real_index + 1;

        let target = policy_target_for(&state, &[stat(action, Some(sample_index), 7)]);
        assert_eq!(target[real_index], 1.0, "the real state's slot carries the target");
        assert_eq!(target[sample_index], 0.0, "the sample's slot must not");
    }

    /// An action with no slot in the real state contributes nothing rather
    /// than mislabelling one, and leaves a valid (if empty) distribution.
    #[test]
    fn an_action_that_does_not_encode_contributes_nothing() {
        let registry = CardRegistry::new();
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        let _ = &registry;

        // An id nothing on the table carries.
        let action = PlayerAction::RezIce { ice: InstallId(99) };
        assert!(ActionSpace::index_of(&state, &action).is_none());

        let target = policy_target_for(&state, &[stat(action, Some(3), 9)]);
        assert_eq!(target.len(), ActionSpace::SIZE);
        assert!(target.iter().all(|p| *p == 0.0));
    }

    #[test]
    fn choose_action_reports_no_choice_rather_than_panicking_on_an_empty_list() {
        let mut rng = StdRng::seed_from_u64(0);
        assert!(choose_action(&[], 0, 8, &[], &mut rng).is_none());
        assert!(choose_action(&[], 99, 8, &[PlayerAction::EndTurn], &mut rng).is_none());
    }

    #[test]
    fn candidate_score_counts_a_stall_as_half() {
        let results = [ArenaResult::CandidateWin, ArenaResult::Draw, ArenaResult::IncumbentWin, ArenaResult::Draw];
        let games: Vec<ArenaGame> = results
            .iter()
            .enumerate()
            .map(|(index, &result)| ArenaGame { candidate_side: candidate_side(index), result })
            .collect();
        let summary = summarize(&games, CandidateUses::Both, 0.0);
        assert_eq!(
            summary,
            ArenaSummary {
                prior_mix: 0.0,
                candidate_uses: "both",
                games: 4,
                candidate_wins: 1,
                incumbent_wins: 1,
                draws: 2,
                candidate_score: 0.5,
                // Indices 0 and 2 are the Corp chair (a win and a loss),
                // 1 and 3 the Runner chair (two stalls).
                as_corp: ChairSummary { games: 2, wins: 1, losses: 1, draws: 0, score: 0.5 },
                as_runner: ChairSummary { games: 2, wins: 0, losses: 0, draws: 2, score: 0.5 },
            }
        );
        assert_eq!(summarize(&[], CandidateUses::Both, 0.0).candidate_score, 0.0, "no games is not a pass");
    }

    /// An arena line has to say what it measured. Three runs of the same
    /// candidate against the same incumbent differ only in this field, and
    /// a reader comparing them months later will not have the flags.
    #[test]
    fn an_arena_summary_names_the_ablation_it_ran() {
        let labels: Vec<&str> = [CandidateUses::Both, CandidateUses::ValueOnly, CandidateUses::PriorsOnly]
            .into_iter()
            .map(|uses| summarize(&[], uses, 0.0).candidate_uses)
            .collect();
        assert_eq!(labels, ["both", "value-only", "priors-only"], "every variant is labelled, and distinctly");

        let one = [ArenaGame { candidate_side: Side::Corp, result: ArenaResult::CandidateWin }];
        let json = serde_json::to_string(&summarize(&one, CandidateUses::PriorsOnly, 0.5)).unwrap();
        assert!(json.contains("\"prior_mix\":0.5"), "a summary must say what dial produced it: {json}");
        assert!(json.contains(r#""candidate_uses":"priors-only""#), "the label reaches the printed line: {json}");
    }

    /// `Both` must not wrap: an ordinary arena has to be the same search it
    /// was before this flag existed, or every prior verdict becomes
    /// incomparable with every later one.
    #[test]
    fn the_default_ablation_leaves_the_evaluator_untouched() {
        let registry = fixtures::registry();
        let (corp_deck, runner_deck) = fixtures::matchups()[0].decks();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 1).unwrap();

        let plain = UniformPolicyEvaluator::new(Side::Corp).evaluate(&state, &registry);
        let through_both = ablate(Box::new(UniformPolicyEvaluator::new(Side::Corp)), Side::Corp, CandidateUses::Both)
            .evaluate(&state, &registry);
        assert_eq!(plain, through_both);
    }

    /// The instrument itself: priors come from one source, value from the
    /// other, and neither leaks into the other's half. Built from two
    /// uniform evaluators on *different* sides, because `evaluate_state` is
    /// zero-sum enough that the Corp's and Runner's values differ while
    /// their legal-action priors do not.
    #[test]
    fn a_split_evaluator_takes_each_half_from_the_source_it_was_given() {
        let registry = fixtures::registry();
        let (corp_deck, runner_deck) = fixtures::matchups()[0].decks();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 1).unwrap();

        let (corp_priors, corp_value) = UniformPolicyEvaluator::new(Side::Corp).evaluate(&state, &registry);
        let (_runner_priors, runner_value) = UniformPolicyEvaluator::new(Side::Runner).evaluate(&state, &registry);
        assert_ne!(corp_value, runner_value, "the fixture must distinguish the two sources for this to prove anything");

        let split = SplitEvaluator::new(
            Box::new(UniformPolicyEvaluator::new(Side::Corp)),
            Box::new(UniformPolicyEvaluator::new(Side::Runner)),
        );
        let (priors, value) = split.evaluate(&state, &registry);
        assert_eq!(priors, corp_priors, "priors come from the first source");
        assert_eq!(value, runner_value, "value comes from the second");
    }

    #[test]
    fn the_candidate_alternates_chairs_across_games() {
        assert_eq!(candidate_side(0), Side::Corp);
        assert_eq!(candidate_side(1), Side::Runner);
    }

    /// The property the old indexing silently violated. Chair must not be
    /// a function of the matchup, or the candidate is handed a fixed
    /// seating for every pairing and the arena measures the chair.
    #[test]
    fn every_matchup_is_played_from_both_chairs() {
        let cli = Cli::parse_from(["netrunner_selfplay", "-n", "2", "-s", "2", "--arena-candidate", "unused.onnx"]);
        let pairings = fixtures::matchups().len();

        for pair in 0..pairings {
            let (first, second) = (pair * 2, pair * 2 + 1);
            assert_eq!(arena_matchup_index(first), pair);
            assert_eq!(arena_matchup_index(second), pair);
            assert_eq!(
                matchup_for(arena_matchup_index(first), &cli).unwrap().id(),
                matchup_for(arena_matchup_index(second), &cli).unwrap().id(),
                "a pair of games must be the same matchup"
            );
            assert_ne!(candidate_side(first), candidate_side(second), "and opposite chairs");
        }
    }

    /// The regression test for a bias that was worth 0.0755 and cleared
    /// the 0.55 promotion gate on seating alone. Two `None` model paths
    /// make candidate and incumbent the same evaluator, so each pair is
    /// one deal scored from both sides: exactly one win and one loss,
    /// whatever happens in it. Anything but 0.5 is the chair leaking back
    /// into the verdict.
    #[test]
    fn a_candidate_identical_to_the_incumbent_scores_exactly_one_half() {
        let cli = Cli::parse_from(["netrunner_selfplay", "-n", "8", "-s", "2", "--arena-candidate", "unused.onnx"]);
        let games: Vec<ArenaGame> =
            (0..8).map(|index| play_arena_game(index, &cli, &None, &None).expect("arena game")).collect();

        let summary = summarize(&games, CandidateUses::Both, 0.0);
        assert_eq!(summary.candidate_score, 0.5, "a null candidate must score exactly parity: {summary:?}");
        assert_eq!(summary.as_corp.games, 4);
        assert_eq!(summary.as_runner.games, 4);
        assert_eq!(
            summary.as_corp.wins + summary.as_runner.wins,
            summary.as_corp.losses + summary.as_runner.losses,
            "each pair contributes one win and one loss"
        );
    }

    /// Two runs at the same offset are the same game, and a different
    /// offset is a different game on the same matchup — the property the
    /// training loop relies on to give every iteration fresh data without
    /// disturbing the matchup rotation, which is keyed on the index alone.
    #[test]
    fn the_seed_offset_changes_the_game_but_not_the_matchup() {
        let parse = |offset: &str| {
            Cli::parse_from(["netrunner_selfplay", "-n", "1", "-s", "2", "-o", "unused", "--seed-offset", offset])
        };
        let first = play_one_game(3, &parse("0")).expect("self-play game");
        let again = play_one_game(3, &parse("0")).expect("self-play game");
        let shifted = play_one_game(3, &parse("1000")).expect("self-play game");
        assert_eq!(first.seed, 3);
        assert_eq!(shifted.seed, 1003);
        assert_eq!(serde_json::to_string(&first).unwrap(), serde_json::to_string(&again).unwrap());
        assert_eq!(first.matchup, shifted.matchup);
        assert_ne!(
            first.steps.first().map(|s| &s.observation),
            shifted.steps.first().map(|s| &s.observation),
            "a different seed deals a different opening"
        );
        assert_eq!((first.observation_size, first.action_space_size), (OBS_SIZE, ActionSpace::SIZE));
        assert!(first.steps.iter().all(|s| s.observation.len() < OBS_SIZE / 4), "the observation is written sparse");
    }

    /// The two header fields the trainer gates on: which engine recorded
    /// the game, and whether the game actually ended.
    ///
    /// A real game is played rather than a fixture asserted, because the
    /// failure being guarded against is a *recorded* corpus that does not
    /// carry them — the second volume run trained across three different
    /// card pools without either field existing to notice
    /// (ROADMAP Phase 2 §5).
    #[test]
    fn every_recorded_game_names_its_engine_and_how_it_ended() {
        let cli = Cli::parse_from(["netrunner_selfplay", "-n", "1", "-s", "2", "-o", "unused"]);
        let game = play_one_game(3, &cli).expect("self-play game");

        assert_eq!(game.pool_fingerprint, netrunner_core::pool_fingerprint());
        assert!(!game.pool_fingerprint.is_empty(), "an empty fingerprint is how an archived corpus reads");
        assert!(
            !game.end_reason.is_empty() && game.end_reason != "unterminated",
            "a played game names its ending, got {:?}",
            game.end_reason
        );
        assert_eq!(game.end_reason.starts_with("stall_"), game.outcome_corp == 0.0, "only a stall is undecided");
    }

    /// The prefix the trainer keys on to drop a game: every stall carries
    /// it, no ending does. Asserted over the variants rather than by
    /// stalling a real game, which costs `MAX_STEPS` searches.
    #[test]
    fn only_a_stall_is_named_with_the_prefix_the_trainer_drops() {
        let ended = SessionStep::Ended { winner: Side::Corp, reason: GameEndReason::Flatline };
        assert_eq!(end_reason_of(&ended), "flatline");
        for stall in [
            StallReason::BudgetExhausted,
            StallReason::NoLegalActions { side: Side::Runner },
            StallReason::NoCurrentActor,
        ] {
            let named = end_reason_of(&SessionStep::Stalled(stall));
            assert!(named.starts_with("stall_"), "{named} must be droppable by prefix");
        }
        assert_eq!(end_reason_of(&SessionStep::Stalled(StallReason::BudgetExhausted)), "stall_budget_exhausted");
    }

    /// The arena path end to end with no network on either side: two real
    /// games at a token search budget, summarised. Guards the seat wiring
    /// and the JSON shape the training loop parses.
    #[test]
    fn a_uniform_arena_plays_its_games_and_accounts_for_every_one() {
        let cli = Cli::parse_from(["netrunner_selfplay", "-n", "2", "-s", "2", "--arena-candidate", "unused.onnx"]);
        let results: Vec<ArenaGame> =
            (0..2).map(|i| play_arena_game(i, &cli, &None, &None).expect("uniform arena game")).collect();
        let summary = summarize(&results, CandidateUses::Both, 0.0);
        assert_eq!(summary.games, 2);
        assert_eq!(summary.candidate_wins + summary.incumbent_wins + summary.draws, 2);
        let line = serde_json::to_string(&summary).unwrap();
        assert!(line.contains("\"candidate_score\""), "{line}");
        assert!(line.contains("\"as_corp\"") && line.contains("\"as_runner\""), "the chair split reaches the line: {line}");
    }
}
