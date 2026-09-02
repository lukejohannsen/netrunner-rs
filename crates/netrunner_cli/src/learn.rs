//! `netrunner_cli learn …`: the lesson tracks and the starter game
//! (ROADMAP Phase 1.75 §7/§8).
//!
//! Lessons are data in `netrunner_core::tutorial`; this module only picks
//! which to play and hands them to the TUI. `track` plays a side's lessons
//! in order and, if the player completes them all, graduates straight into
//! that side's starter game — the hand-off §8 describes.

use netrunner_core::decks::{self, DeckFile};
use netrunner_core::rules::Side;
use netrunner_core::tutorial;

use crate::config::{Config, LearnAction, SideArg};
use crate::tui::{self, LessonOutcome};

pub fn run(action: LearnAction, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        LearnAction::List => {
            list();
            Ok(())
        }
        LearnAction::Lesson { id } => {
            let lesson = tutorial::by_id(&id).ok_or_else(|| format!("no lesson with id {id:?} — see `learn list`"))?;
            let registry = crate::decks::sample_deck_registry();
            tui::run_lessons(&[lesson], &registry, seed(config))?;
            Ok(())
        }
        LearnAction::Track { side } => {
            let side: Side = side.into();
            let lessons = tutorial::track(side);
            let registry = crate::decks::sample_deck_registry();
            if tui::run_lessons(&lessons, &registry, seed(config))? == LessonOutcome::Completed {
                let (corp, runner) = starter_decks(false)?;
                tui::run_starter_game(side, &corp, &runner, config)?;
            }
            Ok(())
        }
        LearnAction::Game { side, boosted } => {
            let (corp, runner) = starter_decks(boosted)?;
            tui::run_starter_game(side.into(), &corp, &runner, config)
        }
    }
}

/// A lesson's seed only reaches the engine's RNG (a mulligan reshuffle, a
/// random access), so it defaults to a constant: the same lesson should
/// play the same way twice unless the player asks otherwise with `--seed`.
fn seed(config: &Config) -> u64 {
    config.seed.unwrap_or(0)
}

fn starter_decks(boosted: bool) -> Result<(DeckFile, DeckFile), String> {
    let (corp, runner) =
        if boosted { ("the_syndicate_boosted", "the_catalyst_boosted") } else { ("the_syndicate_starter", "the_catalyst_starter") };
    let load = |id: &str| decks::by_id(id).ok_or_else(|| format!("embedded deck {id:?} is missing"));
    Ok((load(corp)?, load(runner)?))
}

fn list() {
    for side in [SideArg::Corp, SideArg::Runner] {
        let side: Side = side.into();
        println!("{side:?} track:");
        for (index, lesson) in tutorial::track(side).iter().enumerate() {
            println!("  {:>2}. {:<28} {}", index + 1, lesson.id, lesson.title);
        }
        println!();
    }
    println!("Play one with `learn lesson <id>`, a whole track with `learn track corp|runner`,");
    println!("or the unguided starter game with `learn game corp|runner [--boosted]`.");
}
