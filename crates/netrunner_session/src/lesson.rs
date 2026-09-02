//! The lesson driver — ROADMAP Phase 1.75 §5/§6 — pumping one `Session`
//! with the learner as `Seat::External` and the opponent as a
//! `netrunner_bots::ScriptedAgent`.
//!
//! This is a driver over the one match loop, not a second loop: every
//! action still goes through `Session::step`/`submit`, and the lesson
//! machinery only decides *when to hand the learner a decision* and *which
//! legal actions to show them* (`netrunner_core::tutorial::LessonProgress`).
//! It lives here rather than in `netrunner_cli` because the lesson gates —
//! every lesson completable, every gated step offers an action — need to
//! drive it from a test, and the CLI crate has no library target.
//!
//! Events reach the lesson through the history-diff idiom `Session::run`
//! documents (`entries()[mark..]`), which replaced the observer callback the
//! ROADMAP originally named; `HistoryEntry::events` is the same
//! `Vec<GameEvent>` `apply_action` produced.

use std::collections::VecDeque;

use netrunner_bots::ScriptedAgent;
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{PlayerAction, Side};
use netrunner_core::tutorial::{Lesson, LessonProgress, TutorialError};
use netrunner_core::view::ClientView;

use crate::outcome::GameEndReason;
use crate::history::PublicHistoryEntry;
use crate::session::{Seat, Session, SessionStep, StallReason, SubmitError};

/// Why a lesson could not start or run — every case an authoring bug.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LessonError {
    #[error("{0}")]
    Tutorial(#[from] TutorialError),
    /// A canned opening action the engine refused. The lesson gate turns
    /// this into a named failure before it can reach a player.
    #[error("opening action {index} ({action:?}) was rejected: {error}")]
    OpeningRejected { index: usize, action: PlayerAction, error: SubmitError },
}

/// What the learner's client should do next.
#[derive(Debug)]
pub enum LessonStep {
    /// The learner has a decision. `allowed` is the live step's filter over
    /// `view.legal_actions`, possibly empty; the full list stays on `view`,
    /// which is the client's escape hatch — a key that reveals every legal
    /// action — so a step that matches nothing can never strand a player.
    Prompt { view: Box<ClientView>, allowed: Vec<PlayerAction>, step: usize, total: usize },
    /// Every step has advanced. Returned as soon as it is true, even
    /// mid-opponent-turn, so the outro follows the deed rather than the
    /// opponent's next three clicks.
    Complete { view: Box<ClientView> },
    /// The match ended before the lesson did.
    Ended { winner: Side, reason: GameEndReason },
    Stalled(StallReason),
}

pub struct LessonSession {
    session: Session,
    progress: LessonProgress,
    opening: VecDeque<PlayerAction>,
    learner: Side,
    /// How much of the history `progress` has seen.
    absorbed: usize,
    /// Counts the opening actions submitted, for `OpeningRejected`.
    opening_index: usize,
    /// The learner's copy of every action resolved since `drain_log`,
    /// each masked against its own post-action state at the moment it
    /// resolved. Kept here rather than diffed by the TUI afterwards because
    /// `step` pumps the opening and the scripted opponent through several
    /// actions before it returns, and `Session::last_entry_for` is only
    /// right for the entry just applied.
    log: Vec<PublicHistoryEntry>,
    /// Set when the learner is first handed a decision. Nothing before that
    /// — the opening's own events, the opponent's turns leading up to it —
    /// counts toward a step: an opening that advances an agenda must not
    /// tick off the "advance the agenda" step it exists to set up.
    handed_over: bool,
}

impl LessonSession {
    /// Sets the lesson up: stacked decks under the lesson's rules, the
    /// learner external, the opponent scripted. `seed` only reaches the
    /// engine's RNG (a mulligan reshuffle, a random access), so the same
    /// lesson replays identically for the same seed.
    pub fn start(lesson: Lesson, registry: CardRegistry, seed: u64) -> Result<Self, LessonError> {
        let (state, _events) = lesson.setup(&registry, seed)?;
        let opponent = Seat::Agent(Box::new(ScriptedAgent::new(lesson.opponent.clone())));
        let learner = lesson.side;
        let (corp, runner) = match learner {
            Side::Corp => (Seat::External, opponent),
            Side::Runner => (opponent, Seat::External),
        };
        Ok(Self {
            session: Session::new(state, registry, corp, runner),
            opening: lesson.opening.iter().cloned().collect(),
            progress: LessonProgress::new(lesson),
            learner,
            absorbed: 0,
            opening_index: 0,
            log: Vec::new(),
            handed_over: false,
        })
    }

