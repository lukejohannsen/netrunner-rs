//! Scripted lessons — ROADMAP Phase 1.75 §5 — and the lesson files embedded
//! at compile time.
//!
//! A [`Lesson`] is `{ stacked deck order, scripted opening, ordered steps }`:
//! the decks it is played with, a top-first prefix pinning the cards each
//! side draws, the learner's canned actions that fast-forward to the
//! position being taught, the opponent's script, and the steps themselves.
//! A [`Step`] is `{ prose, allow, advance_when }` plus the intended
//! `solution`, which exists so a test can prove every lesson completable.
//!
//! Lessons live in `data/lessons/{corp,runner}/*.json`, embedded by
//! `build.rs` exactly like cards and decks and parsed with
//! `deny_unknown_fields`, so a misspelt key is a parse error rather than a
//! silently ignored field. Never hardcoded in Rust — the same house rule as
//! cards, for the same reason: an author edits JSON, and the engine grows
//! no per-lesson code.
//!
//! Homed in `netrunner_core` because it depends only on `GameEvent`,
//! `PlayerAction` and `ClientView`, and must be reachable by any client, not
//! just the TUI — the reasoning that already puts `decks` here. It adds no
//! dependencies. What it deliberately does *not* contain is the loop that
//! plays a lesson: that is `netrunner_session::lesson::LessonSession`,
//! because there is exactly one match loop and this crate does not own it.
//!
//! **A lesson step narrows `view.legal_actions`; it never widens them.**
//! [`LessonProgress::allowed`] is a filter over the legal list and nothing
//! in this module can construct an action, so presenting its result is a
//! UI affordance like sorting: it cannot make an illegal action legal and
//! does not violate AGENTS.md §3's ban on a client re-deriving legality.

pub mod predicate;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use predicate::{ActionPredicate, EventPredicate};

use crate::cards::CardRegistry;
use crate::decks::{self, DeckFile};
use crate::dsl::CardId;
use crate::rules::{DeckOrder, GameEvent, GameState, MatchRules, PlayerAction, RulesError, Side};
use crate::view::ClientView;

const CORP_LESSONS_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/corp_lessons.json"));
const RUNNER_LESSONS_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/runner_lessons.json"));

/// A top-first *prefix* of each draw deck: `corp[0]` is the first card the
/// Corp draws, exactly `DeckOrder::Fixed`'s convention.
///
/// A prefix rather than a full permutation, deliberately. `DeckOrder::
/// Fixed` demands the whole deck so the decklist stays authoritative, but
/// a lesson only cares about the opening hand and the next few draws;
/// making an author list all 34 cards to pin six was the chore the ROADMAP
/// worried about. [`Lesson::deck_order`] completes the prefix with the
/// remaining copies in decklist order, so the engine still receives — and
/// validates — a permutation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StackedOrder {
    #[serde(default)]
    pub corp: Vec<CardId>,
    #[serde(default)]
    pub runner: Vec<CardId>,
}

/// One canned action for the opponent's seat: played the first time it is
/// legal, or — when `turn` is set — the first time it is legal on that
/// turn. The `turn` guard is what lets a lesson say "the Runner runs your
/// remote on turn 2" rather than on the first turn a run is possible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptedAction {
    pub action: PlayerAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<u32>,
}

/// One concept: what to tell the learner, which of their legal actions to
/// offer, and which observed event means they have done it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// The coaching text shown while this step is live.
    pub prose: String,
    /// A shorter nudge, shown alongside the prose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Which legal actions to present — a filter, never a source.
    pub allow: ActionPredicate,
    /// The event that completes the step.
    pub advance_when: EventPredicate,
    /// The play the author intends: candidate actions in order of
    /// preference, of which the first one the step allows is played at
    /// each prompt until the step advances. A preference list rather than
    /// a fixed sequence because the prompts between two intended plays —
    /// priority windows, "continue run" beats — vary in number, and an
    /// author should not have to count them. `LessonSession` never reads
    /// it; the lesson gate does, to prove the step is completable through
    /// its own `allow` and that `advance_when` then fires.
    pub solution: Vec<PlayerAction>,
}

