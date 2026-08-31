//! Runs `PuctAgent` vs. `PuctAgent` self-play games over Null Signal Games'
//! System Gateway sample decks (`fixtures`) and writes one JSONL trajectory
//! file per game into `--output-dir` for the training pipeline to consume.
//!
//! Games rotate through all twelve Corp/Runner pairings by default so a
//! training set covers the whole implemented card pool rather than one
//! matchup's slice of it; `--matchup` pins a single pairing.

mod fixtures;
mod schema;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Parser;
use rand::distr::weighted::WeightedIndex;
use rand::distr::Distribution;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;

use netrunner_bots::{encode_observation, ActionStat, PolicyEvaluator, PuctAgent, PuctConfig, UniformPolicyEvaluator};
#[cfg(feature = "onnx")]
use netrunner_bots::OnnxPolicyEvaluator;
use netrunner_core::rules::{current_actor, GamePhase, GameState, RulesError, Side};
use netrunner_core::view::build_client_view;

use schema::{GameTrajectory, SelfPlayStep};

/// Guard against a stalled/looping game running forever — same budget as
/// `netrunner_server::MatchSession::MAX_STEPS`.
const MAX_STEPS: u32 = 10_000;

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
    #[arg(short = 'o', long = "output-dir")]
    output_dir: PathBuf,
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
    /// all twelve pairings, which is what a general training set wants.
    #[arg(long = "matchup")]
    matchup: Option<String>,
}

#[derive(Debug, thiserror::Error)]
enum SelfPlayError {
    #[error(transparent)]
    Rules(#[from] RulesError),
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

/// How many times the same action index may be chosen back-to-back before
/// selection stops being greedy.
///
/// Legitimate repeats exist — clicking for credits three or four times in a
/// row is ordinary play — but each of those consumes a click and moves the
/// game on. A run far past a turn's worth of clicks means the state is
/// cycling instead, so the bound sits comfortably above normal repetition.
const MAX_GREEDY_REPEATS: usize = 8;

/// Picks which searched action to actually play: weighted-sampled by
/// visit count while `ply < temp_plies` (the "temperature" phase), then
/// greedy (max visits) afterward — mirrors `PuctAgent::select_action`'s own
/// greedy tie-break once temperature drops out.
///
/// `repeats` breaks deterministic cycles. Greedy selection is a pure
/// function of the visit counts, so a decision that leaves the state
/// essentially unchanged gets re-picked forever: toggling one card of a
/// card-selection on and off is the observed case — a perfect two-cycle
/// that burned a whole game's step budget even though
/// `ConfirmCardSelection` was legal and sitting right there. Weak search
/// makes it likelier (few simulations against a uniform evaluator leave
/// visit counts nearly flat), but nothing about greedy selection rules it
/// out at any strength, so escaping is handled here rather than assumed
/// away. Falling back to sampling keeps the escape probabilistic instead of
/// forcing a specific action, so it perturbs the trajectory as little as
/// possible.
fn choose_action<'a>(
    actions: &'a [ActionStat],
    ply: usize,
    temp_plies: usize,
    repeats: usize,
    rng: &mut StdRng,
) -> &'a ActionStat {
    if ply < temp_plies || repeats >= MAX_GREEDY_REPEATS {
        let weights: Vec<u32> = actions.iter().map(|stat| stat.visits).collect();
        if let Ok(distribution) = WeightedIndex::new(&weights) {
            return &actions[distribution.sample(rng)];
        }
        // Every action has zero visits, so visit-weighted sampling has
        // nothing to work with. Uniform choice still breaks the cycle.
        if repeats >= MAX_GREEDY_REPEATS {
            return &actions[rng.random_range(0..actions.len())];
        }
    }
    actions.iter().max_by_key(|stat| stat.visits).expect("PuctAgent::search always returns at least one action")
}

/// The matchup this game plays: the one `--matchup` names, or the
/// `game_index`th of the twelve pairings so a run spreads evenly over all of
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
    let seed = game_index as u64;

    let (mut state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

    let config = PuctConfig { iterations: cli.simulations.max(1), ..PuctConfig::default() };
    let mut corp_agent = PuctAgent::with_config(Side::Corp, seed, make_evaluator(Side::Corp, &cli.model_path)?, config);
    let mut runner_agent =
        PuctAgent::with_config(Side::Runner, seed.wrapping_add(1), make_evaluator(Side::Runner, &cli.model_path)?, config);
    let mut rng = StdRng::seed_from_u64(seed.wrapping_add(2));

    let mut steps: Vec<SelfPlayStep> = Vec::new();
    let mut ply = 0usize;
    // Consecutive identical chosen indices, for `choose_action`'s
    // cycle-breaking fallback.
    let mut last_index: Option<usize> = None;
    let mut repeats = 0usize;

    for _ in 0..MAX_STEPS {
        if matches!(state.phase, GamePhase::GameOver(_)) {
            break;
        }
        let Some(side) = current_actor(&state) else { break };
        let view = build_client_view(&state, &registry, side);

        if view.legal_actions.len() == 1 {
            // No real decision was made, so there's nothing meaningful to
            // record — mirrors `PuctAgent::select_action`'s own
            // single-action short-circuit.
            let (next, _events) = state.step(&registry, view.legal_actions[0].clone())?;
            state = next;
            continue;
        }

        let observation = encode_observation(&state, &registry, side);
        let agent = match side {
            Side::Corp => &mut corp_agent,
            Side::Runner => &mut runner_agent,
        };
        let stats = agent.search(&view, &registry);
        let policy_target = normalized_policy_target(&stats.visit_counts);
        let chosen = choose_action(&stats.actions, ply, cli.temp_plies, repeats, &mut rng);
        repeats = if last_index == Some(chosen.index) { repeats + 1 } else { 0 };
        last_index = Some(chosen.index);

        steps.push(SelfPlayStep { observation, policy_target, action_taken: chosen.index, active_side: side as u8 });

        let (next, _events) = state.step(&registry, chosen.action.clone())?;
        state = next;
        ply += 1;
    }

    let outcome_corp = match state.phase {
        GamePhase::GameOver(Side::Corp) => 1.0,
        GamePhase::GameOver(Side::Runner) => -1.0,
        _ => 0.0,
    };

    Ok(GameTrajectory { steps, outcome_corp, matchup: matchup.id() })
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
    fs::create_dir_all(&cli.output_dir)?;

    (0..cli.num_games).into_par_iter().try_for_each(|game_index| -> Result<(), SelfPlayError> {
        let trajectory = play_one_game(game_index, &cli)?;
        write_trajectory(&cli.output_dir, game_index, &trajectory)
    })?;

    Ok(())
}
