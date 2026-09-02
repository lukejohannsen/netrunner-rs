//! `netrunner_cli bench`: the headless benchmark suite. Every ordered
//! pairing of the requested bot kinds plays `--games` matches in parallel,
//! and the results feed `netrunner_rating`'s bot-benchmark track — a
//! Glicko-2 rating per bot per role, printed as a ladder with its
//! uncertainty.
//!
//! **Why a ladder and not a win rate.** A trained policy is measured by
//! the arena's head-to-head score against one incumbent, which says
//! nothing about how it fares against the heuristic or the random bot,
//! and a win rate has no error bar. A rating per role against every kind
//! at once is one number per chair, with an interval that says how much
//! of it is evidence. `--bots puct,puct-onnx --model X` is the arena with
//! a ladder attached.
//!
//! **Rated in play order, after all games finish.** The games run on a
//! rayon pool and land in any order; the ratings are then applied by game
//! index, so the same arguments produce the same ladder however many
//! threads ran it (a Glicko-2 update is order-dependent). A stalled game
//! — `MAX_STEPS` with no winner — is counted in the pairing's tally and
//! not rated: `netrunner_rating` has no "nobody won" outcome on purpose.
//!
//! Plays through `netrunner_session::Session` like every other driver in
//! the workspace (AGENTS.md's Session Rule); this module owns scheduling
//! and bookkeeping, never a rule.

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use clap::ValueEnum;
use rayon::prelude::*;
use serde::Serialize;

use netrunner_core::cards::CardRegistry;
use netrunner_core::decks as core_decks;
use netrunner_core::rules::{GameState, Side};
use netrunner_rating::{Outcome, RatingBook, Standing, Track};
use netrunner_session::{GameEndReason, Seat, Session, SessionStep};

use crate::bots;
use crate::config::{BotKind, BotSpec, Config};
use crate::decks;

pub struct BenchArgs {
    pub bots: Vec<BotSpec>,
    pub games: u32,
    pub seed: Option<u64>,
    pub simulations: usize,
    pub threads: Option<usize>,
    pub report: Option<PathBuf>,
    pub ratings: Option<PathBuf>,
    pub label: Option<String>,
}

/// One game's result, as the report records it.
#[derive(Debug, Clone, Serialize)]
pub struct GameRecord {
    pub index: u32,
    pub seed: u64,
    pub corp: String,
    pub runner: String,
    pub matchup: String,
    pub steps: u32,
    /// `None` for a stall.
    pub winner: Option<Side>,
    pub reason: String,
}

