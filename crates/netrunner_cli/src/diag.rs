//! `netrunner_cli diag leaf-sensitivity`: how much of a leaf evaluation is
//! the hidden state?
//!
//! ROADMAP Phase 2 §5 item 35 left one question open. Four independent
//! determinizations are worth **+0.125** to `mcts` on the Runner chair and
//! **−0.102** to `puct` on the same games, and the standing hypothesis was
//! about what reads the sample: `mcts` values a leaf by *playing the
//! sampled hidden cards out* for 16 plies, while `puct` reads a static
//! `evaluate_state_with` a few plies down that may barely see what was
//! sampled. If so, PUCT marginalizes over nothing and only pays the depth.
//!
//! **The two leaves differ only by the plies in between.** `mcts::rollout`
//! walks a weighted-random policy for `depth_budget` plies and then calls
//! `evaluate_state_with`; `UniformPolicyEvaluator` calls
//! `evaluate_state_with` at the leaf directly. So one function with
//! `depth_budget` swept from 0 gives both searches' leaves on one scale,
//! and no normalization is needed to compare them — which is why this
//! measures through the real `rollout` rather than a copy of it.
//!
//! **Anchored, because a per-sample offset cancels in both searches.**
//! The naive statistic — the spread of a leaf value across determinizations
//! — overstates what either search can use. `PuctNode` scores leaves with
//! `evaluate_from`, relative to *that sample's own root*, so a
//! determinization that simply looks better for the Corp shifts every leaf
//! in its tree together and changes no ranking. `MctsAgent` backs up
//! absolute values, but merges root stats across trees by action, so a
//! common per-tree offset cancels there too. What a sample has to move to
//! be worth anything is the *difference between root actions*, so every
//! figure here is centred per determinization before its spread is taken.
//!
//! Two statistics, one computation:
//!
//! - **`between_sd` against `spread_sd`.** How far a determinization moves
//!   one action's score, against how far apart the actions are in the
//!   first place. Their ratio is the load-bearing number: near zero means
//!   averaging over samples cannot change the choice.
//! - **argmax agreement.** The same thing in the search's own currency —
//!   whether the sampled hidden state ever changes which action looks best.
//!   Decisive rather than suggestive: if every determinization ranks the
//!   actions identically, more samples provably buy nothing.
//!
//! `within_sd` is the rollout's own noise, the thing a deeper leaf pays to
//! read the hidden state at all; at depth 0 it is zero by construction.
//!
//! **Every figure past depth 0 is reported against a shared-sample
//! control, because otherwise it is unreadable.** Two determinizations can
//! disagree about the best action because they sampled different hidden
//! cards, or because a 16-ply weighted-random playout is noisy and they
//! rolled differently. The control is item 35's `--shared-sample` without
//! the games: the same N leaves, N rollout seeds, but **one**
//! determinization copied N times, so all of its disagreement is noise.
//! The hidden state's contribution is the gap between the two legs, and
//! at depth 0 the control is exact agreement by construction.
//!
//! Positions come from real games played through `netrunner_session`, not
//! from constructed states: the question is about the decisions these
//! searches actually face.

use std::fs;
use std::path::PathBuf;

use rand::SeedableRng;
use rand::rngs::StdRng;
use rayon::prelude::*;
use serde::Serialize;

use netrunner_bots::mcts::rollout;
use netrunner_bots::{Weights, determinize};
use netrunner_core::cards::CardRegistry;
use netrunner_core::decks as core_decks;
use netrunner_core::rules::{GameState, Side, apply_action, current_actor};
use netrunner_core::view::ClientView;
use netrunner_session::{Seat, Session, SessionStep};

use crate::bots;
use crate::config::{BotSpec, Config};
use crate::decks;

pub struct LeafSensitivityArgs {
    pub games: u32,
    pub positions: usize,
    pub seed: u64,
    pub determinizations: usize,
    pub rollouts: usize,
    pub depths: Vec<usize>,
    pub source: BotSpec,
    pub threads: Option<usize>,
    pub report: Option<PathBuf>,
}

