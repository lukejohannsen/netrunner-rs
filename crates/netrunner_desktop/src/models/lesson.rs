//! A lesson on the board (Phase 7 §6): its opening words, the coaching
//! for the decision in front of the person, and its closing words.
//!
//! **A lesson narrows the list the board is built from, and nothing
//! else.** The step's `allowed` (`netrunner_client::play::Coaching`) is a
//! filter over the view's `legal_actions`, and [`LessonBoard::offered`]
//! hands the board a copy of the view holding only those — so the control
//! bar greys, the menus shorten, the glows go out and the keys and drags
//! stop at exactly the actions the step is about, with no surface of the
//! board knowing a lesson is on. The alternative was a check at every
//! surface, and one forgotten would have been an action the lesson hides
//! on one door and offers on another. The board itself is drawn from the
//! whole view: a lesson shows everything and offers a little.
//!
//! **Every legal action stays one press away** (`every_action`), the
//! terminal's `a`, because a step that has narrowed the list to nothing
//! the person wants must never strand them; and a step whose filter
//! matches nothing on this view narrows nothing and says so (Phase 1.75
//! §6: the gate can narrow the list, never empty it).

use std::borrow::Cow;

use netrunner_client::play::Coaching;
use netrunner_core::view::ClientView;

#[derive(Debug, Clone, PartialEq)]
pub struct LessonBoard {
    pub title: String,
    /// The lesson's opening words, until the person begins. The board is
    /// covered while they are up, as it is by any question.
    pub intro: Option<String>,
    /// The coaching for the decision the board is on, or the last one
    /// while the opponent plays.
    pub coaching: Option<Coaching>,
    /// The escape hatch is open: every legal action is offered.
    pub every_action: bool,
    /// The lesson's closing words, once every step has advanced.
    pub outro: Option<String>,
    /// Its track goes on after it, so the closing words offer the next.
    pub has_next: bool,
}

impl LessonBoard {
    pub fn new(title: impl Into<String>, intro: impl Into<String>) -> Self {
        LessonBoard { title: title.into(), intro: Some(intro.into()), coaching: None, every_action: false, outro: None, has_next: false }
    }

    /// Whether this step's filter matched anything on the view it came
    /// with. When it did not, every legal action is offered and the panel
    /// says why.
    pub fn gated(&self) -> bool {
        self.coaching.as_ref().is_some_and(|coaching| !coaching.allowed.is_empty())
    }

    /// The view the board's actions are built from: `view` itself when
    /// nothing narrows it, otherwise a copy offering only the step's
    /// actions — in the engine's own order, so a key that names the Nth
    /// entry names the same one either way.
    pub fn offered<'a>(&self, view: &'a ClientView) -> Cow<'a, ClientView> {
        let Some(coaching) = self.coaching.as_ref().filter(|_| self.gated() && !self.every_action) else { return Cow::Borrowed(view) };
        let mut narrowed = view.clone();
        narrowed.legal_actions.retain(|action| coaching.allowed.contains(action));
        Cow::Owned(narrowed)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

        use netrunner_client::board::Target;
    use netrunner_client::play::{MatchHandle, MatchMessage};
    use netrunner_core::rules::PlayerAction;
    use netrunner_core::tutorial;

    use super::*;
    use crate::models::game::{Anchor, Game, Intent, MatchMessageRef, Outcome};

    /// The first lesson whose first step narrows the list, on a board
    /// fed by the real lesson thread up to that step's decision.
    fn at_a_narrowed_step() -> (Game, MatchHandle, usize) {
        let registry = Arc::new(netrunner_client::decks::sample_deck_registry());
        for lesson in tutorial::embedded_lessons() {
            let board = LessonBoard::new(lesson.title.clone(), lesson.intro.clone());
            let mut game = Game::lesson(Arc::clone(&registry), lesson.side, board);
            let mut handle = MatchHandle::start_lesson(Arc::clone(&registry), lesson, 0).unwrap();
            while !game.awaiting {
                let message = handle.wait().expect("the lesson is alive");
                assert!(!matches!(message, MatchMessage::LessonComplete { .. }));
                let outcome = game.apply(Intent::Message(MatchMessageRef(message)));
                assert!(!matches!(outcome, Outcome::Submit(_)), "a lesson board answers nothing for the learner");
            }
            let legal = game.view.as_ref().unwrap().legal_actions.len();
            if game.lesson.as_ref().unwrap().gated() && game.actions.entries.len() < legal {
                return (game, handle, legal);
            }
        }
        panic!("no lesson narrows its first step");
    }

    #[test]
    fn a_step_narrows_the_board_and_the_escape_hatch_widens_it_again() {
        let (mut game, _handle, legal) = at_a_narrowed_step();
        let allowed = game.lesson.as_ref().unwrap().coaching.as_ref().unwrap().allowed.clone();
        assert_eq!(game.actions.entries.iter().map(|entry| entry.action.clone()).collect::<Vec<_>>(), allowed, "the step's actions, in the engine's order");
        // The board still shows the whole view: only the offer narrows.
        assert_eq!(game.view.as_ref().unwrap().legal_actions.len(), legal);
        assert_eq!(game.apply(Intent::EveryAction), Outcome::Redraw);
        assert_eq!(game.actions.entries.len(), legal);
        assert_eq!(game.apply(Intent::EveryAction), Outcome::Redraw);
        assert_eq!(game.actions.entries.len(), allowed.len());
    }

    #[test]
    fn the_intro_covers_the_board_until_the_lesson_begins() {
        let (mut game, _handle, _) = at_a_narrowed_step();
        assert!(game.intro_open() && game.covered());
        // Neither kind of click reaches the board under the intro.
        let target = Target::Identity(game.side);
        assert_eq!(game.apply(Intent::Click { target: target.clone(), over: Anchor::default() }), Outcome::Nothing);
        assert_eq!(game.apply(Intent::Inspect(target)), Outcome::Nothing);
        assert!(game.menu.is_none() && game.sheet.is_none());
        assert_eq!(game.apply(Intent::BeginLesson), Outcome::Redraw);
        assert!(!game.intro_open() && !game.covered());
        assert_eq!(game.apply(Intent::BeginLesson), Outcome::Nothing);
    }

    /// Escape on the opening words leaves at once, as their Leave does:
    /// nothing has been played. Once begun, it asks first.
    #[test]
    fn escape_leaves_from_the_intro_and_asks_once_begun() {
        let (mut game, _handle, _) = at_a_narrowed_step();
        assert_eq!(game.apply(Intent::Back), Outcome::Quit);
        game.apply(Intent::BeginLesson);
        assert_eq!(game.apply(Intent::Back), Outcome::Redraw);
        assert!(game.confirm_quit);
    }

    /// The lone pass the board takes for a person in a game is the
    /// lesson's to take or not: a lesson that hands the learner a lone
    /// pass wants it pressed.
    #[test]
    fn a_lesson_board_never_passes_for_the_learner() {
        let (mut game, _handle, _) = at_a_narrowed_step();
        let mut view = game.view.clone().unwrap();
        view.legal_actions = vec![PlayerAction::PassPriority { side: game.side }];
        game.awaiting = false;
        assert_eq!(game.apply(Intent::Message(MatchMessageRef(MatchMessage::Awaiting { view: Box::new(view) }))), Outcome::Redraw);
        assert!(game.awaiting);
        assert_eq!(game.run_pass_label(), None);
        assert!(game.breaks.is_empty());
    }
}
