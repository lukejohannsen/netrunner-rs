//! Runs `PuctAgent` vs. `PuctAgent` self-play games over Null Signal Games'
//! System Gateway sample decks (`fixtures`) and writes one JSONL trajectory
//! file per game into `--output-dir` for the training pipeline to consume.
//!
//! Games rotate through all twelve Corp/Runner pairings by default so a
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
use rand::distr::weighted::WeightedIndex;
use rand::distr::Distribution;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;

use netrunner_bots::{encode_observation, ActionStat, PolicyEvaluator, PuctAgent, PuctConfig, UniformPolicyEvaluator};
#[cfg(feature = "onnx")]
use netrunner_bots::OnnxPolicyEvaluator;
use netrunner_core::rules::{ActionSpace, GamePhase, GameState, RulesError, Side};
use netrunner_session::{Seat, Session, SessionStep, SubmitError};

use schema::{GameTrajectory, SelfPlayStep};
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
    /// all twelve pairings, which is what a general training set wants.
    #[arg(long = "matchup")]
    matchup: Option<String>,
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

/// How one arena game went, from the candidate's side of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArenaResult {
    CandidateWin,
    IncumbentWin,
    /// The session stalled at `MAX_STEPS` — nobody won, and it counts as
    /// half a point each in `ArenaSummary::candidate_score`.
    Draw,
}

/// The one line arena mode prints: what `scripts/run_iteration_loop.py`
/// reads to decide a promotion.
#[derive(Debug, Serialize, PartialEq)]
struct ArenaSummary {
    games: usize,
    candidate_wins: usize,
    incumbent_wins: usize,
    draws: usize,
    /// `(wins + draws / 2) / games` — 1.0 is a clean sweep, 0.5 is parity.
    candidate_score: f64,
}

fn summarize(results: &[ArenaResult]) -> ArenaSummary {
    let count = |wanted: ArenaResult| results.iter().filter(|r| **r == wanted).count();
    let (candidate_wins, incumbent_wins, draws) =
        (count(ArenaResult::CandidateWin), count(ArenaResult::IncumbentWin), count(ArenaResult::Draw));
    let games = results.len();
    let candidate_score = if games == 0 { 0.0 } else { (candidate_wins as f64 + draws as f64 / 2.0) / games as f64 };
    ArenaSummary { games, candidate_wins, incumbent_wins, draws, candidate_score }
}