/// One lesson, as authored. See the module doc for the shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lesson {
    pub id: String,
    /// The side the learner plays.
    pub side: Side,
    pub title: String,
    /// Shown before the first step.
    pub intro: String,
    /// Shown after the last step advances.
    pub outro: String,
    /// Embedded deck ids (`decks::by_id`).
    pub corp_deck: String,
    pub runner_deck: String,
    #[serde(default)]
    pub order: StackedOrder,
    /// The learner's canned actions, submitted before the first step is
    /// shown. Together with the opponent's script this fast-forwards to the
    /// position being taught.
    #[serde(default)]
    pub opening: Vec<PlayerAction>,
    /// The opponent's canned actions; once exhausted (or never legal) the
    /// opponent plays a passive fallback — see `netrunner_bots::
    /// ScriptedAgent`.
    #[serde(default)]
    pub opponent: Vec<ScriptedAction>,
    pub steps: Vec<Step>,
}

/// Why a lesson could not be set up or is mis-authored.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TutorialError {
    #[error("lesson names deck {0:?}, which is not an embedded deck")]
    UnknownDeck(String),

    #[error("lesson names {deck:?} as its {expected:?} deck, but that deck is for the other side")]
    DeckOnWrongSide { deck: String, expected: Side },

    #[error("the stacked order for {side:?} names {card:?}, which that deck has no (further) copy of")]
    OrderNotInDeck { side: Side, card: CardId },

    #[error("lesson has no steps")]
    NoSteps,

    #[error("step {step} has an empty solution, so nothing can prove it completable")]
    EmptySolution { step: usize },

    #[error("step {step} allows action kind {kind:?}, which is not a PlayerAction variant")]
    UnknownActionKind { step: usize, kind: String },

    #[error("{0}")]
    Rules(#[from] RulesError),
}

impl Lesson {
    /// Parses one lesson file's JSON text. Pure — the file is the caller's.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// The `(corp, runner)` decks this lesson is played with.
    pub fn decks(&self) -> Result<(DeckFile, DeckFile), TutorialError> {
        let corp = decks::by_id(&self.corp_deck).ok_or_else(|| TutorialError::UnknownDeck(self.corp_deck.clone()))?;
        let runner = decks::by_id(&self.runner_deck).ok_or_else(|| TutorialError::UnknownDeck(self.runner_deck.clone()))?;
        if corp.side != Side::Corp {
            return Err(TutorialError::DeckOnWrongSide { deck: self.corp_deck.clone(), expected: Side::Corp });
        }
        if runner.side != Side::Runner {
            return Err(TutorialError::DeckOnWrongSide { deck: self.runner_deck.clone(), expected: Side::Runner });
        }
        Ok((corp, runner))
    }

    /// The rules the lesson is played under: the Corp deck's category
    /// decides (a starter deck plays to 6), the same way the starter game
    /// itself does.
    pub fn match_rules(&self) -> Result<MatchRules, TutorialError> {
        Ok(self.decks()?.0.category.match_rules())
    }

    /// The full `DeckOrder::Fixed` this lesson's prefix expands to.
    pub fn deck_order(&self, corp: &DeckFile, runner: &DeckFile) -> Result<DeckOrder, TutorialError> {
        Ok(DeckOrder::Fixed {
            corp: complete_order(&self.order.corp, corp, Side::Corp)?,
            runner: complete_order(&self.order.runner, runner, Side::Runner)?,
        })
    }

    /// The opening position: both decks in the stacked order, under the
    /// lesson's rules. `seed` only matters for a mulligan reshuffle.
    pub fn setup(&self, registry: &CardRegistry, seed: u64) -> Result<(GameState, Vec<GameEvent>), TutorialError> {
        let (corp, runner) = self.decks()?;
        let order = self.deck_order(&corp, &runner)?;
        let rules = corp.category.match_rules();
        Ok(GameState::setup_with(&corp.to_deck(), &runner.to_deck(), registry, seed, rules, order)?)
    }

