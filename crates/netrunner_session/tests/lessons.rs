//! The two lesson gates ROADMAP Phase 1.75 §9 requires, run over every
//! embedded lesson:
//!
//! - **every lesson is completable** — the author's `solution` for each
//!   step is played through the step's own `allow` filter, and the step
//!   must advance by the time its solution is exhausted; the lesson must
//!   reach `Complete`, never `Ended` or `Stalled`. The lesson analogue of
//!   `every_sample_deck_matchup_finishes`.
//! - **every gated step offers at least one action** at every point it is
//!   live — the deadlock analogue, and why §6 gates on a predicate rather
//!   than an index.
//!
//! A failure names the lesson, the step, and the legal actions at that
//! point, which is what an author needs to fix the JSON.

use std::collections::VecDeque;

use netrunner_core::cards::{register_playable_cards, CardRegistry};
use netrunner_core::tutorial::embedded_lessons;
use netrunner_session::{LessonSession, LessonStep};

const MAX_PROMPTS: usize = 500;

#[test]
fn every_lesson_is_completable_through_its_own_gates() {
    let mut registry = CardRegistry::new();
    register_playable_cards(&mut registry);
    let lessons = embedded_lessons();
    assert!(!lessons.is_empty());

    for lesson in lessons {
        let id = lesson.id.clone();
        let steps = lesson.steps.clone();
        let mut session = LessonSession::start(lesson, registry.clone(), 0).unwrap_or_else(|e| panic!("lesson {id}: {e}"));
        let mut queue: VecDeque<netrunner_core::rules::PlayerAction> = VecDeque::new();
        let mut queued_for: Option<usize> = None;
        let mut prompts = 0;

        loop {
            prompts += 1;
            assert!(prompts <= MAX_PROMPTS, "lesson {id}: {MAX_PROMPTS} prompts without completing");
            match session.step().unwrap_or_else(|e| panic!("lesson {id}: {e}")) {
                LessonStep::Prompt { view, allowed, step, total } => {
                    assert!(
                        !allowed.is_empty(),
                        "lesson {id} step {step}/{total} offers no action at turn {} ({:?}); legal: {:#?}",
                        view.turn,
                        view.phase,
                        view.legal_actions
                    );
                    if queued_for != Some(step) {
                        queue = steps[step].solution.iter().cloned().collect();
                        queued_for = Some(step);
                    }
                    let Some(action) = queue.pop_front() else {
                        panic!(
                            "lesson {id} step {step}: the solution ran out without advancing the step at turn {} ({:?}); allowed: {:#?}",
                            view.turn, view.phase, allowed
                        );
                    };
                    assert!(
                        allowed.contains(&action),
                        "lesson {id} step {step}: solution action {action:?} is not allowed at turn {} ({:?}); allowed: {:#?}; legal: {:#?}",
                        view.turn,
                        view.phase,
                        allowed,
                        view.legal_actions
                    );
                    session.submit(action.clone()).unwrap_or_else(|e| panic!("lesson {id} step {step}: {action:?} rejected: {e}"));
                }
                LessonStep::Complete { .. } => {
                    assert_eq!(session.progress().step_index(), steps.len(), "lesson {id}: complete with steps unvisited");
                    break;
                }
                LessonStep::Ended { winner, reason } => {
                    panic!("lesson {id}: the match ended ({winner:?}, {reason:?}) at step {} of {}", session.progress().step_index(), steps.len())
                }
                LessonStep::Stalled(reason) => panic!("lesson {id}: stalled ({reason:?}) at step {}", session.progress().step_index()),
            }
        }
    }
}
