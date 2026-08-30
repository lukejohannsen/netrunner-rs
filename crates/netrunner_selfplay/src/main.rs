//! Runs `PuctAgent` vs. `PuctAgent` self-play games over the fixed Kate
//! vs. HB matchup (`fixtures`) and writes one JSONL trajectory file per
//! game into `--output-dir`, for an eventual training pipeline to consume.

mod fixtures;
mod schema;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Parser;
use rand::distr::weighted::WeightedIndex;
use rand::distr::Distribution;
use rand::rngs::StdRng;
use rand::SeedableRng;
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

/// Picks which searched action to actually play: weighted-sampled by
/// visit count while `ply < temp_plies` (the "temperature" phase), then
/// greedy (max visits) afterward — mirrors `PuctAgent::select_action`'s own
/// greedy tie-break once temperature drops out.
fn choose_action<'a>(actions: &'a [ActionStat], ply: usize, temp_plies: usize, rng: &mut StdRng) -> &'a ActionStat {
    if ply < temp_plies {
        let weights: Vec<u32> = actions.iter().map(|stat| stat.visits).collect();
        if let Ok(distribution) = WeightedIndex::new(&weights) {
            return &actions[distribution.sample(rng)];
        }
    }
    actions.iter().max_by_key(|stat| stat.visits).expect("PuctAgent::search always returns at least one action")
}

fn play_one_game(game_index: usize, cli: &Cli) -> Result<GameTrajectory, SelfPlayError> {
    let registry = fixtures::kate_vs_hb_registry();
    let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
    let seed = game_index as u64;

    let (mut state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

    let config = PuctConfig { iterations: cli.simulations.max(1), ..PuctConfig::default() };
    let mut corp_agent = PuctAgent::with_config(Side::Corp, seed, make_evaluator(Side::Corp, &cli.model_path)?, config);
    let mut runner_agent =
        PuctAgent::with_config(Side::Runner, seed.wrapping_add(1), make_evaluator(Side::Runner, &cli.model_path)?, config);
    let mut rng = StdRng::seed_from_u64(seed.wrapping_add(2));

    let mut steps = Vec::new();
    let mut ply = 0usize;

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
        let chosen = choose_action(&stats.actions, ply, cli.temp_plies, &mut rng);

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

    Ok(GameTrajectory { steps, outcome_corp })
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