    /// Everything that can be checked without playing the lesson. What
    /// cannot — that the opening is legal, that every step's solution is
    /// allowed and advances it — is the lesson gate's job.
    pub fn validate(&self, registry: &CardRegistry) -> Result<(), TutorialError> {
        self.setup(registry, 0)?;
        if self.steps.is_empty() {
            return Err(TutorialError::NoSteps);
        }
        for (index, step) in self.steps.iter().enumerate() {
            if step.solution.is_empty() {
                return Err(TutorialError::EmptySolution { step: index });
            }
            if let Some(kind) = step.allow.kinds().into_iter().find(|kind| !PlayerAction::VARIANT_NAMES.contains(kind)) {
                return Err(TutorialError::UnknownActionKind { step: index, kind: kind.to_string() });
            }
        }
        Ok(())
    }
}

/// `prefix` followed by every remaining copy of the decklist, in decklist
/// order. Each prefix card consumes one copy; a card the deck has no
/// further copy of is an authoring error.
fn complete_order(prefix: &[CardId], deck: &DeckFile, side: Side) -> Result<Vec<CardId>, TutorialError> {
    let mut remaining: Vec<CardId> =
        deck.cards.iter().flat_map(|entry| std::iter::repeat_n(entry.card.clone(), entry.count as usize)).collect();
    let mut order = Vec::with_capacity(remaining.len());
    for card in prefix {
        let position = remaining
            .iter()
            .position(|candidate| candidate == card)
            .ok_or_else(|| TutorialError::OrderNotInDeck { side, card: card.clone() })?;
        order.push(remaining.remove(position));
    }
    order.extend(remaining);
    Ok(order)
}

/// Where a learner is in a lesson: the pure state machine, shared by the
/// TUI and the lesson gate so they cannot disagree about what a step
/// allows or when it advances.
#[derive(Debug, Clone)]
pub struct LessonProgress {
    lesson: Lesson,
    step: usize,
}

impl LessonProgress {
    pub fn new(lesson: Lesson) -> Self {
        Self { lesson, step: 0 }
    }

    pub fn lesson(&self) -> &Lesson {
        &self.lesson
    }

    /// Zero-based index of the live step; equals `total` once complete.
    pub fn step_index(&self) -> usize {
        self.step
    }

    pub fn total(&self) -> usize {
        self.lesson.steps.len()
    }

    pub fn current(&self) -> Option<&Step> {
        self.lesson.steps.get(self.step)
    }

    pub fn is_complete(&self) -> bool {
        self.step >= self.lesson.steps.len()
    }

    /// Feeds one action's events through, in order; each may advance the
    /// live step, so a single action can complete two consecutive steps
    /// ("play Hedge Fund", then "notice the credits arrive"). Returns
    /// whether any step advanced.
    pub fn observe(&mut self, events: &[GameEvent]) -> bool {
        let before = self.step;
        for event in events {
            if self.current().is_some_and(|step| step.advance_when.matches(event)) {
                self.step += 1;
            }
        }
        self.step != before
    }

    /// The subset of `view.legal_actions` the live step lets through — a
    /// filter and nothing else. Once the lesson is complete, everything.
    /// May be empty, which is the client's cue to fall back to the full
    /// list (the escape hatch ROADMAP §6 requires) rather than strand the
    /// learner with no action at all.
    pub fn allowed<'a>(&self, view: &'a ClientView) -> Vec<&'a PlayerAction> {
        match self.current() {
            Some(step) => view.legal_actions.iter().filter(|action| step.allow.matches(action, view)).collect(),
            None => view.legal_actions.iter().collect(),
        }
    }
}

/// Parses one side's embedded lessons. A failure is an authoring bug that
/// got past the test suite, so it panics — the same reasoning as
/// `decks::parse`.
fn parse(json: &str) -> Vec<Lesson> {
    serde_json::from_str(json).unwrap_or_else(|e| panic!("embedded lesson data failed to parse: {e}"))
}

/// Every lesson compiled into the binary: the Corp track, then the Runner
/// track, each in file-name order — which is the track order.
pub fn embedded_lessons() -> Vec<Lesson> {
    let mut lessons = parse(CORP_LESSONS_JSON);
    lessons.extend(parse(RUNNER_LESSONS_JSON));
    lessons
}

/// One side's track, in order.
pub fn track(side: Side) -> Vec<Lesson> {
    match side {
        Side::Corp => parse(CORP_LESSONS_JSON),
        Side::Runner => parse(RUNNER_LESSONS_JSON),
    }
}

