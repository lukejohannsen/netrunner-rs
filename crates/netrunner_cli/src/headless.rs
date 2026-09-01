//! Non-rendered bot-vs-bot play that *reports what the rules did*.
//!
//! Runs `--games` matches through `netrunner_session::Session` on the real
//! sample decks and prints a `netrunner_session::Coverage` table: every
//! `PlayerAction` variant with its applied count (never-applied first),
//! how runs ended per server, and which sample-deck cards were ever
//! installed, played, rezzed, accessed or trashed. `--report` writes the
//! same numbers as sorted JSON so two runs can be `diff`ed.
//!
//! This mode used to assert only "every game reached `GameOver`" and print
//! one line — over a synthetic Kate-vs-HB filler matchup, ignoring
//! `--corp-deck`. That is the shape of harness under which no program could
//! be installed for months without anyone noticing: reachability says
//! nothing about *which* rules ran. See ROADMAP's "Rules Audit".
//!
//! A stalled game is counted under `end_reasons`, not treated as an error:
//! the sweeps are the assertion that games finish; this is the report.
//! Random-vs-random in particular can exhaust `MAX_STEPS` on some seeds, and
//! that count is itself worth tracking before and after an engine change.

use std::fs;

use netrunner_core::cards::CardRegistry;
use netrunner_core::decks as core_decks;
use netrunner_core::rules::{Deck, GameState, Side};
use netrunner_session::coverage::sample_pool_card_ids;
use netrunner_session::{Coverage, Seat, Session, SessionStep};
use netrunner_single_player::SinglePlayerSession;

use crate::bots;
use crate::config::{BotKind, Config};
use crate::decks;

/// A `Human` agent can't drive a headless game forward on its own, so it
/// falls back to `Random` here — see `config::Config::corp`'s doc comment.
fn headless_kind(kind: BotKind) -> BotKind {
    if matches!(kind, BotKind::Human) {
        BotKind::Random
    } else {
        kind
    }
}

/// The decks game `index` plays: either the fixed `--corp-deck` /
/// `--runner-deck` pair, or — under `--all-matchups` — the sample matchup
/// at `index % 12`, the same rotation the sweeps and self-play use.
fn deck_pair(config: &Config, registry: &CardRegistry, index: u32) -> Result<(String, Deck, Deck), String> {
    if config.all_matchups {
        let matchups = core_decks::matchups();
        let (corp, runner) = &matchups[index as usize % matchups.len()];
        return Ok((format!("{}_vs_{}", corp.id, runner.id), corp.to_deck(), runner.to_deck()));
    }
    let decks_dir = crate::deck_store::resolve_decks_dir(config.decks_dir.as_deref())?;
    let (corp, runner) =
        decks::decks_for_match(&decks_dir, &config.corp_deck, &config.runner_deck, registry, config.format.into())?;
    Ok((format!("{}_vs_{}", config.corp_deck, config.runner_deck), corp, runner))
}

pub fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    // `OnnxPolicyEvaluator` has no `BotAgent` form and the index path here
    // is for the scripted kinds; reject it explicitly rather than let
    // `make_agent`'s `None` become a panic below.
    for (side, kind) in [("--corp", config.corp), ("--runner", config.runner)] {
        if matches!(kind, BotKind::Onnx) {
            return Err(format!(
                "{side} onnx is not supported in --headless mode; it is available in local \
                 interactive play (drop --headless)"
            )
            .into());
        }
    }

    let registry = decks::sample_deck_registry();
    let base_seed = config.seed.unwrap_or_else(rand::random);
    let corp_kind = headless_kind(config.corp);
    let runner_kind = headless_kind(config.runner);
    let mut coverage = Coverage::default();

    for game_index in 0..config.games {
        let seed = base_seed.wrapping_add(u64::from(game_index));
        let (matchup, corp_deck, runner_deck) = deck_pair(config, &registry, game_index)?;
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

        let (history, outcome, steps) = if config.index_path {
            let corp = bots::make_driver(corp_kind, Side::Corp, seed, config.simulations, &config.model)?;
            let runner = bots::make_driver(runner_kind, Side::Runner, seed.wrapping_add(1), config.simulations, &config.model)?;
            let (_state, history, outcome) = SinglePlayerSession::new(state, registry.clone(), corp, runner).run_with_outcome();
            let steps = history.len();
            (history, outcome, steps)
        } else {
            let corp = bots::make_agent(corp_kind, Side::Corp, seed, config.simulations)
                .expect("headless_kind never resolves to a kind without a BotAgent form");
            let runner = bots::make_agent(runner_kind, Side::Runner, seed.wrapping_add(1), config.simulations)
                .expect("headless_kind never resolves to a kind without a BotAgent form");
            let mut session = Session::new(state, registry.clone(), Seat::Agent(corp), Seat::Agent(runner));
            let outcome = session.run();
            let steps = session.steps() as usize;
            let (_state, history) = session.into_parts();
            (history, outcome, steps)
        };

        if config.verbose {
            println!("game {game_index:>5} seed {seed:<20} {matchup:<40} {steps:>6} steps  {}", describe(&outcome));
        }
        coverage.absorb_match(&history, &registry, &outcome);
    }

    let universe = sample_pool_card_ids(&registry);
    print!("{}", coverage.render_table(&universe));
    if let Some(path) = &config.report {
        fs::write(path, coverage.to_json())?;
        println!("\nreport written to {}", path.display());
    }
    Ok(())
}

fn describe(outcome: &SessionStep) -> String {
    match outcome {
        SessionStep::Ended { winner, reason } => format!("{winner:?} wins by {reason:?}"),
        SessionStep::Stalled(reason) => format!("stalled: {reason:?}"),
        other => format!("{other:?}"),
    }
}