/// A pairing's tally over its games.
#[derive(Debug, Clone, Serialize)]
pub struct PairingSummary {
    pub corp: String,
    pub runner: String,
    pub corp_wins: u32,
    pub runner_wins: u32,
    pub stalls: u32,
    pub mean_steps: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LadderRow {
    pub participant: String,
    pub overall: f64,
    pub standing: Standing,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchReport {
    pub bots: Vec<String>,
    pub games_per_pairing: u32,
    pub seed: u64,
    pub simulations: usize,
    pub games: Vec<GameRecord>,
    pub pairings: Vec<PairingSummary>,
    pub ladder: Vec<LadderRow>,
}

/// The participant id a bot rates under: the kind's name, the search
/// budget for the kinds that have one (`puct@32` and `puct@200` are
/// different players), the personality when it is not `balanced`
/// (`heuristic:rush`), and `--label` if given.
pub fn participant_id(bot: BotSpec, simulations: usize, label: Option<&str>) -> String {
    let name = bot.kind.to_possible_value().expect("every BotKind has a name").get_name().to_string();
    let mut id = match bot.kind {
        BotKind::Mcts | BotKind::Puct | BotKind::PuctOnnx => format!("{name}@{simulations}"),
        _ => name,
    };
    if bot.personality != netrunner_bots::Personality::Balanced {
        id.push(':');
        id.push_str(bot.personality.name());
    }
    if let Some(label) = label {
        id.push('#');
        id.push_str(label);
    }
    id
}

struct Job {
    index: u32,
    seed: u64,
    corp: BotSpec,
    runner: BotSpec,
}

pub fn run(args: &BenchArgs, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    for bot in &args.bots {
        if matches!(bot.kind, BotKind::Human | BotKind::Onnx) {
            return Err(format!("{:?} cannot be seated in a benchmark: it has no BotAgent form", bot.kind).into());
        }
    }
    if args.bots.is_empty() || args.games == 0 {
        return Err("bench needs at least one bot and one game per pairing".into());
    }

    let registry = decks::sample_deck_registry();
    let base_seed = args.seed.unwrap_or_else(rand::random);
    let matchups = core_decks::matchups();

    let mut jobs = Vec::new();
    for corp in &args.bots {
        for runner in &args.bots {
            for _ in 0..args.games {
                let index = jobs.len() as u32;
                jobs.push(Job { index, seed: base_seed.wrapping_add(u64::from(index)), corp: *corp, runner: *runner });
            }
        }
    }

    let started = Instant::now();
    let pool = match args.threads {
        Some(threads) => rayon::ThreadPoolBuilder::new().num_threads(threads).build()?,
        None => rayon::ThreadPoolBuilder::new().build()?,
    };
    let threads = pool.current_num_threads();
    let mut games: Vec<GameRecord> = pool.install(|| {
        jobs.par_iter()
            .map(|job| play(job, &registry, &matchups, args, config))
            .collect::<Result<Vec<_>, String>>()
    })?;
    games.sort_by_key(|game| game.index);
    let elapsed = started.elapsed();

    let mut book = match &args.ratings {
        Some(path) if path.exists() => RatingBook::from_json(&fs::read_to_string(path)?)?,
        _ => RatingBook::default(),
    };
    for game in &games {
        let outcome = match game.winner {
            Some(Side::Corp) => Outcome::CorpWin,
            Some(Side::Runner) => Outcome::RunnerWin,
            None => continue,
        };
        book.record(Track::BotBenchmark, &game.corp, &game.runner, outcome);
    }

    let pairings = summarize_pairings(&games, args);
    let ladder: Vec<LadderRow> = book
        .ladder(Track::BotBenchmark)
        .into_iter()
        .map(|(participant, standing)| LadderRow { participant: participant.to_string(), overall: standing.overall(), standing })
        .collect();

    println!(
        "Bot benchmark: {} pairings × {} games = {} games, seed {base_seed}, {} simulations, {threads} threads, {:.1}s",
        args.bots.len() * args.bots.len(),
        args.games,
        games.len(),
        args.simulations,
        elapsed.as_secs_f64()
    );
    println!();
    print!("{}", render_ladder(&ladder));
    println!();
    print!("{}", render_pairings(&pairings));

    if let Some(path) = &args.ratings {
        fs::write(path, book.to_json())?;
        println!("\nrating book saved to {}", path.display());
    }
    if let Some(path) = &args.report {
        let report = BenchReport {
            bots: args.bots.iter().map(|bot| participant_id(*bot, args.simulations, args.label.as_deref())).collect(),
            games_per_pairing: args.games,
            seed: base_seed,
            simulations: args.simulations,
            games,
            pairings,
            ladder,
        };
        fs::write(path, serde_json::to_string_pretty(&report)?)?;
        println!("report written to {}", path.display());
    }
    Ok(())
}

fn play(
    job: &Job,
    registry: &CardRegistry,
    matchups: &[(core_decks::DeckFile, core_decks::DeckFile)],
    args: &BenchArgs,
    config: &Config,
) -> Result<GameRecord, String> {
    let (corp_deck, runner_deck) = &matchups[job.index as usize % matchups.len()];
    let (state, _events) =
        GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), registry, job.seed).map_err(|e| format!("{e:?}"))?;
    let corp = bots::make_agent_with_model(job.corp.kind, Side::Corp, job.seed, args.simulations, &config.model, job.corp.personality)?
        .expect("kinds without a BotAgent form were rejected up front");
    let runner = bots::make_agent_with_model(
        job.runner.kind,
        Side::Runner,
        job.seed.wrapping_add(1),
        args.simulations,
        &config.model,
        job.runner.personality,
    )?
    .expect("kinds without a BotAgent form were rejected up front");
    let mut session = Session::new(state, registry.clone(), Seat::Agent(corp), Seat::Agent(runner));
    let outcome = session.run();
    let (winner, reason) = match outcome {
        SessionStep::Ended { winner, reason } => (Some(winner), describe_reason(reason)),
        SessionStep::Stalled(reason) => (None, format!("stalled: {reason:?}")),
        other => (None, format!("{other:?}")),
    };
    Ok(GameRecord {
        index: job.index,
        seed: job.seed,
        corp: participant_id(job.corp, args.simulations, args.label.as_deref()),
        runner: participant_id(job.runner, args.simulations, args.label.as_deref()),
        matchup: format!("{}_vs_{}", corp_deck.id, runner_deck.id),
        steps: session.steps(),
        winner,
        reason,
    })
}

fn describe_reason(reason: GameEndReason) -> String {
    format!("{reason:?}")
}

fn summarize_pairings(games: &[GameRecord], args: &BenchArgs) -> Vec<PairingSummary> {
    let mut pairings = Vec::new();
    for corp in &args.bots {
        for runner in &args.bots {
            let corp_id = participant_id(*corp, args.simulations, args.label.as_deref());
            let runner_id = participant_id(*runner, args.simulations, args.label.as_deref());
            let played: Vec<&GameRecord> = games.iter().filter(|g| g.corp == corp_id && g.runner == runner_id).collect();
            let steps: u64 = played.iter().map(|g| u64::from(g.steps)).sum();
            pairings.push(PairingSummary {
                corp_wins: played.iter().filter(|g| g.winner == Some(Side::Corp)).count() as u32,
                runner_wins: played.iter().filter(|g| g.winner == Some(Side::Runner)).count() as u32,
                stalls: played.iter().filter(|g| g.winner.is_none()).count() as u32,
                mean_steps: if played.is_empty() { 0.0 } else { steps as f64 / played.len() as f64 },
                corp: corp_id,
                runner: runner_id,
            });
        }
    }
    pairings
}