/// The embedded lesson with this id, if one exists.
pub fn by_id(id: &str) -> Option<Lesson> {
    embedded_lessons().into_iter().find(|lesson| lesson.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::register_playable_cards;
    use crate::rules::{GamePhase, ServerId};
    use std::collections::HashSet;

    fn registry() -> CardRegistry {
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        registry
    }

    fn id(s: &str) -> CardId {
        CardId(s.to_string())
    }

    fn lesson_with(order: StackedOrder, steps: Vec<Step>) -> Lesson {
        Lesson {
            id: "test".to_string(),
            side: Side::Corp,
            title: "Test".to_string(),
            intro: String::new(),
            outro: String::new(),
            corp_deck: "the_syndicate_starter".to_string(),
            runner_deck: "the_catalyst_starter".to_string(),
            order,
            opening: Vec::new(),
            opponent: Vec::new(),
            steps,
        }
    }

    fn step(allow: ActionPredicate, advance_when: EventPredicate) -> Step {
        Step { prose: "do it".to_string(), hint: None, allow, advance_when, solution: vec![PlayerAction::EndTurn] }
    }

    #[test]
    fn every_embedded_lesson_parses_and_validates() {
        let registry = registry();
        let lessons = embedded_lessons();
        assert!(!lessons.is_empty(), "no lessons are embedded");
        for lesson in &lessons {
            lesson.validate(&registry).unwrap_or_else(|e| panic!("lesson {}: {e}", lesson.id));
            assert!(track(lesson.side).iter().any(|l| l.id == lesson.id), "lesson {} is filed under the wrong side", lesson.id);
        }
    }

    #[test]
    fn lesson_ids_are_unique() {
        let mut seen = HashSet::new();
        for lesson in embedded_lessons() {
            assert!(seen.insert(lesson.id.clone()), "duplicate lesson id {}", lesson.id);
            assert_eq!(by_id(&lesson.id).map(|l| l.title), Some(lesson.title));
        }
    }

    #[test]
    fn a_misspelled_lesson_key_is_rejected() {
        let json = r#"{"id":"x","side":"Corp","title":"t","intro":"","outro":"","corp_deck":"a","runner_deck":"b","stpes":[]}"#;
        let error = Lesson::from_json(json).unwrap_err().to_string();
        assert!(error.contains("stpes"), "the error should name the offending key: {error}");
    }

    #[test]
    fn a_prefix_order_completes_to_a_permutation_of_the_deck() {
        let registry = registry();
        let order = StackedOrder { corp: vec![id("hedge_fund"), id("palisade"), id("hedge_fund")], runner: vec![id("cleaver")] };
        let lesson = lesson_with(order, vec![step(ActionPredicate::Any, EventPredicate::Any)]);
        let (corp, runner) = lesson.decks().unwrap();
        let DeckOrder::Fixed { corp: corp_order, runner: runner_order } = lesson.deck_order(&corp, &runner).unwrap() else {
            panic!("a lesson always fixes its order");
        };
        assert_eq!(corp_order.len() as u32, corp.size());
        assert_eq!(&corp_order[..3], &[id("hedge_fund"), id("palisade"), id("hedge_fund")]);
        assert_eq!(corp_order.iter().filter(|c| **c == id("hedge_fund")).count(), 3, "the prefix consumed copies, not added them");
        assert_eq!(runner_order[0], id("cleaver"));
        assert_eq!(runner_order.len() as u32, runner.size());

        let (state, _) = lesson.setup(&registry, 0).unwrap();
        assert_eq!(state.phase, GamePhase::Mulligan(Side::Corp));
        assert_eq!(state.corp.hq[0], id("hedge_fund"), "the first prefix card is the first drawn");
        assert_eq!(state.rules.winning_agenda_points, 6, "starter decks play to six");

        let too_many = lesson_with(StackedOrder { corp: vec![id("hedge_fund"); 4], runner: vec![] }, vec![]);
        assert_eq!(
            too_many.deck_order(&corp, &runner),
            Err(TutorialError::OrderNotInDeck { side: Side::Corp, card: id("hedge_fund") }),
            "a fourth Hedge Fund is one the deck does not have"
        );
    }

    #[test]
    fn validate_rejects_the_authoring_mistakes_it_can_see() {
        let registry = registry();
        assert_eq!(lesson_with(StackedOrder::default(), vec![]).validate(&registry), Err(TutorialError::NoSteps));
        let mut empty = step(ActionPredicate::Any, EventPredicate::Any);
        empty.solution.clear();
        assert_eq!(
            lesson_with(StackedOrder::default(), vec![empty]).validate(&registry),
            Err(TutorialError::EmptySolution { step: 0 })
        );
        let typo = step(ActionPredicate::Kind("GainCredit".to_string()), EventPredicate::Any);
        assert_eq!(
            lesson_with(StackedOrder::default(), vec![typo]).validate(&registry),
            Err(TutorialError::UnknownActionKind { step: 0, kind: "GainCredit".to_string() })
        );
        let mut wrong = lesson_with(StackedOrder::default(), vec![step(ActionPredicate::Any, EventPredicate::Any)]);
        wrong.corp_deck = "the_catalyst_starter".to_string();
        assert_eq!(
            wrong.validate(&registry),
            Err(TutorialError::DeckOnWrongSide { deck: "the_catalyst_starter".to_string(), expected: Side::Corp })
        );
    }

    #[test]
    fn progress_advances_only_on_the_current_steps_event() {
        let lesson = lesson_with(
            StackedOrder::default(),
            vec![
                step(ActionPredicate::Kind("PlayOperation".to_string()), EventPredicate::Kind("OperationPlayed".to_string())),
                step(ActionPredicate::Any, EventPredicate::Kind("CreditsGained".to_string())),
                step(ActionPredicate::Kind("EndTurn".to_string()), EventPredicate::Side { kind: "TurnEnded".to_string(), side: Side::Corp }),
            ],
        );
        let mut progress = LessonProgress::new(lesson);
        assert_eq!(progress.step_index(), 0);
        assert_eq!(progress.total(), 3);
        // A credit gain before the operation is played does not skip ahead.
        assert!(!progress.observe(&[GameEvent::CreditsGained { side: Side::Corp, amount: 1 }]));
        assert_eq!(progress.step_index(), 0);
        // One action's events can complete two consecutive steps, in order.
        let played = vec![
            GameEvent::OperationPlayed { side: Side::Corp, card: id("hedge_fund"), from_archives: false },
            GameEvent::CreditsGained { side: Side::Corp, amount: 9 },
        ];
        assert!(progress.observe(&played));
        assert_eq!(progress.step_index(), 2);
        assert!(!progress.observe(&[GameEvent::TurnEnded { side: Side::Runner }]), "the other side's turn end is not ours");
        assert!(progress.observe(&[GameEvent::TurnEnded { side: Side::Corp }]));
        assert!(progress.is_complete());
        assert!(progress.current().is_none());
    }

    #[test]
    fn allowed_is_a_filter_over_the_legal_actions() {
        let registry = registry();
        let lesson = lesson_with(
            StackedOrder::default(),
            vec![
                step(ActionPredicate::Kind("KeepHand".to_string()), EventPredicate::Kind("HandKept".to_string())),
                step(ActionPredicate::Server { kind: Some("InitiateRun".to_string()), server: ServerId::Hq }, EventPredicate::Any),
            ],
        );
        let (state, _) = lesson.setup(&registry, 0).unwrap();
        let view = crate::view::build_client_view(&state, &registry, Side::Corp);
        let mut progress = LessonProgress::new(lesson);
        let allowed = progress.allowed(&view);
        assert_eq!(allowed, vec![&PlayerAction::KeepHand]);
        assert!(view.legal_actions.contains(&PlayerAction::TakeMulligan), "the filter hid a legal action; it did not remove it");
        progress.observe(&[GameEvent::HandKept { side: Side::Corp }]);
        assert!(progress.allowed(&view).is_empty(), "a Corp cannot run: the step matches nothing, and the client must fall back");
        progress.observe(&[GameEvent::HandKept { side: Side::Corp }]);
        assert_eq!(progress.allowed(&view).len(), view.legal_actions.len(), "a complete lesson filters nothing");
    }
}