/// The candidate takes the Corp chair in even-numbered games and the
/// Runner chair in odd ones, so with the matchup rotating on the same
/// index every pairing is played from both sides over a 24-game arena.
fn candidate_side(game_index: usize) -> Side {
    if game_index.is_multiple_of(2) { Side::Corp } else { Side::Runner }
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
) -> Result<ArenaResult, SelfPlayError> {
    let registry = fixtures::registry();
    let matchup = matchup_for(game_index, cli)?;
    let (corp_deck, runner_deck) = matchup.decks();
    let seed = game_index as u64;
    let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

    let candidate_side = candidate_side(game_index);
    let config = PuctConfig { iterations: cli.simulations.max(1), ..PuctConfig::default() };
    let seat = |side: Side, seat_seed: u64| -> Result<Seat, SelfPlayError> {
        let model = if side == candidate_side { candidate } else { incumbent };
        let evaluator = make_evaluator(side, model)?;
        Ok(Seat::Agent(Box::new(PuctAgent::with_config(side, seat_seed, evaluator, config))))
    };
    let corp = seat(Side::Corp, seed)?;
    let runner = seat(Side::Runner, seed.wrapping_add(1))?;
    let mut session = Session::new(state, registry, corp, runner).without_history();
    match session.run() {
        SessionStep::Ended { winner, .. } => {
            Ok(if winner == candidate_side { ArenaResult::CandidateWin } else { ArenaResult::IncumbentWin })
        }
        SessionStep::Stalled(_) => Ok(ArenaResult::Draw),
        other => Err(SelfPlayError::ArenaUnexpectedStep(format!("{other:?}"))),
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
///
/// `None` only if `actions` is empty, which `PuctAgent::search` no longer
/// produces — it reports one stat per `view.legal_actions` entry. Returned
/// as an `Option` rather than asserted anyway: this runs unattended for
/// millions of games, and killing a whole training run over one
/// unreachable-by-contract decision is the wrong trade. The caller plays a
/// legal action and records nothing for that ply instead.
fn choose_action<'a>(
    actions: &'a [ActionStat],
    ply: usize,
    temp_plies: usize,
    repeats: usize,
    rng: &mut StdRng,
) -> Option<&'a ActionStat> {
    if actions.is_empty() {
        return None;
    }
    if ply < temp_plies || repeats >= MAX_GREEDY_REPEATS {
        let weights: Vec<u32> = actions.iter().map(|stat| stat.visits).collect();
        if let Ok(distribution) = WeightedIndex::new(&weights) {
            return Some(&actions[distribution.sample(rng)]);
        }
        // Every action has zero visits, so visit-weighted sampling has
        // nothing to work with. Uniform choice still breaks the cycle.
        if repeats >= MAX_GREEDY_REPEATS {
            return Some(&actions[rng.random_range(0..actions.len())]);
        }
    }
    actions.iter().max_by_key(|stat| stat.visits)
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
    // Consecutive identical chosen indices, for `choose_action`'s
    // cycle-breaking fallback.
    let mut last_index: Option<usize> = None;
    let mut repeats = 0usize;

    // The session's own `MAX_STEPS` bounds this now; the local copy this
    // loop used to carry is gone.
    while let SessionStep::Awaiting { side, view } = session.step() {
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

        let Some(chosen) = choose_action(&stats.actions, ply, cli.temp_plies, repeats, &mut rng) else {
            // Unreachable by `search`'s contract. Play on rather than end
            // the run; the ply simply contributes no training example.
            session.submit(view.legal_actions[0].clone())?;
            continue;
        };
        let chosen_index = ActionSpace::index_of(session.state(), &chosen.action);
        repeats = if last_index.is_some() && last_index == chosen_index { repeats + 1 } else { 0 };
        last_index = chosen_index;

        // A step whose action has no slot in the real state's encoding
        // can't be labelled, so it is played but not recorded.
        if let Some(action_taken) = chosen_index {
            steps.push(SelfPlayStep { observation, policy_target, action_taken, active_side: side as u8 });
        }

        session.submit(chosen.action.clone())?;
        ply += 1;
    }

    let outcome_corp = match session.state().phase {
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

    if cli.arena_candidate.is_some() {
        let results = (0..cli.num_games)
            .into_par_iter()
            .map(|game_index| play_arena_game(game_index, &cli, &cli.arena_candidate, &cli.arena_incumbent))
            .collect::<Result<Vec<_>, _>>()?;
        println!("{}", serde_json::to_string(&summarize(&results))?);
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
        assert!(choose_action(&[], 0, 8, 0, &mut rng).is_none());
        assert!(choose_action(&[], 99, 8, MAX_GREEDY_REPEATS, &mut rng).is_none());
    }

    #[test]
    fn candidate_score_counts_a_stall_as_half() {
        let summary = summarize(&[ArenaResult::CandidateWin, ArenaResult::Draw, ArenaResult::IncumbentWin, ArenaResult::Draw]);
        assert_eq!(
            summary,
            ArenaSummary { games: 4, candidate_wins: 1, incumbent_wins: 1, draws: 2, candidate_score: 0.5 }
        );
        assert_eq!(summarize(&[]).candidate_score, 0.0, "no games is not a pass");
    }

    #[test]
    fn the_candidate_alternates_chairs_across_games() {
        assert_eq!(candidate_side(0), Side::Corp);
        assert_eq!(candidate_side(1), Side::Runner);
        assert_eq!(candidate_side(12), Side::Corp, "the same matchup, the other chair");
    }

    /// The arena path end to end with no network on either side: two real
    /// games at a token search budget, summarised. Guards the seat wiring
    /// and the JSON shape the training loop parses.
    #[test]
    fn a_uniform_arena_plays_its_games_and_accounts_for_every_one() {
        let cli = Cli::parse_from(["netrunner_selfplay", "-n", "2", "-s", "2", "--arena-candidate", "unused.onnx"]);
        let results: Vec<ArenaResult> =
            (0..2).map(|i| play_arena_game(i, &cli, &None, &None).expect("uniform arena game")).collect();
        let summary = summarize(&results);
        assert_eq!(summary.games, 2);
        assert_eq!(summary.candidate_wins + summary.incumbent_wins + summary.draws, 2);
        let line = serde_json::to_string(&summary).unwrap();
        assert!(line.contains("\"candidate_score\""), "{line}");
    }
}