    /// Pumps the match until the learner has a decision to make, the
    /// lesson completes, or the match stops.
    ///
    /// Two decisions are made *for* the learner here, both documented
    /// because ROADMAP §6 forbids a lesson deciding anything a player
    /// could: an opening action is submitted on their behalf (that is what
    /// an opening is), and priority is passed for them when either it is
    /// their only legal action (no choice exists) or the live step allows
    /// none of their legal actions but passing is one of them (the author
    /// has said nothing in this window is part of the lesson). Neither
    /// narrows what a player could have chosen within the lesson's own
    /// terms, and without the second a lesson would have to spell out
    /// "pass priority" around every turn start.
    pub fn step(&mut self) -> Result<LessonStep, LessonError> {
        loop {
            let step = self.session.step();
            if matches!(step, SessionStep::Applied { .. }) {
                self.log_last();
            }
            self.absorb();
            if self.progress.is_complete() {
                return Ok(LessonStep::Complete { view: Box::new(self.session.view_for(self.learner)) });
            }
            match step {
                SessionStep::Applied { .. } => continue,
                SessionStep::Awaiting { side, view } => {
                    debug_assert_eq!(side, self.learner, "only the learner's seat is external");
                    let pass = view.legal_actions.iter().find(|action| matches!(action, PlayerAction::PassPriority { .. })).cloned();
                    if let Some(action) = self.opening.front() {
                        // An opening is written as the turn's actions; the
                        // paid-ability windows between them are passed
                        // through, the same rule as below.
                        if !view.legal_actions.contains(action)
                            && let Some(pass) = pass
                        {
                            self.session.submit(pass).expect("passing priority is legal: it was in legal_actions");
                            self.log_last();
                            continue;
                        }
                        let action = self.opening.pop_front().expect("front was Some");
                        let index = self.opening_index;
                        self.opening_index += 1;
                        self.session
                            .submit(action.clone())
                            .map_err(|error| LessonError::OpeningRejected { index, action, error })?;
                        self.log_last();
                        continue;
                    }
                    let allowed: Vec<PlayerAction> = self.progress.allowed(&view).into_iter().cloned().collect();
                    if let Some(pass) = pass
                        && (view.legal_actions.len() == 1 || allowed.is_empty())
                    {
                        self.session.submit(pass).expect("passing priority is legal: it was in legal_actions");
                        self.log_last();
                        continue;
                    }
                    self.handed_over = true;
                    return Ok(LessonStep::Prompt {
                        view,
                        allowed,
                        step: self.progress.step_index(),
                        total: self.progress.total(),
                    });
                }
                SessionStep::Ended { winner, reason } => return Ok(LessonStep::Ended { winner, reason }),
                SessionStep::Stalled(reason) => return Ok(LessonStep::Stalled(reason)),
            }
        }
    }

    /// The learner's action. Exactly `Session::submit` — not filtered by
    /// the live step, for the same reason `Session::submit` is not filtered
    /// by `legal_actions`: the client chose from a list it was shown, and
    /// the engine's guards are the only authority.
    pub fn submit(&mut self, action: PlayerAction) -> Result<(), SubmitError> {
        self.session.submit(action)?;
        self.log_last();
        Ok(())
    }

    fn log_last(&mut self) {
        if let Some(entry) = self.session.last_entry_for(self.learner) {
            self.log.push(entry);
        }
    }

    /// Every action resolved since the last call, as the learner may see
    /// it. The TUI drains this after each `step` and `submit` for its
    /// match log.
    pub fn drain_log(&mut self) -> Vec<PublicHistoryEntry> {
        std::mem::take(&mut self.log)
    }

    /// Feeds every recorded action since the last call into the lesson —
    /// once the learner has been handed over to; before that the entries
    /// are only skipped past.
    fn absorb(&mut self) {
        let entries = self.session.history().entries();
        if self.handed_over {
            for entry in &entries[self.absorbed..] {
                self.progress.observe(&entry.events);
            }
        }
        self.absorbed = entries.len();
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn progress(&self) -> &LessonProgress {
        &self.progress
    }

    pub fn learner(&self) -> Side {
        self.learner
    }
}
