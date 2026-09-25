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

use netrunner_client::actions::card_title;
use netrunner_client::board::action_map::server_name;
use netrunner_client::board::{ActionMap, Control, Target};
use netrunner_client::play::Coaching;
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{PlayerAction, ServerId, Side};
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

/// The most ways the coach names. A step that offers more than this
/// many ("spend your clicks however you like") is not asking for one
/// move, so the coach names none rather than a list.
pub const MOST_WAYS: usize = 3;

/// One way the board offers a step's move: the gesture that makes it.
/// Structured rather than a sentence so a test can *make* the gesture
/// through the board's own intents and check it submits the move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Way {
    /// A control-bar button.
    Control(Control),
    /// A button under the prompt or in the pop-up: entry `0`.
    Decision(usize),
    /// A hand card carried to one of the places lit for it.
    Drag { card: CardId, places: Vec<Target> },
    /// A card or a zone clicked, and entry `entry` chosen on its menu.
    Menu { target: Target, entry: usize },
}

/// The ways the step's moves are made, off `map` — **the lesson's
/// narrowed map, the one the board is built from**, so a way can only
/// point at something the board is offering. One per control, decision,
/// menu entry, and one per hand card however many places it has. Empty
/// when there are more than [`MOST_WAYS`].
///
/// Here rather than in the lesson files because it is the board's to
/// say: the terminal reaches the same move from a list, and the lesson's
/// words are shared by both clients (§6c).
pub fn ways(map: &ActionMap, side: Side) -> Vec<Way> {
    let decisions = map.decisions();
    let mut ways: Vec<Way> = Vec::new();
    for (index, entry) in map.entries.iter().enumerate() {
        let way = if let Some(control) = Control::for_side(side).iter().find(|control| control.matches(&entry.action)) {
            Way::Control(*control)
        } else if decisions.contains(&index) {
            Way::Decision(index)
        } else {
            let card = entry.targets.iter().find_map(|target| match target {
                Target::HandCard(card) => Some(card.clone()),
                _ => None,
            });
            let places: Vec<Target> = entry.targets.iter().filter(|target| !matches!(target, Target::HandCard(_))).cloned().collect();
            match (card, entry.targets.first()) {
                (Some(card), _) if !places.is_empty() => {
                    if let Some(Way::Drag { places: known, .. }) = ways.iter_mut().find(|way| matches!(way, Way::Drag { card: held, .. } if *held == card)) {
                        known.extend(places.into_iter().filter(|place| !known.contains(place)).collect::<Vec<_>>());
                        continue;
                    }
                    Way::Drag { card, places }
                }
                (Some(card), _) => Way::Menu { target: Target::HandCard(card), entry: index },
                (None, Some(target)) => Way::Menu { target: target.clone(), entry: index },
                // Nothing to click and no control: under the prompt.
                (None, None) => Way::Decision(index),
            }
        };
        if !ways.contains(&way) {
            ways.push(way);
        }
    }
    if ways.len() > MOST_WAYS { Vec::new() } else { ways }
}

impl Way {
    /// Whether this way submits `action`.
    pub fn reaches(&self, map: &ActionMap, action: &PlayerAction) -> bool {
        match self {
            Way::Control(control) => control.matches(action),
            Way::Decision(index) | Way::Menu { entry: index, .. } => map.entries.get(*index).is_some_and(|entry| entry.action == *action),
            Way::Drag { card, places } => places.iter().any(|place| map.for_hand_card_at(card, place).iter().any(|index| map.entries[*index].action == *action)),
        }
    }

    /// The coach's sentence for it, quoting the button's own words
    /// (`ActionEntry::label`, `ActionMap::continue_label`).
    pub fn words(&self, map: &ActionMap, view: &ClientView, registry: &CardRegistry) -> String {
        let quoted = |label: &str| format!("\u{201c}{label}\u{201d}");
        let label = |index: &usize| map.entries.get(*index).map_or_else(String::new, |entry| quoted(&entry.label));
        let installed = |id: &netrunner_core::rules::InstallId| netrunner_client::actions::installed_card_id(view, id).map(|card| card_title(&card, registry));
        match self {
            Way::Control(Control::Continue) => format!("Press {} above your hand.", quoted(map.continue_label())),
            Way::Control(control) => format!("Press {} above your hand.", quoted(control.label())),
            Way::Decision(index) => format!("Press {} in the middle of the window.", label(index)),
            Way::Drag { card, places } => {
                let place = |target: &Target| match target {
                    Target::Server(server) if is_new_remote(*server, view) => "the new column that opens beside your servers as you lift it".to_string(),
                    Target::Server(server) => server_name(*server),
                    Target::Rig => "your rig".to_string(),
                    Target::Table => "the table".to_string(),
                    Target::Install(id) => installed(id).unwrap_or_else(|| "the ice it goes on".to_string()),
                    _ => "the place that lights up".to_string(),
                };
                // Past two, the places are named by what the board does
                // with them: every one lights up while the card is held.
                let onto = match places.as_slice() {
                    [one] => place(one),
                    [one, other] => format!("{} or {}", place(one), place(other)),
                    _ => "any place that lights up as you lift it".to_string(),
                };
                format!("Drag {} from your hand onto {onto}.", card_title(card, registry))
            }
            Way::Menu { target, entry } => {
                let what = match target {
                    Target::HandCard(card) => format!("{} in your hand", card_title(card, registry)),
                    Target::Install(id) => installed(id).unwrap_or_else(|| "the card".to_string()),
                    Target::Server(server) => server_name(*server),
                    Target::Identity(_) => "your identity".to_string(),
                    Target::Pile(pile) => format!("your {}", pile.name().to_lowercase()),
                    Target::Position(_) | Target::Rig | Target::Table => "it".to_string(),
                };
                format!("Click {what} and choose {}.", label(entry))
            }
        }
    }
}