/// One decision a real game reached, kept with everything needed to say
/// where it came from when a number looks wrong.
struct Position {
    game: u32,
    seed: u64,
    matchup: String,
    step: u32,
    side: Side,
    view: ClientView,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepthStat {
    pub depth: usize,
    /// Mean over actions of the sd across determinizations of that
    /// action's centred value — corrected for the rollout noise that
    /// leaks into it (`var_k / rollouts`, scaled by the `1 - 1/A` the
    /// centring leaves behind).
    pub between_sd: f64,
    /// Mean rollout noise within one (determinization, action). Zero at
    /// depth 0, which is deterministic.
    pub within_sd: f64,
    /// Sd across actions of the determinization-averaged centred value:
    /// how far apart this leaf thinks the root actions are at all.
    pub spread_sd: f64,
    /// `between_sd / spread_sd`. The headline: what fraction of the
    /// signal a search ranks on is attributable to the hidden state.
    pub between_over_spread: f64,
    /// Fraction of positions where every determinization's argmax is the
    /// same action.
    pub argmax_unanimous: f64,
    /// Mean over positions of the chance two determinizations picked the
    /// same argmax.
    pub argmax_agreement: f64,
    /// Median over positions of that position's own `between / spread`.
    /// The mean is a variance average and one decided position — a
    /// `ScoreAgenda` on the table is worth `agenda_point_weight` 20
    /// against a credit's 0.4 — swamps a hundred ordinary ones.
    pub ratio_median: f64,
    /// Fraction of positions where the leaf value is **bit-identical**
    /// across every determinization. The structural form of the claim:
    /// not "the sample moves it a little" but "the sample cannot move
    /// it at all".
    pub identical: f64,
    /// `between_sd` over one determinization copied N times: what the
    /// rollout's own noise contributes to the column above, after the
    /// same leak correction. The correction clamps at zero per action,
    /// so this is also the estimator's positive bias, measured.
    pub control_between_sd: f64,
    /// `argmax_agreement` with the hidden state held fixed.
    pub control_argmax_agreement: f64,
    /// `control_argmax_agreement - argmax_agreement`: how much of the
    /// disagreement is the hidden state rather than the dice.
    pub diversity_cost: f64,
    /// `diversity_cost` over the sd of its own per-position paired
    /// difference. Paired because the two legs share the position, the
    /// action set and the rollout seeds, and positions differ enormously
    /// in how much any leaf can tell them apart.
    pub diversity_cost_z: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChairStat {
    pub side: Side,
    pub positions: usize,
    pub mean_actions: f64,
    pub depths: Vec<DepthStat>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LeafSensitivityReport {
    pub games: u32,
    pub seed: u64,
    pub source: String,
    pub determinizations: usize,
    pub rollouts: usize,
    pub depths: Vec<usize>,
    pub positions_requested: usize,
    pub positions_measured: usize,
    /// Root actions dropped because `apply_action` refused them on at
    /// least one determinization — the sample can contradict a legal
    /// action, and a ragged matrix would bias every statistic.
    pub actions_dropped: usize,
    pub chairs: Vec<ChairStat>,
}

/// What one position contributes, before the per-chair average.
struct PositionResult {
    side: Side,
    actions: usize,
    /// Per depth, in `args.depths` order.
    depths: Vec<PositionDepth>,
    /// The same, over one determinization copied `determinizations`
    /// times: the rollout-noise-only control.
    shared: Vec<PositionDepth>,
    dropped: usize,
}

struct PositionDepth {
    between_var: f64,
    within_var: f64,
    spread_var: f64,
    argmax_unanimous: bool,
    argmax_agreement: f64,
    /// This position's own `between / spread`, before any pooling.
    ratio: f64,
    /// Every action's value identical across every determinization.
    identical: bool,
}

pub fn run(args: &LeafSensitivityArgs, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(threads) = args.threads {
        rayon::ThreadPoolBuilder::new().num_threads(threads).build_global()?;
    }
    let registry = decks::sample_deck_registry();
    let source = format!("{:?}:{:?}", args.source.kind, args.source.personality).to_lowercase();

    println!("collecting positions from {} games seated {source} vs {source}...", args.games);
    let positions = collect_positions(args, &registry, config)?;
    println!("  {} decision positions sampled", positions.len());

    let weights = Weights::default();
    let results: Vec<PositionResult> =
        positions.par_iter().map(|position| measure(position, args, &registry, &weights)).collect();

    let report = LeafSensitivityReport {
        games: args.games,
        seed: args.seed,
        source,
        determinizations: args.determinizations,
        rollouts: args.rollouts,
        depths: args.depths.clone(),
        positions_requested: args.positions,
        positions_measured: results.len(),
        actions_dropped: results.iter().map(|r| r.dropped).sum(),
        chairs: [Side::Corp, Side::Runner].iter().map(|side| summarize(*side, &results, args)).collect(),
    };

    print_report(&report);
    if let Some(path) = &args.report {
        fs::write(path, serde_json::to_string_pretty(&report)?)?;
        println!("\nreport written to {}", path.display());
    }
    Ok(())
}

/// Plays `games` matches with `source` in both chairs and keeps evenly
/// spaced decisions from each. Evenly spaced rather than the first N,
/// because the question is about mid-game decisions and the opening is
/// where a game spends its cheapest steps; and rather than random, so a
/// re-run of the same seed measures the same positions.
fn collect_positions(
    args: &LeafSensitivityArgs,
    registry: &CardRegistry,
    config: &Config,
) -> Result<Vec<Position>, Box<dyn std::error::Error>> {
    let matchups = core_decks::matchups();
    let mut collected = Vec::new();
    for game in 0..args.games {
        let seed = args.seed.wrapping_add(u64::from(game));
        let (corp_deck, runner_deck) = &matchups[game as usize % matchups.len()];
        let (state, _events) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), registry, seed)?;
        let setup = bots::AgentSetup::new(32).with_personality(args.source.personality);
        let corp = bots::make_agent_with_model(args.source.kind, Side::Corp, seed, setup, &config.model)?
            .ok_or("the position source must be a bot that can take a seat")?;
        let runner =
            bots::make_agent_with_model(args.source.kind, Side::Runner, seed.wrapping_add(1), setup, &config.model)?
                .ok_or("the position source must be a bot that can take a seat")?;
        let mut session =
            Session::new(state, registry.clone(), Seat::Agent(corp), Seat::Agent(runner)).without_history();

        let matchup = format!("{}_vs_{}", corp_deck.id, runner_deck.id);
        let mut seen: Vec<Position> = Vec::new();
        loop {
            if let Some(side) = current_actor(session.state()) {
                let view = session.view_for(side);
                // A decision with one legal action has nothing for a
                // determinization to change; measuring it would dilute
                // every average with a guaranteed zero.
                if view.legal_actions.len() > 1 {
                    seen.push(Position { game, seed, matchup: matchup.clone(), step: session.steps(), side, view });
                }
            }
            match session.step() {
                SessionStep::Applied { .. } => continue,
                _ => break,
            }
        }
        for side in [Side::Corp, Side::Runner] {
            let indices: Vec<usize> = seen.iter().enumerate().filter(|(_, p)| p.side == side).map(|(i, _)| i).collect();
            collected.extend(evenly_spaced(&indices, args.positions).into_iter().map(|i| Position {
                game: seen[i].game,
                seed: seen[i].seed,
                matchup: seen[i].matchup.clone(),
                step: seen[i].step,
                side: seen[i].side,
                view: seen[i].view.clone(),
            }));
        }
    }
    Ok(collected)
}

/// `want` items spread across `from`, endpoints included. Fewer than
/// `want` available means take them all.
fn evenly_spaced(from: &[usize], want: usize) -> Vec<usize> {
    if want == 0 {
        return Vec::new();
    }
    if from.len() <= want {
        return from.to_vec();
    }
    if want == 1 {
        return vec![from[from.len() / 2]];
    }
    (0..want).map(|k| from[k * (from.len() - 1) / (want - 1)]).collect()
}

fn measure(position: &Position, args: &LeafSensitivityArgs, registry: &CardRegistry, weights: &Weights) -> PositionResult {
    let side = position.side;
    // Seeded off the position, not off an iteration counter, so rayon's
    // scheduling cannot change a number.
    let base = args.seed ^ (u64::from(position.game) << 32) ^ u64::from(position.step) ^ (side as u64) << 16;

    let samples: Vec<GameState> = (0..args.determinizations)
        .map(|i| {
            let mut rng = StdRng::seed_from_u64(base.wrapping_add(i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            determinize(&position.view, registry, &mut rng)
        })
        .collect();

    // A determinization can contradict an action the view calls legal
    // (`determinize` samples some other card into a parked decision), so
    // keep only the actions that apply on *every* sample: an action
    // missing from one sample's row would show up as hidden-state
    // sensitivity that is really a hole in the matrix.
    let mut children: Vec<Vec<GameState>> = Vec::new();
    let mut dropped = 0;
    for action in &position.view.legal_actions {
        let row: Option<Vec<GameState>> = samples
            .iter()
            .map(|sample| apply_action(sample, registry, action.clone()).ok().map(|(next, _)| next))
            .collect();
        match row {
            Some(row) => children.push(row),
            None => dropped += 1,
        }
    }

    // The control: one determinization, copied. Same leaves, same
    // rollout seeds, nothing left to disagree about but the dice.
    let shared_children: Vec<Vec<GameState>> =
        children.iter().map(|row| std::iter::repeat_n(row[0].clone(), row.len()).collect()).collect();

    let actions = children.len();
    let depths = args
        .depths
        .iter()
        .map(|&depth| depth_stat(&children, depth, args, registry, side, weights, base))
        .collect();
    let shared = args
        .depths
        .iter()
        .map(|&depth| depth_stat(&shared_children, depth, args, registry, side, weights, base))
        .collect();
    PositionResult { side, actions, depths, shared, dropped }
}

fn depth_stat(
    children: &[Vec<GameState>],
    depth: usize,
    args: &LeafSensitivityArgs,
    registry: &CardRegistry,
    side: Side,
    weights: &Weights,
    base: u64,
) -> PositionDepth {
    let actions = children.len();
    let dets = args.determinizations;
    // Depth 0 is `evaluate_state_with` itself: deterministic, so repeating
    // it would only multiply the cost of a constant.
    let k = if depth == 0 { 1 } else { args.rollouts };
    if actions < 2 || dets < 2 {
        return PositionDepth {
            between_var: 0.0,
            within_var: 0.0,
            spread_var: 0.0,
            argmax_unanimous: true,
            argmax_agreement: 1.0,
            ratio: 0.0,
            identical: true,
        };
    }

    // value[a][i] = mean over rollouts; noise[a][i] = their variance.
    let mut value = vec![vec![0.0f64; dets]; actions];
    let mut noise = vec![vec![0.0f64; dets]; actions];
    for (a, row) in children.iter().enumerate() {
        for (i, state) in row.iter().enumerate() {
            let mut rng = StdRng::seed_from_u64(
                base ^ ((a as u64) << 40) ^ ((i as u64) << 24) ^ ((depth as u64) << 8),
            );
            let draws: Vec<f64> = (0..k).map(|_| rollout(state, registry, side, depth, &mut rng, weights)).collect();
            value[a][i] = mean(&draws);
            noise[a][i] = if k > 1 { variance(&draws) } else { 0.0 };
        }
    }

    // Centre each determinization across the actions: a sample that simply
    // looks better shifts its whole column, and neither search can use it.
    let mut centred = value.clone();
    for i in 0..dets {
        let column_mean = (0..actions).map(|a| value[a][i]).sum::<f64>() / actions as f64;
        for row in centred.iter_mut() {
            row[i] -= column_mean;
        }
    }

    let within_var = noise.iter().flatten().sum::<f64>() / (actions * dets) as f64;
    // Rollout noise inflates the observed between-determinization spread
    // by `within_var / k`; centring across `actions` leaves `1 - 1/A` of
    // it. Subtract it rather than reporting a floor that is really noise.
    let leak = (within_var / k as f64) * (1.0 - 1.0 / actions as f64);
    let between_var = centred
        .iter()
        .map(|row| (variance(row) - leak).max(0.0))
        .sum::<f64>()
        / actions as f64;
    let action_means: Vec<f64> = centred.iter().map(|row| mean(row)).collect();
    let spread_var = variance(&action_means);

    let argmaxes: Vec<usize> = (0..dets)
        .map(|i| (0..actions).max_by(|&a, &b| value[a][i].total_cmp(&value[b][i])).unwrap_or(0))
        .collect();
    let mut agree = 0usize;
    let mut pairs = 0usize;
    for i in 0..dets {
        for j in (i + 1)..dets {
            pairs += 1;
            if argmaxes[i] == argmaxes[j] {
                agree += 1;
            }
        }
    }
    PositionDepth {
        between_var,
        within_var,
        spread_var,
        argmax_unanimous: argmaxes.iter().all(|&a| a == argmaxes[0]),
        argmax_agreement: if pairs == 0 { 1.0 } else { agree as f64 / pairs as f64 },
        ratio: if spread_var > 0.0 { (between_var / spread_var).sqrt() } else { 0.0 },
        identical: value.iter().all(|row| row.iter().all(|v| *v == row[0])),
    }
}

fn summarize(side: Side, results: &[PositionResult], args: &LeafSensitivityArgs) -> ChairStat {
    let mine: Vec<&PositionResult> = results.iter().filter(|r| r.side == side).collect();
    let n = mine.len().max(1) as f64;
    let depths = args
        .depths
        .iter()
        .enumerate()
        .map(|(d, &depth)| {
            let paired: Vec<f64> =
                mine.iter().map(|r| r.shared[d].argmax_agreement - r.depths[d].argmax_agreement).collect();
            // Averaged as variances and rooted once: an sd is not linear,
            // and averaging sds across positions would under-report the
            // spread of the pooled quantity.
            let between = (mine.iter().map(|r| r.depths[d].between_var).sum::<f64>() / n).sqrt();
            let spread = (mine.iter().map(|r| r.depths[d].spread_var).sum::<f64>() / n).sqrt();
            DepthStat {
                depth,
                between_sd: between,
                within_sd: (mine.iter().map(|r| r.depths[d].within_var).sum::<f64>() / n).sqrt(),
                spread_sd: spread,
                between_over_spread: if spread > 0.0 { between / spread } else { 0.0 },
                argmax_unanimous: mine.iter().filter(|r| r.depths[d].argmax_unanimous).count() as f64 / n,
                argmax_agreement: mine.iter().map(|r| r.depths[d].argmax_agreement).sum::<f64>() / n,
                ratio_median: median(&mine.iter().map(|r| r.depths[d].ratio).collect::<Vec<f64>>()),
                identical: mine.iter().filter(|r| r.depths[d].identical).count() as f64 / n,
                control_between_sd: (mine.iter().map(|r| r.shared[d].between_var).sum::<f64>() / n).sqrt(),
                control_argmax_agreement: mine.iter().map(|r| r.shared[d].argmax_agreement).sum::<f64>() / n,
                diversity_cost: mean(&paired),
                diversity_cost_z: {
                    let sd = variance(&paired).sqrt();
                    if sd > 0.0 { mean(&paired) / (sd / n.sqrt()) } else { 0.0 }
                },
            }
        })
        .collect();
    ChairStat {
        side,
        positions: mine.len(),
        mean_actions: mine.iter().map(|r| r.actions as f64).sum::<f64>() / n,
        depths,
    }
}

fn median(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut sorted = xs.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) { (sorted[mid - 1] + sorted[mid]) / 2.0 } else { sorted[mid] }
}

fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}

/// Sample variance (`n - 1`), zero for fewer than two observations.
fn variance(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let m = mean(xs);
    xs.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (xs.len() - 1) as f64
}

fn print_report(report: &LeafSensitivityReport) {
    println!(
        "\n{} determinizations x {} rollouts, {} positions ({} root actions dropped)",
        report.determinizations, report.rollouts, report.positions_measured, report.actions_dropped
    );
    for chair in &report.chairs {
        println!(
            "\n{:?} chair — {} positions, {:.1} root actions each",
            chair.side, chair.positions, chair.mean_actions
        );
        println!(
            "  {:>5}  {:>9}  {:>9}  {:>9}  {:>7}  {:>9}  {:>9}  {:>9}  {:>9}",
            "depth", "between", "ctl.betw", "spread", "median", "identical", "unanimous", "agreement", "ctl.agree"
        );
        for d in &chair.depths {
            println!(
                "  {:>5}  {:>9.3}  {:>9.3}  {:>9.3}  {:>7.3}  {:>9.3}  {:>9.3}  {:>9.3}  {:>9.3}",
                d.depth,
                d.between_sd,
                d.control_between_sd,
                d.spread_sd,
                d.ratio_median,
                d.identical,
                d.argmax_unanimous,
                d.argmax_agreement,
                d.control_argmax_agreement
            );
        }
        println!(
            "  hidden-state cost to argmax agreement, paired: {}",
            chair
                .depths
                .iter()
                .map(|d| format!("d{}: {:+.3} (z {:+.2})", d.depth, d.diversity_cost, d.diversity_cost_z))
                .collect::<Vec<String>>()
                .join("  ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evenly_spaced_takes_the_endpoints_and_spreads_the_rest() {
        // The opening is where a game spends its cheapest decisions, so
        // "the first N" would measure almost none of the midgame.
        assert_eq!(evenly_spaced(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9], 4), vec![0, 3, 6, 9]);
        assert_eq!(evenly_spaced(&[0, 1, 2], 8), vec![0, 1, 2], "fewer than asked for is all of them");
        assert_eq!(evenly_spaced(&[0, 1, 2, 3], 1), vec![2], "one lands mid-game, never on the opening");
        assert!(evenly_spaced(&[0, 1], 0).is_empty());
    }

    #[test]
    fn variance_is_the_sample_estimator_and_zero_below_two_observations() {
        assert_eq!(variance(&[]), 0.0);
        assert_eq!(variance(&[4.0]), 0.0);
        // n - 1: ((1-3)^2 + (5-3)^2) / 1.
        assert_eq!(variance(&[1.0, 5.0]), 8.0);
    }

    #[test]
    fn median_interpolates_an_even_count() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
        assert_eq!(median(&[]), 0.0);
    }
}