fn render_ladder(ladder: &[LadderRow]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<24} {:>7}   {:<20} {:<10}  {:<20} {:<10}\n",
        "participant", "overall", "corp (±2 RD)", "W-L-D", "runner (±2 RD)", "W-L-D"
    ));
    for row in ladder {
        let corp = &row.standing.corp;
        let runner = &row.standing.runner;
        out.push_str(&format!(
            "{:<24} {:>7.0}   {:<20} {:<10}  {:<20} {:<10}\n",
            row.participant,
            row.overall,
            format!("{:.0} ± {:.0}", corp.rating.rating, 2.0 * corp.rating.deviation),
            format!("{}-{}-{}", corp.wins, corp.losses, corp.draws),
            format!("{:.0} ± {:.0}", runner.rating.rating, 2.0 * runner.rating.deviation),
            format!("{}-{}-{}", runner.wins, runner.losses, runner.draws),
        ));
    }
    out
}

fn render_pairings(pairings: &[PairingSummary]) -> String {
    let mut out = String::new();
    out.push_str(&format!("{:<40} {:>9} {:>11} {:>7} {:>10}\n", "pairing (corp vs runner)", "corp wins", "runner wins", "stalls", "mean steps"));
    for pairing in pairings {
        out.push_str(&format!(
            "{:<40} {:>9} {:>11} {:>7} {:>10.0}\n",
            format!("{} vs {}", pairing.corp, pairing.runner),
            pairing.corp_wins,
            pairing.runner_wins,
            pairing.stalls,
            pairing.mean_steps
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::config::Command;

    fn parse(extra: &[&str]) -> (BenchArgs, Config) {
        let mut argv = vec!["netrunner_cli", "bench"];
        argv.extend_from_slice(extra);
        let mut config = Config::parse_from(argv);
        let Some(Command::Bench { bots, games, seed, simulations, threads, report, ratings, label }) = config.command.take() else {
            panic!("parsed a bench command");
        };
        (BenchArgs { bots, games, seed, simulations, threads, report, ratings, label }, config)
    }

    /// Two kinds, one game a pairing, two threads: four games, every seat
    /// rated, the report on disk, and the same ladder on a second run.
    #[test]
    fn a_bench_rates_every_seating_and_reproduces_from_its_seed() {
        let dir = std::env::temp_dir().join(format!("netrunner_bench_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let report_path = dir.join("bench.json");
        let ratings_path = dir.join("ratings.json");
        let (args, config) = parse(&[
            "--bots", "random,heuristic", "--games", "1", "--seed", "7", "--threads", "2",
            "--report", report_path.to_str().unwrap(), "--ratings", ratings_path.to_str().unwrap(),
        ]);
        run(&args, &config).expect("four random/heuristic games play out");

        let report: serde_json::Value = serde_json::from_str(&fs::read_to_string(&report_path).unwrap()).unwrap();
        assert_eq!(report["games"].as_array().unwrap().len(), 4);
        assert_eq!(report["pairings"].as_array().unwrap().len(), 4);
        let ladder = report["ladder"].as_array().unwrap();
        assert_eq!(ladder.len(), 2);
        for row in ladder {
            let games = |role: &str| {
                let r = &row["standing"][role];
                r["wins"].as_u64().unwrap() + r["losses"].as_u64().unwrap() + r["draws"].as_u64().unwrap()
            };
            // Each kind sat in each chair twice: against itself and the other.
            assert!(games("corp") <= 2 && games("runner") <= 2, "stalls are unrated, so at most two");
        }
        let first_book = RatingBook::from_json(&fs::read_to_string(&ratings_path).unwrap()).unwrap();
        assert!(!first_book.is_empty());

        // Same seed, fresh book, one thread: the same ladder, because the
        // ratings are applied in game order after the games finish.
        let fresh = dir.join("fresh.json");
        let (args, config) = parse(&["--bots", "random,heuristic", "--games", "1", "--seed", "7", "--threads", "1", "--ratings", fresh.to_str().unwrap()]);
        run(&args, &config).unwrap();
        let second_book = RatingBook::from_json(&fs::read_to_string(&fresh).unwrap()).unwrap();
        assert_eq!(first_book, second_book);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_kinds_carry_their_budget_and_personalities_their_name_in_the_participant_id() {
        let spec = |s: &str| s.parse::<BotSpec>().unwrap();
        assert_eq!(participant_id(spec("puct"), 64, None), "puct@64");
        assert_eq!(participant_id(spec("heuristic"), 64, None), "heuristic");
        assert_eq!(participant_id(spec("heuristic:rush"), 64, None), "heuristic:rush");
        assert_eq!(participant_id(spec("mcts:cautious"), 32, Some("abc123")), "mcts@32:cautious#abc123");
        assert!("heuristic:berserk".parse::<BotSpec>().is_err());
        assert!("android".parse::<BotSpec>().is_err());
    }

    #[test]
    fn a_human_or_bare_onnx_seat_is_refused_up_front() {
        let (args, config) = parse(&["--bots", "human,random", "--games", "1"]);
        assert!(run(&args, &config).is_err());
    }
}