/// A remote the engine offers that the table does not have yet: the
/// board draws its column only while a card that could go there is held.
fn is_new_remote(server: ServerId, view: &ClientView) -> bool {
    matches!(server, ServerId::Remote(_)) && !view.corp.servers.iter().any(|existing| existing.server == server)
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

    /// Makes `way`'s gesture on the board, for `action`: the intents a
    /// person's hands would send, ending in what the board submits.
    fn make(game: &mut Game, way: &Way, action: &PlayerAction) -> Outcome {
        match way {
            Way::Control(control) => game.apply(Intent::Control(*control)),
            Way::Decision(index) => game.apply(Intent::Choose(*index)),
            Way::Menu { target, entry } => {
                assert_eq!(game.apply(Intent::Click { target: target.clone(), over: Anchor::default() }), Outcome::Redraw, "the click opens a menu");
                assert!(game.menu.as_ref().is_some_and(|menu| menu.entries.contains(entry)), "the menu holds the entry the coach quotes");
                game.apply(Intent::Choose(*entry))
            }
            Way::Drag { card, places } => {
                let slot = game.hand.cards().iter().position(|held| held == card).expect("the card to drag is in the hand");
                let place = places.iter().find(|place| game.actions.for_hand_card_at(card, place).iter().any(|i| game.actions.entries[*i].action == *action)).expect("a place takes it").clone();
                game.apply(Intent::DragPress { slot, at: (100.0, 900.0) });
                game.apply(Intent::DragMove { at: (400.0, 400.0) });
                assert!(game.drop_places().contains(&place), "the board lights the place the coach names");
                match game.apply(Intent::DragDrop { target: place, over: Anchor::default() }) {
                    Outcome::Redraw => {
                        let entry = game.menu.as_ref().and_then(|menu| menu.entries.iter().copied().find(|i| game.actions.entries[*i].action == *action)).expect("the drop's menu offers it");
                        game.apply(Intent::Choose(entry))
                    }
                    outcome => outcome,
                }
            }
        }
    }

    /// Every step of every lesson is played on the board the way the
    /// coach says: the gesture a way names is made through the board's
    /// intents and submits the step's move. A step the coach names no way
    /// for is one offering more than `MOST_WAYS`, and there are few.
    #[test]
    fn every_lesson_is_played_the_way_the_coach_says() {
        let registry = Arc::new(netrunner_client::decks::sample_deck_registry());
        let (mut named, mut unnamed) = (0, 0);
        for lesson in tutorial::embedded_lessons() {
            let (id, steps) = (lesson.id.clone(), lesson.steps.clone());
            let mut game = Game::lesson(Arc::clone(&registry), lesson.side, LessonBoard::new(lesson.title.clone(), lesson.intro.clone()));
            game.apply(Intent::BeginLesson);
            let mut handle = MatchHandle::start_lesson(Arc::clone(&registry), lesson, 0).unwrap();
            loop {
                let message = handle.wait().unwrap_or_else(|| panic!("{id}: the thread went silent"));
                let complete = matches!(message, MatchMessage::LessonComplete { .. });
                game.apply(Intent::Message(MatchMessageRef(message)));
                if complete {
                    break;
                }
                if !game.awaiting {
                    continue;
                }
                let board = game.lesson.clone().unwrap();
                let coaching = board.coaching.clone().unwrap();
                let action = steps[coaching.step - 1].solution.iter().find(|a| coaching.allowed.contains(a)).cloned().unwrap_or_else(|| panic!("{id} step {}: no solution allowed", coaching.step));
                let found = if board.gated() { ways(&game.actions, game.side) } else { Vec::new() };
                let view = game.view.clone().unwrap();
                for way in &found {
                    eprintln!("{id} step {}: {}", coaching.step, way.words(&game.actions, &view, game.registry()));
                }
                let outcome = match found.iter().find(|way| way.reaches(&game.actions, &action)) {
                    Some(way) => {
                        named += 1;
                        make(&mut game, way, &action)
                    }
                    None => {
                        assert!(found.is_empty(), "{id} step {}: the coach names ways, none of them the step's move", coaching.step);
                        unnamed += 1;
                        let index = game.actions.entries.iter().position(|entry| entry.action == action).unwrap();
                        game.apply(Intent::Choose(index))
                    }
                };
                assert_eq!(outcome, Outcome::Submit(action.clone()), "{id} step {}", coaching.step);
                handle.submit(action).unwrap();
            }
        }
        eprintln!("{named} decisions named a way, {unnamed} did not");
        assert!(named > 4 * unnamed, "the coach names a way for most decisions: {named} against {unnamed}");
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
