//! The board's state: the human's latest masked view, the actions it
//! offers and what each click on the board means, the match log, what
//! just changed, and which overlay is up.
//!
//! **No rule and no `GameState`.** Everything here is a `ClientView`,
//! the `ActionMap` built from its `legal_actions`, and the `Transition`s
//! between one view and the next; a click resolves to an index into the
//! map, and the outcome of an intent is at most one `PlayerAction` for
//! the screen to hand to `MatchHandle::submit`, which does not filter
//! it. The model never decides an action is legal — it lists what the
//! engine said was.
//!
//! **A click never acts; it opens a [`Menu`] above the card.** A card or
//! a zone clicked with the primary button lists what may be done with
//! it (`entries_for`) as a small panel just above the card's own box
//! (the [`Anchor`] the screen read off the node), never at the pointer,
//! so it is in the same place however the card was clicked and the card
//! stays in view; a press on one of its buttons submits. The first cut
//! submitted a card's single action on the click meant to examine it and
//! lost a game to it (Phase 7 §3, item 8); the menu is the deliberate
//! second click. It never opens over a sheet, the options or the quit
//! prompt (the board still reports hovers through an overlay's ground),
//! and it closes on the next click anywhere, on Escape, and when the
//! board moves under it. The basic actions live on a fixed control bar
//! (`board::Control`) and the prompt's decisions under the prompt, so
//! nothing depends on the flat panel, which is an aid a person turns on
//! (`DesktopPrefs::play_helper`).
//!
//! **A secondary click reads; it never offers.** The right button (or
//! Ctrl or Cmd with the primary) opens the target's [`Sheet`]: the card
//! large — plus its state, for an installed card, which the printed face
//! cannot show — or a zone's contents. It carries no actions. Until
//! Phase 7 §4g the primary click opened the sheet with the actions beside
//! the card and the secondary opened the menu; the person asked for the
//! card-client convention instead, where the everyday click is the one
//! that acts and reading is the other button, and one list of actions in
//! one place is also one fewer thing to keep in step. The score area is
//! the exception: a scored agenda has no card on the board to click, so
//! its abilities stay on its row.
//!
//! **The hand is the person's own order** ([`HandOrder`]). The rules give
//! a hand no order at all — it is a multiset — so the one the engine hands
//! over is the order cards happened to be drawn in, and a person sorting
//! their hand is doing something the rules allow and the view cannot
//! record. The order lives here, is reconciled with every view (a drawn
//! card joins the end, a played one drops out), and never leaves the
//! client: the engine, the mask and the action map know nothing about it.
//!
//! **A card dragged onto the board is played there.** While a hand card
//! is held, the places its own entries name are lit (`ActionMap::
//! destinations_for_hand_card`), a remote the Corp has not made yet
//! included, the rig for a Runner install and the table for an event or
//! an operation; dropping on one submits the entry that lands there, or opens
//! a menu of just those entries when a place offers more than one. A drop
//! anywhere else puts the card back. This is the one gesture on the board
//! that acts — a *click* still never does, because a click is what a
//! person does to look — and it is deliberate in a way a click is not:
//! the card was picked up, carried to a place that lit up, and let go.
//!
//! **A run is a [`RunTrail`], kept after it ends.** The screen's pacer
//! hands the run's events over a beat at a time ([`Intent::RunStep`])
//! before the message that applied them, so the trail moves ahead of
//! the board and the board never ahead of the trail; the message then
//! `sync`s the trail with the view. A trail outlives its run — until
//! the next run begins or the next turn starts — because a run that
//! vanished the instant it ended told the Corp nothing about what it
//! did.

use std::sync::Arc;

use netrunner_client::actions::{pop_log_entries, push_linked_log_line, LogLine};
use netrunner_client::board::{encounter_subroutines, routes, transitions, ActionMap, Affordance, Asks, AutoBreak, Control, Encounter, Next, Pile, Prompt, Route, RunTrail, Target, Transition};
use netrunner_client::play::{lone_pass, GameEndReason, MatchMessage, PublicHistoryEntry};
use netrunner_client::tally::Tally;
use netrunner_client::run_pass::RunPass;
use netrunner_client::standing::{optional_trigger, standing_answer, Answer, Answers, OptionalPrompt};
use netrunner_client::record::RecordReport;
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{GameEvent, InstallId, PlayerAction, ServerId, Side};
use netrunner_core::view::ClientView;

use crate::models::drag::{insert_at, Drag, Release};
use crate::models::lesson::LessonBoard;
use crate::models::shortcuts::Shortcut;

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Message(MatchMessageRef),
    /// A beat of a run: the events, in order, that the trail observes
    /// before the message that applied them arrives.
    RunStep(Vec<GameEvent>),
    /// A primary click on a card or a zone: opens its actions as a menu
    /// just above the node that was clicked, `over`.
    Click { target: Target, over: Anchor },
    /// A secondary click on a card or a zone: opens its sheet, to read.
    Inspect(Target),
    /// A click that landed on nothing of the menu's: closes it.
    CloseMenu,
    /// An entry of the action map, by index — from a sheet, the rail or
    /// the flat panel.
    Choose(usize),
    /// A control-bar button: the entry it means, if the engine lists one.
    Control(Control),
    /// Open a card over whatever is open (a face in a zone sheet), or
    /// close it with `None`.
    InspectCard(Option<CardId>),
    /// A row of a list sheet (the score area's agendas), by position:
    /// opens its details in place, or closes them if it was the open one.
    Expand(usize),
    /// A card of the person's hand dragged from one place in it to
    /// another: `to` is the place it was dropped before.
    ReorderHand { from: usize, to: usize },
    /// The primary button went down on the person's own hand card at
    /// place `slot`, with the pointer at `at`: armed, not acted on, until
    /// the release says whether it was a click or a drag.
    DragPress { slot: usize, at: (f32, f32) },
    /// The pointer moved while a hand card's press is held.
    DragMove { at: (f32, f32) },
    /// The card being dragged was dropped on `target`, a place the board
    /// lit for it: the one entry that takes it there, or a menu of them
    /// when the place offers more than one.
    DragDrop { target: Target, over: Anchor },
    /// The primary button came up: a still press is the click that opens
    /// the card's menu over `over`; a travelled one drops the card into
    /// the row, before the card whose centre the pointer is left of
    /// (`slots`, the laid-out centres of the hand's faces).
    DragRelease { over: Anchor, slots: Vec<f32> },
    /// A route of the encountered ICE's (`Game::breaks`), by index:
    /// break every subroutine with that card, one step a view.
    Break(usize),
    /// Take the last move back (`Game::back`).
    TakeBack,
    /// The lesson's opening words are read: the board is the person's.
    BeginLesson,
    /// Open or close a lesson's escape hatch: every legal action offered,
    /// or only the step's (`LessonBoard::every_action`).
    EveryAction,
    /// The Corp's "no more this run": pass this window and every one
    /// after it until the run ends (`netrunner_client::run_pass`), or,
    /// while that is on, stop and be asked again.
    PassTheRun,
    /// Answer the optional trigger on the prompt, and every one like it
    /// from now on (`netrunner_client::standing`). The screen saves
    /// `Game::answers` after it.
    Remember(Answer),
    /// A key the board reads (`shortcuts`). The ones about the pointer
    /// or the settings file are the screen's; the model answers the rest.
    Shortcut(Shortcut),
    /// The gear: open or close the game options.
    ToggleOptions,
    /// The phase panel, or T: open or close the timing chart.
    ToggleTiming,
    /// Escape: closes the options, the list of keys, the menu, the
    /// inspector, the sheet or the quit prompt, in that order, else asks
    /// to quit.
    Back,
    RequestQuit,
    ConfirmQuit,
    CancelQuit,
    /// A replay moved somewhere other than one step on: the board is put
    /// at `view` with the log as it read there, and nothing moved *to*
    /// here, so there is no transition to light (`Game::replay`).
    Show { view: Box<ClientView>, log: Vec<LogLine> },
}

/// Where a replay stands, for its bar and its rail (`Game::replay`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayAt {
    /// How many actions have been applied at the position shown.
    pub cursor: usize,
    /// How many the record holds.
    pub len: usize,
    /// What names the record: its file, and the bot it was against.
    pub title: String,
}

/// `MatchMessage` is not `Clone` (a view is large), so an intent carries
/// it by value through this wrapper, which the screen builds from the
/// message it polled.
#[derive(Debug)]
pub struct MatchMessageRef(pub MatchMessage);

impl Clone for MatchMessageRef {
    fn clone(&self) -> Self {
        unreachable!("a match message is applied once and never cloned")
    }
}

impl PartialEq for MatchMessageRef {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

/// What the screen does after an intent.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Nothing,
    /// Something to draw changed.
    Redraw,
    /// Hand this to the match.
    Submit(PlayerAction),
    /// Ask the match for the last move back (`MatchHandle::rewind`).
    Rewind,
    /// Leave the screen.
    Quit,
}

/// What a secondary click opened, to read: a card target is drawn as the
/// card (and an install's state), a zone target as its contents. No
/// entries: what may be done is the menu's, so a sheet left open while
/// the opponent acts has nothing to go stale.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub target: Target,
}

/// The box on the window a menu sits against. It lives with the fitting
/// math now (`models::layout`), beside the [`crate::models::layout::menu_box`]
/// that is its only reader, and is re-exported here because a menu is
/// what one is *for*.
pub use crate::models::layout::Anchor;

/// A click's menu: the target, the entries a press could mean
/// (`entries_for`), and the box it sits above.
/// Unlike a sheet it does not outlive the board it was opened over:
/// an applied action closes it, since the card it sits on may have
/// moved.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub target: Target,
    pub entries: Vec<usize>,
    pub over: Anchor,
}

/// The person's own order for their hand, kept by card id.
///
/// A hand is a multiset, so two copies of a card are interchangeable and
/// the order is a list of ids rather than of anything per-copy: moving
/// "a Sure Gamble" is the whole of the choice, and there is nothing a
/// per-copy handle would buy. [`HandOrder::sync`] keeps it honest against
/// the view, which is the authority on *what* is in hand.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HandOrder {
    order: Vec<CardId>,
}

impl HandOrder {
    /// The hand as the person arranged it. Always the view's own multiset.
    pub fn cards(&self) -> &[CardId] {
        &self.order
    }

    /// Reconciles with the hand the view gives: cards already placed keep
    /// their places, a card drawn (or a second copy of one held) joins the
    /// end, and a card that left drops out.
    pub fn sync(&mut self, hand: &[CardId]) {
        let mut left: Vec<CardId> = hand.to_vec();
        let mut kept: Vec<CardId> = Vec::with_capacity(hand.len());
        for card in &self.order {
            if let Some(at) = left.iter().position(|c| c == card) {
                // `remove`, not `swap_remove`: what is left over is the
                // cards the person has not placed, and they join the end
                // in the view's own order rather than a scrambled one.
                left.remove(at);
                kept.push(card.clone());
            }
        }
        kept.extend(left);
        self.order = kept;
    }

    /// Moves the card at `from` to sit before what is now at `to`, as a
    /// drop between two cards reads.
    pub fn move_card(&mut self, from: usize, to: usize) -> bool {
        if from >= self.order.len() || to > self.order.len() {
            return false;
        }
        let card = self.order.remove(from);
        let at = if to > from { to - 1 } else { to };
        self.order.insert(at.min(self.order.len()), card);
        at != from
    }
}

/// The end of the match, for the game-over overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct Over {
    pub winner: Side,
    pub reason: GameEndReason,
    pub report: Option<RecordReport>,
    pub notice: Option<String>,
}

pub struct Game {
    registry: Arc<CardRegistry>,
    pub side: Side,
    pub view: Option<ClientView>,
    pub actions: ActionMap,
    pub prompt: Option<Prompt>,
    pub log: Vec<LogLine>,
    /// What the last applied action changed, for the screen to
    /// highlight on its next redraw; cleared by `take_transitions`.
    pub transitions: Vec<Transition>,
    /// The human may act: an `Awaiting` arrived and nothing has been
    /// submitted since. Off while the opponent thinks or a submit is in
    /// flight, so a double click cannot send two actions.
    pub awaiting: bool,
    pub sheet: Option<Sheet>,
    /// A card's text over the sheet (a face in a pile), or alone.
    pub inspecting: Option<CardId>,
    /// The row of a list sheet whose details are open, by position. One
    /// at a time, in place rather than over the sheet: the score area is
    /// a short list read top to bottom, and a card over it hid the rest.
    /// Cleared whenever a sheet opens.
    pub expanded: Option<usize>,
    /// A click's menu, over its card.
    pub menu: Option<Menu>,
    /// The person's own order for their own hand.
    pub hand: HandOrder,
    /// The press being held on a hand card: which place in the hand it
    /// picked up, and where the pointer has been since.
    pub dragging: Option<Drag<usize>>,
    pub options_open: bool,
    /// The list of keys (`shortcuts::LIST`) is up.
    pub help_open: bool,
    /// The timing of the turn and the run is up
    /// (`netrunner_client::board::timing`).
    pub timing_open: bool,
    /// Whether there is a move to take back, as the match thread last
    /// said (`MatchMessage::Back`). The kind it names is not kept: a game
    /// against a bot is casual, so every take-back costs the same nothing
    /// and none of them asks twice (`netrunner_client::play`).
    pub back: bool,
    pub rejection: Option<String>,
    /// Every card's way through the encountered ICE, with its price
    /// (`netrunner_client::board::breaks`), while the person is awaiting
    /// mid-encounter; empty otherwise.
    pub breaks: Vec<Route>,
    /// The route being carried out: each `Awaiting` asks it for the next
    /// step before the person is asked anything.
    pub breaking: Option<AutoBreak>,
    /// Why a route stopped before every subroutine was broken. Not a
    /// rejection — the engine refused nothing; the driver saw the price
    /// or the ICE change and handed the controls back.
    pub break_stopped: Option<String>,
    pub over: Option<Over>,
    pub stalled: Option<String>,
    /// Where the last bug report was saved, or why it was not — the
    /// screen's to write (`netrunner_client::bug_report` is file I/O) and
    /// the options' and the stall panel's to show, since the client's
    /// notices are drawn on the main menu, not over a game.
    pub saved_report: Option<String>,
    /// The file that report went to, when it was written: the board's
    /// "Watch it" opens it under Replays.
    pub saved_report_path: Option<std::path::PathBuf>,
    pub confirm_quit: bool,
    /// How many actions have been applied, for a screen to know the
    /// board moved without comparing views.
    pub applied: usize,
    /// The run on, or the last one, as a trail of steps; `None` before
    /// the first run and after the turn that followed the last.
    pub trail: Option<RunTrail>,
    /// A recorded match being stepped through rather than one being
    /// played (`screens::replay`). Nothing is ever awaiting, so no entry,
    /// glow or decision is offered; a primary click reads, as a secondary
    /// one does, and leaving asks nothing, because nothing is lost.
    pub replay: Option<ReplayAt>,
    /// The person's answers to optional triggers, from the settings file
    /// (`netrunner_client::standing`). A prompt one covers is answered
    /// without asking, the way `lone_pass` passes.
    pub answers: Answers,
    /// A move was just taken back: the prompt it restores is asked, not
    /// answered from `answers`, or taking back an answer given without
    /// asking would give it again at once.
    asking_again: bool,
    /// The Corp said "no more this run" (`Intent::PassTheRun`). Seen by
    /// every view, so it ends with the run.
    pub run_pass: RunPass,
    /// The chair's own log as it arrived, for [`Game::tally`] to be
    /// recounted from when a take-back drops the end of it.
    entries: Vec<PublicHistoryEntry>,
    /// What each side has done so far (`netrunner_client::tally`): the
    /// start-of-game box reads what the Corp did with its hand, and the
    /// end of the match is its table. The live match's only — a replay
    /// is put at a position by `Intent::Show`, and has no end panel.
    pub tally: Tally,
    /// A lesson being played rather than a game (`models::lesson`): its
    /// words, and the step that narrows what the board offers. Every
    /// policy that answers for the person — the lone pass, the run pass,
    /// a remembered answer, a break route — is off under a lesson, which
    /// passes for the learner itself where it means to and teaches the
    /// pump and the break by hand.
    pub lesson: Option<LessonBoard>,
}

impl Game {
    pub fn new(registry: Arc<CardRegistry>, side: Side) -> Self {
        Game {
            registry,
            side,
            view: None,
            actions: ActionMap::default(),
            prompt: None,
            log: Vec::new(),
            transitions: Vec::new(),
            awaiting: false,
            sheet: None,
            inspecting: None,
            expanded: None,
            menu: None,
            hand: HandOrder::default(),
            dragging: None,
            options_open: false,
            help_open: false,
            timing_open: false,
            back: false,
            rejection: None,
            breaks: Vec::new(),
            breaking: None,
            break_stopped: None,
            over: None,
            stalled: None,
            saved_report: None,
            saved_report_path: None,
            confirm_quit: false,
            applied: 0,
            trail: None,
            replay: None,
            answers: Answers::default(),
            asking_again: false,
            run_pass: RunPass::default(),
            entries: Vec::new(),
            tally: Tally::default(),
            lesson: None,
        }
    }

    /// A board for a lesson, its opening words up.
    pub fn lesson(registry: Arc<CardRegistry>, side: Side, lesson: LessonBoard) -> Self {
        Game { lesson: Some(lesson), ..Game::new(registry, side) }
    }

    /// A board for a recorded match, at `at`, seen from `side`.
    pub fn replay(registry: Arc<CardRegistry>, side: Side, at: ReplayAt) -> Self {
        Game { replay: Some(at), ..Game::new(registry, side) }
    }

    /// The ice being encountered and the state of its subroutines
    /// (`netrunner_client::board::encounter_subroutines`), for the rail
    /// to mark broken, fired or pending. Derived rather than a field,
    /// because it is a reading of the current view and nothing else —
    /// unlike `breaks`, which is a search and is worth keeping.
    ///
    /// **Not gated on `awaiting`**, unlike the glow and the routes: the
    /// marks are something to read, not something to press, and a person
    /// watching the Corp think mid-encounter still wants to know what is
    /// left pending.
    pub fn encounter(&self) -> Option<Encounter> {
        self.view.as_ref().and_then(encounter_subroutines)
    }

    pub fn registry(&self) -> &CardRegistry {
        &self.registry
    }

    /// The optional trigger the person is being asked, which the pop-up
    /// offers to answer for good (`Intent::Remember`); `None` in a
    /// replay and while nothing is awaited.
    pub fn optional_prompt(&self) -> Option<OptionalPrompt> {
        if !self.awaiting || self.replay.is_some() || self.lesson.is_some() {
            return None;
        }
        optional_trigger(self.view.as_ref()?, &self.registry)
    }

    /// Whether the match is over or gone, so the panel offers nothing. A
    /// lesson is over when its last step advances, whoever is ahead.
    pub fn finished(&self) -> bool {
        self.over.is_some() || self.stalled.is_some() || self.lesson.as_ref().is_some_and(|lesson| lesson.outro.is_some())
    }

    /// A lesson's opening words are up and nothing has been played.
    pub fn intro_open(&self) -> bool {
        self.lesson.as_ref().is_some_and(|lesson| lesson.intro.is_some())
    }

    /// Whether something covers the board — a sheet, a card being read,
    /// the options, the list of keys, the quit prompt, the end of the
    /// match — so a click that reaches a card through it opens nothing,
    /// and a key does nothing.
    pub fn covered(&self) -> bool {
        self.finished() || self.intro_open() || self.confirm_quit || self.options_open || self.help_open || self.timing_open || self.sheet.is_some() || self.inspecting.is_some()
    }

    /// Whether a click that misses the panel closes what is open.
    ///
    /// Only a *reading* surface: the card, the install, the zone, the
    /// score area, the timing chart. Those lost their Close button because the name was
    /// already on the card and the button was a third door to a rule
    /// Escape already had, so missing the panel is the second door.
    ///
    /// Three kinds of panel are deliberately excluded. The options and
    /// the list of keys are **forms**, and they keep their Close: a
    /// setting given up because the pointer landed an inch wide is a
    /// worse failure than a button nobody needed. The end of the match,
    /// a stall and the quit prompt have nowhere to dismiss *to* — they
    /// are asking a question, and Escape does not close them either
    /// (`Intent::Back` falls through to `RequestQuit`), so a scrim that
    /// closed them would be the only way out and would mean something
    /// different from every other panel's.
    pub fn dismissed_by_a_click_away(&self) -> bool {
        !self.finished() && !self.intro_open() && !self.confirm_quit && !self.options_open && !self.help_open && (self.sheet.is_some() || self.inspecting.is_some() || self.timing_open)
    }

    pub fn take_transitions(&mut self) -> Vec<Transition> {
        std::mem::take(&mut self.transitions)
    }

    /// The entries a press on `target` could mean, now: none while the
    /// person is not awaiting.
    pub fn entries_for(&self, target: &Target) -> Vec<usize> {
        if !self.awaiting {
            return Vec::new();
        }
        self.actions.for_target(target)
    }

    /// The glow `target` earns: what the engine will accept on it, and in
    /// which mood (`netrunner_client::board::affordance`).
    ///
    /// **Gated on `awaiting`, not on the map being empty**, and the
    /// difference is load-bearing: a seat off priority keeps legal
    /// actions of its own — the Corp's standing rez is in the Corp's
    /// `legal_actions` all through the Runner's turn — so a board that
    /// lit whatever the map held would glow at a person who cannot act.
    pub fn affordance_for(&self, target: &Target) -> Option<Affordance> {
        if !self.awaiting {
            return None;
        }
        self.actions.affordance(target)
    }

    pub fn apply(&mut self, intent: Intent) -> Outcome {
        match intent {
            Intent::TakeBack => self.take_back(),
            intent => self.apply_intent(intent),
        }
    }

    /// The words on the button that takes the last move back, `None` when
    /// there is none to take. One wording for the pop-up and the rail.
    pub fn back_label(&self) -> Option<&'static str> {
        (self.awaiting && self.back).then_some("Take it back")
    }

    /// The words on the rail's run-pass button (`Intent::PassTheRun`):
    /// the offer while the Corp is in a run's window, the way to stop
    /// while it is on, and `None` otherwise — in a replay, the other
    /// chair, and between runs.
    pub fn run_pass_label(&self) -> Option<&'static str> {
        if self.replay.is_some() || self.finished() || self.lesson.is_some() {
            return None;
        }
        if self.run_pass.is_on() {
            return Some("Stop passing: ask me again this run");
        }
        let view = self.view.as_ref()?;
        (self.awaiting && self.run_pass.offered(view)).then_some("Pass for the rest of this run")
    }

    /// The Back button, the rail's and U. It goes at once: nothing rides
    /// on a game against a bot, so there is nothing to ask about. Not a
    /// board click and not an action: the state it returns to is one the
    /// engine produced, and what may be done there is again whatever the
    /// engine lists.
    fn take_back(&mut self) -> Outcome {
        if !self.awaiting || self.covered() || !self.back {
            return Outcome::Nothing;
        }
        self.awaiting = false;
        self.menu = None;
        Outcome::Rewind
    }

    fn apply_intent(&mut self, intent: Intent) -> Outcome {
        match intent {
            Intent::Message(MatchMessageRef(message)) => self.message(message),
            Intent::BeginLesson => match &mut self.lesson {
                Some(lesson) if lesson.intro.is_some() => {
                    lesson.intro = None;
                    Outcome::Redraw
                }
                _ => Outcome::Nothing,
            },
            Intent::EveryAction => {
                let finished = self.finished();
                let Some(lesson) = &mut self.lesson else { return Outcome::Nothing };
                if !lesson.gated() || finished {
                    return Outcome::Nothing;
                }
                lesson.every_action = !lesson.every_action;
                // The list the board was built from changes under it; the
                // view and the prompt do not.
                if self.awaiting
                    && let Some(view) = &self.view
                {
                    self.menu = None;
                    self.actions = self.action_map(view);
                }
                Outcome::Redraw
            }
            Intent::RunStep(events) => {
                // A run beginning replaces the last run's trail; its ice
                // is filled in by the message that follows.
                if events.iter().any(|e| matches!(e, GameEvent::RunInitiated { .. })) {
                    self.trail = None;
                }
                match &mut self.trail {
                    Some(trail) if !trail.ended() => {
                        for event in &events {
                            trail.observe(event, &self.registry);
                        }
                        Outcome::Redraw
                    }
                    _ => Outcome::Nothing,
                }
            }
            Intent::Click { target, over } => self.click(target, over),
            Intent::Inspect(target) => {
                if self.covered() {
                    return Outcome::Nothing;
                }
                self.menu = None;
                self.expanded = None;
                self.sheet = Some(Sheet { target });
                Outcome::Redraw
            }
            Intent::CloseMenu => {
                if self.menu.take().is_some() {
                    Outcome::Redraw
                } else {
                    Outcome::Nothing
                }
            }
            Intent::Choose(index) => {
                self.sheet = None;
                self.inspecting = None;
                self.menu = None;
                match self.actions.entries.get(index) {
                    Some(entry) if self.awaiting => {
                        self.awaiting = false;
                        self.rejection = None;
                        self.breaking = None;
                        self.break_stopped = None;
                        Outcome::Submit(entry.action.clone())
                    }
                    _ => Outcome::Nothing,
                }
            }
            Intent::Remember(answer) => {
                let Some(prompt) = self.optional_prompt() else { return Outcome::Nothing };
                let Some(index) = prompt.action(answer).and_then(|action| self.actions.entries.iter().position(|entry| entry.action == action)) else {
                    return Outcome::Nothing;
                };
                self.answers.set(prompt.key, Some(answer));
                self.apply(Intent::Choose(index))
            }
            Intent::Control(control) => match self.actions.for_control(control) {
                Some(index) if self.awaiting => self.apply(Intent::Choose(index)),
                _ => Outcome::Nothing,
            },
            Intent::Break(index) => self.start_break(index),
            Intent::PassTheRun if self.run_pass.is_on() => {
                self.run_pass.stop();
                Outcome::Redraw
            }
            Intent::PassTheRun => {
                if self.covered() || self.replay.is_some() || !self.awaiting {
                    return Outcome::Nothing;
                }
                let Some(pass) = self.view.as_ref().and_then(|view| self.run_pass.start(view)) else { return Outcome::Nothing };
                match self.actions.entries.iter().position(|entry| entry.action == pass) {
                    Some(index) => self.apply(Intent::Choose(index)),
                    None => {
                        self.run_pass.stop();
                        Outcome::Nothing
                    }
                }
            }
            // Routed by `apply`.
            Intent::TakeBack => Outcome::Nothing,
            Intent::InspectCard(card) => {
                self.inspecting = card;
                Outcome::Redraw
            }
            Intent::DragPress { slot, at } => {
                self.dragging = Some(Drag::press(slot, at));
                Outcome::Nothing
            }
            Intent::DragMove { at } => {
                // Only the move that starts the drag redraws: the card is
                // lifted once, and the row does not follow the pointer.
                match self.dragging.as_mut().map(|drag| drag.moved(at)) {
                    Some(true) => Outcome::Redraw,
                    _ => Outcome::Nothing,
                }
            }
            Intent::DragDrop { target, over } => {
                let Some(drag) = self.dragging.take() else { return Outcome::Nothing };
                let Some(card) = self.hand.cards().get(drag.what).cloned() else { return Outcome::Redraw };
                if !drag.dragging {
                    return self.click(Target::HandCard(card), over);
                }
                match self.actions.for_hand_card_at(&card, &target).as_slice() {
                    [] => Outcome::Redraw,
                    [index] => self.apply(Intent::Choose(*index)),
                    // Two ways to the same place — an agenda over what is
                    // in the root, say. A drag says where, not which, so
                    // the menu asks rather than the drop guessing.
                    several => {
                        let entries = several.to_vec();
                        self.menu = Some(Menu { target, entries, over });
                        Outcome::Redraw
                    }
                }
            }
            Intent::DragRelease { over, slots } => {
                let Some(drag) = self.dragging.take() else { return Outcome::Nothing };
                let (release, (x, _)) = drag.release();
                match release {
                    Release::Click => match self.hand.cards().get(drag.what).cloned() {
                        Some(card) => self.click(Target::HandCard(card), over),
                        None => Outcome::Redraw,
                    },
                    Release::Moved => self.apply(Intent::ReorderHand { from: drag.what, to: insert_at(&slots, x) }),
                }
            }
            Intent::ReorderHand { from, to } => {
                if self.hand.move_card(from, to) {
                    Outcome::Redraw
                } else {
                    Outcome::Nothing
                }
            }
            Intent::Expand(row) => {
                if self.sheet.is_none() {
                    return Outcome::Nothing;
                }
                self.expanded = if self.expanded == Some(row) { None } else { Some(row) };
                Outcome::Redraw
            }
            Intent::Shortcut(shortcut) => self.shortcut(shortcut),
            Intent::ToggleOptions => {
                self.options_open = !self.options_open;
                self.menu = None;
                Outcome::Redraw
            }
            Intent::ToggleTiming => {
                if self.timing_open {
                    self.timing_open = false;
                    return Outcome::Redraw;
                }
                // Not over another panel: the chart is read over the board.
                if self.covered() {
                    return Outcome::Nothing;
                }
                self.menu = None;
                self.timing_open = true;
                Outcome::Redraw
            }
            Intent::Back => {
                if self.options_open {
                    self.options_open = false;
                    Outcome::Redraw
                } else if self.help_open {
                    self.help_open = false;
                    Outcome::Redraw
                } else if self.timing_open {
                    self.timing_open = false;
                    Outcome::Redraw
                } else if self.menu.take().is_some() || self.inspecting.take().is_some() || self.sheet.take().is_some() {
                    Outcome::Redraw
                } else if self.confirm_quit {
                    self.confirm_quit = false;
                    Outcome::Redraw
                } else {
                    self.apply(Intent::RequestQuit)
                }
            }
            Intent::RequestQuit => {
                // Nothing is lost before a lesson begins, as nothing is
                // after a match ends: Escape on the intro is its Leave.
                if self.finished() || self.replay.is_some() || self.intro_open() {
                    Outcome::Quit
                } else {
                    self.confirm_quit = true;
                    Outcome::Redraw
                }
            }
            Intent::ConfirmQuit => Outcome::Quit,
            Intent::CancelQuit => {
                self.confirm_quit = false;
                Outcome::Redraw
            }
            Intent::Show { view, log } => {
                self.log = log;
                self.transitions.clear();
                self.trail = None;
                self.follow_run(&view);
                self.view = Some(*view);
                self.follow_hand();
                self.menu = None;
                Outcome::Redraw
            }
        }
    }

    fn message(&mut self, message: MatchMessage) -> Outcome {
        match message {
            MatchMessage::Applied { entry, view } => {
                self.run_pass.see(&view);
                if let Some(before) = &self.view {
                    self.transitions.extend(transitions(before, &view, &entry));
                }
                push_linked_log_line(&mut self.log, &entry, &self.registry, Some(&view));
                self.tally.add(&entry);
                self.entries.push(entry);
                self.follow_run(&view);
                self.view = Some(*view);
                self.follow_hand();
                self.applied += 1;
                self.awaiting = false;
                self.actions = ActionMap::default();
                self.breaks.clear();
                self.prompt = None;
                // The board moved under any open sheet, but what it
                // shows — Archives, a card — is still worth reading. A
                // menu is not: it sat over a card that may be gone.
                self.menu = None;
                Outcome::Redraw
            }
            // Passing is the only thing to do: the board shows the view
            // and the pass goes back without a click (`lone_pass`), with
            // nothing offered meanwhile — the pacer has already held this
            // message a beat inside a run, so the board is seen to move.
            // The map is still built, so a pass the engine rejects comes
            // back as `Rejected` onto a live bar and cannot loop.
            MatchMessage::Awaiting { view } if self.lesson.is_none() && lone_pass(&view, &self.registry).is_some() => {
                let pass = lone_pass(&view, &self.registry).expect("matched above");
                self.actions = ActionMap::build(&view, &self.registry);
                self.prompt = Prompt::of(&view, &self.registry);
                self.view = Some(*view);
                self.follow_hand();
                self.awaiting = false;
                Outcome::Submit(pass)
            }
            // The rest of the run is being passed: the same, for a pass
            // that sits beside a choice the person has already declined.
            MatchMessage::Awaiting { view } if self.lesson.is_none() && self.run_pass.pass(&view).is_some() => {
                let pass = self.run_pass.pass(&view).expect("matched above");
                self.actions = ActionMap::build(&view, &self.registry);
                self.prompt = Prompt::of(&view, &self.registry);
                self.view = Some(*view);
                self.follow_hand();
                self.awaiting = false;
                Outcome::Submit(pass)
            }
            // An optional trigger the person has answered for good: the
            // same, for the same reasons, with the answer they gave.
            MatchMessage::Awaiting { view } if self.lesson.is_none() && !self.asking_again && standing_answer(&view, &self.registry, &self.answers).is_some() => {
                let (action, _) = standing_answer(&view, &self.registry, &self.answers).expect("matched above");
                self.actions = ActionMap::build(&view, &self.registry);
                self.prompt = Prompt::of(&view, &self.registry);
                self.view = Some(*view);
                self.follow_hand();
                self.awaiting = false;
                Outcome::Submit(action)
            }
            MatchMessage::Awaiting { view } => {
                self.asking_again = false;
                self.run_pass.see(&view);
                self.actions = self.action_map(&view);
                self.prompt = Prompt::of(&view, &self.registry);
                // A route under way takes its next step before the person
                // is asked anything; the board still shows the view.
                if let Some(step) = self.continue_break(&view) {
                    self.view = Some(*view);
                    self.follow_hand();
                    self.awaiting = false;
                    return Outcome::Submit(step);
                }
                // A lesson teaches the pump and the break; a route would
                // skip them.
                self.breaks = if self.lesson.is_some() { Vec::new() } else { routes(&view, &self.registry) };
                self.view = Some(*view);
                self.follow_hand();
                self.awaiting = true;
                Outcome::Redraw
            }
            // Held until the `Awaiting` it belongs to, which follows at
            // once; a board that is not a lesson has nothing to coach.
            MatchMessage::Coach(coaching) => {
                if let Some(lesson) = &mut self.lesson {
                    lesson.coaching = Some(coaching);
                }
                Outcome::Nothing
            }
            MatchMessage::LessonComplete { view, outro } => {
                self.view = Some(*view);
                self.follow_hand();
                self.close_for_the_end();
                if let Some(lesson) = &mut self.lesson {
                    lesson.coaching = None;
                    lesson.outro = Some(outro);
                }
                Outcome::Redraw
            }
            MatchMessage::Back { rewind } => {
                self.back = rewind.is_some();
                Outcome::Nothing
            }
            // The board snaps to where the move was made from: nothing
            // moved *to* here, so there is no transition to play, and the
            // log loses what no longer happened. The `Awaiting` that
            // follows hands the controls back.
            MatchMessage::Rewound { view, removed, .. } => {
                pop_log_entries(&mut self.log, removed, "You took that back.");
                self.entries.truncate(self.entries.len().saturating_sub(removed));
                self.tally = Tally::of(&self.entries);
                self.asking_again = true;
                // A pass taken for the person is taken back like any
                // other, and would be taken again at once.
                self.run_pass.stop();
                self.applied = self.applied.saturating_sub(removed);
                self.transitions.clear();
                self.trail = None;
                self.view = Some(*view);
                self.follow_hand();
                self.awaiting = false;
                self.actions = ActionMap::default();
                self.breaks.clear();
                self.breaking = None;
                self.prompt = None;
                self.menu = None;
                self.back = false;
                self.rejection = None;
                Outcome::Redraw
            }
            MatchMessage::Rejected { reason } => {
                // The engine refused a route's step: the route is over
                // and the refusal is the thing to read.
                self.breaking = None;
                self.rejection = Some(reason);
                self.awaiting = true;
                Outcome::Redraw
            }
            MatchMessage::Ended { winner, reason, view, report, notice } => {
                self.view = Some(*view);
                self.follow_hand();
                self.close_for_the_end();
                self.over = Some(Over { winner, reason, report, notice });
                Outcome::Redraw
            }
            MatchMessage::Stalled { reason } => {
                self.awaiting = false;
                self.actions = ActionMap::default();
                self.menu = None;
                self.breaks.clear();
                self.breaking = None;
                self.stalled = Some(reason);
                Outcome::Redraw
            }
        }
    }

    /// The action map for a view the person is asked on: its entries, what
    /// each card will ask before it is played, and — in a lesson — only
    /// the step's actions (`LessonBoard::offered`).
    fn action_map(&self, view: &ClientView) -> ActionMap {
        let offered = match &self.lesson {
            Some(lesson) => lesson.offered(view),
            None => std::borrow::Cow::Borrowed(view),
        };
        let mut actions = ActionMap::build(&offered, &self.registry);
        // What each card will ask, on its button before it is played:
        // once per view, like the routes.
        actions.annotate(&Asks::of(&offered, &self.registry));
        actions
    }

    /// Everything that offers or covers put away for the end of a match
    /// or a lesson: nothing is awaited, and the end's panel is the one
    /// thing up.
    fn close_for_the_end(&mut self) {
        self.awaiting = false;
        self.actions = ActionMap::default();
        self.prompt = None;
        self.sheet = None;
        self.inspecting = None;
        self.menu = None;
        self.options_open = false;
        self.help_open = false;
        self.timing_open = false;
        self.confirm_quit = false;
        self.breaks.clear();
        self.breaking = None;
    }

    /// Starts route `index`: its first step is submitted now, the rest on
    /// the views that follow (`continue_break`).
    fn start_break(&mut self, index: usize) -> Outcome {
        if !self.awaiting || self.covered() {
            return Outcome::Nothing;
        }
        let (Some(route), Some(view)) = (self.breaks.get(index), self.view.as_ref()) else { return Outcome::Nothing };
        let driver = AutoBreak::new(route, view, &self.registry);
        match driver.next(view, &self.registry) {
            Next::Submit(step) => {
                self.menu = None;
                self.awaiting = false;
                self.rejection = None;
                self.break_stopped = None;
                self.breaking = Some(driver);
                Outcome::Submit(step)
            }
            Next::Stopped(reason) => {
                self.break_stopped = Some(reason);
                Outcome::Redraw
            }
            Next::Wait | Next::Done => Outcome::Nothing,
        }
    }

    /// The running route's next step on `view`, if it has one; a route
    /// that finished or stopped is cleared, the latter with its reason.
    fn continue_break(&mut self, view: &ClientView) -> Option<PlayerAction> {
        match self.breaking.as_ref()?.next(view, &self.registry) {
            Next::Submit(step) => Some(step),
            Next::Wait => None,
            Next::Done => {
                self.breaking = None;
                None
            }
            Next::Stopped(reason) => {
                self.breaking = None;
                self.break_stopped = Some(reason);
                None
            }
        }
    }

    /// Keeps the person's hand order with the view: what is in hand is
    /// the view's to say, the order the person's.
    fn follow_hand(&mut self) {
        let hand = self.view.as_ref().and_then(|view| match self.side {
            Side::Corp => view.corp.hq_cards.clone(),
            Side::Runner => view.runner.grip_cards.clone(),
        });
        self.hand.sync(&hand.unwrap_or_default());
    }

    /// Keeps the trail with the view after an action applied: a run on
    /// the board with no trail (or an ended one) begins one, a run on
    /// the board with a trail syncs it, a trail whose run is gone is
    /// given its outcome and kept until a new turn starts.
    fn follow_run(&mut self, view: &ClientView) {
        let new_turn = self.transitions.iter().any(|t| matches!(t, Transition::TurnStarted { .. }));
        match (&mut self.trail, &view.active_run) {
            (Some(trail), Some(run)) if !trail.ended() => trail.sync(Some(run)),
            (_, Some(run)) => self.trail = Some(RunTrail::begin(run)),
            (Some(trail), None) => {
                trail.sync(None);
                if new_turn {
                    self.trail = None;
                }
            }
            (None, None) => {}
        }
    }

    /// A key: the button it stands for, pressed. The list of keys opens
    /// and closes on its own key whatever else is up, since it is how a
    /// person finds the rest; every other key does nothing while the board
    /// is covered. The pointer's keys and the play helper are the screen's
    /// (it knows what is hovered and owns the settings file), so here they
    /// are nothing.
    fn shortcut(&mut self, shortcut: Shortcut) -> Outcome {
        if shortcut == Shortcut::Help {
            if self.help_open {
                self.help_open = false;
                return Outcome::Redraw;
            }
            if self.covered() {
                return Outcome::Nothing;
            }
            self.menu = None;
            self.help_open = true;
            return Outcome::Redraw;
        }
        if shortcut == Shortcut::Timing {
            return self.apply(Intent::ToggleTiming);
        }
        if self.covered() {
            return Outcome::Nothing;
        }
        match shortcut {
            Shortcut::Control(control) => self.apply(Intent::Control(control)),
            Shortcut::Decision(n) => {
                if !self.awaiting {
                    return Outcome::Nothing;
                }
                let buttons = match &self.menu {
                    Some(menu) => menu.entries.clone(),
                    None => self.actions.decisions(),
                };
                match buttons.get(n) {
                    Some(index) => self.apply(Intent::Choose(*index)),
                    // An encounter has no decisions of its own, so the
                    // number keys are the rail's route buttons there.
                    None if buttons.is_empty() && self.menu.is_none() => self.apply(Intent::Break(n)),
                    None => Outcome::Nothing,
                }
            }
            Shortcut::ScoreArea(side) => self.apply(Intent::Inspect(Target::Pile(Pile::Agendas(side)))),
            Shortcut::TakeBack => self.apply(Intent::TakeBack),
            Shortcut::PassTheRun => self.apply(Intent::PassTheRun),
            Shortcut::ReadHovered | Shortcut::MenuHovered | Shortcut::PlayHelper | Shortcut::PhaseBar | Shortcut::Help | Shortcut::Timing => Outcome::Nothing,
        }
    }

    /// Opens the target's menu over `over`, or closes it if it was this
    /// target's. Three exceptions: a selection position is a toggle on a
    /// card the prompt lists, so its one entry is submitted as the rail's
    /// button would; the Agendas readout is a door to the score area,
    /// not a card, so it opens that list — where a scored agenda's
    /// abilities are; and the heap is public to both sides and no action
    /// is ever on it (a card installed from the heap is offered on the
    /// prompt, not the pile), so a click opens its contents. Its menu was
    /// always empty, and the person asked that a click on the heap show
    /// every card in it, as the secondary click already did.
    fn click(&mut self, target: Target, over: Anchor) -> Outcome {
        if self.covered() {
            return Outcome::Nothing;
        }
        // A replay has nothing to offer on a card, so the click reads it:
        // a menu of nothing would be a click that did nothing twice.
        if self.replay.is_some() {
            return match target {
                Target::Position(_) => Outcome::Nothing,
                target => self.apply(Intent::Inspect(target)),
            };
        }
        let entries = self.entries_for(&target);
        match target {
            Target::Position(_) => {
                self.menu = None;
                match entries.as_slice() {
                    [index] => self.apply(Intent::Choose(*index)),
                    _ => Outcome::Nothing,
                }
            }
            Target::Pile(Pile::Agendas(_)) => self.apply(Intent::Inspect(target)),
            Target::Pile(Pile::Heap) if entries.is_empty() => self.apply(Intent::Inspect(target)),
            _ if self.menu.as_ref().is_some_and(|menu| menu.target == target) => {
                self.menu = None;
                Outcome::Redraw
            }
            _ => {
                self.menu = Some(Menu { target, entries, over });
                Outcome::Redraw
            }
        }
    }

    /// The card at an install, when the viewer may see it.
    pub fn card_at(&self, id: InstallId) -> Option<CardId> {
        self.view.as_ref().and_then(|view| netrunner_client::actions::installed_card_id(view, &id))
    }

    /// The card a sheet's target shows, when the viewer may see it: a
    /// hand card, a visible install, an identity. A zone has none.
    pub fn card_of(&self, target: &Target) -> Option<CardId> {
        match target {
            Target::HandCard(card) => Some(card.clone()),
            Target::Install(id) => self.card_at(*id),
            Target::Identity(side) => self.view.as_ref().and_then(|view| match side {
                Side::Corp => view.corp.identity.clone(),
                Side::Runner => view.runner.identity.clone(),
            }),
            Target::Server(_) | Target::Position(_) | Target::Pile(_) | Target::Rig | Target::Table => None,
        }
    }

    /// The place in the hand the pointer is dragging a card out of, once
    /// it has travelled far enough to be a drag.
    pub fn dragged_slot(&self) -> Option<usize> {
        self.dragging.as_ref().filter(|drag| drag.dragging).map(|drag| drag.what)
    }

    /// The card being dragged, once the press has become a drag.
    pub fn dragged_card(&self) -> Option<CardId> {
        self.dragged_slot().and_then(|slot| self.hand.cards().get(slot).cloned())
    }

    /// Where the card being dragged may be dropped: lit while it is held,
    /// and empty when nothing is held or the engine offers the card
    /// nowhere.
    pub fn drop_places(&self) -> Vec<Target> {
        match self.dragged_card() {
            Some(card) if self.awaiting => self.actions.destinations_for_hand_card(&card),
            _ => Vec::new(),
        }
    }

    /// The clicks the person's side has left, as the view shows them.
    pub fn clicks_left(&self) -> u32 {
        self.view.as_ref().map_or(0, |view| match self.side {
            Side::Corp => view.corp.clicks,
            Side::Runner => view.runner.clicks,
        })
    }

    /// Whether `server` is the one under run.
    pub fn run_on(&self, server: ServerId) -> bool {
        self.view.as_ref().and_then(|v| v.active_run.as_ref()).is_some_and(|run| run.server == server)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_client::play::{LocalMatchSpec, MatchHandle};
    use netrunner_client::start::Level;
    use netrunner_core::rules::GamePhase;

    /// A real match against the bottom rung, so the views are real; the
    /// model is driven by hand from there.
    fn game(side: Side) -> (Game, MatchHandle) {
        let registry = Arc::new(netrunner_client::decks::sample_deck_registry());
        let corp = netrunner_core::decks::by_id("discretion_advised").unwrap();
        let runner = netrunner_core::decks::by_id("stolen_goods").unwrap();
        let spec = LocalMatchSpec { registry: registry.clone(), corp, runner, human: side, level: Level::Novice, style: None, seed: 11, rules: Default::default(), record: None };
        let handle = MatchHandle::start_local(spec).unwrap();
        (Game::new(registry, side), handle)
    }

    fn until_awaiting(game: &mut Game, handle: &mut MatchHandle) {
        while !game.awaiting {
            let message = handle.wait().expect("the match is alive");
            if let Outcome::Submit(pass) = game.apply(Intent::Message(MatchMessageRef(message))) {
                handle.submit(pass).unwrap();
            }
            assert!(!game.finished(), "the game ended first");
        }
    }

    /// Keeps the first decision (the mulligan), and lets the Runner keep,
    /// until the Corp's first action phase.
    fn until_the_corps_action_phase(game: &mut Game, handle: &mut MatchHandle) {
        until_the_action_phase(game, handle, Side::Corp);
    }

    /// Takes the first decision each time it is asked until `side`'s
    /// first action phase.
    fn until_the_action_phase(game: &mut Game, handle: &mut MatchHandle, side: Side) {
        while !matches!(game.view.as_ref().unwrap().phase, GamePhase::Action(s) if s == side) {
            let Outcome::Submit(action) = game.apply(Intent::Choose(0)) else { panic!() };
            handle.submit(action).unwrap();
            game.awaiting = false;
            until_awaiting(game, handle);
        }
    }

    #[test]
    fn an_awaiting_view_offers_every_legal_action_and_a_choice_submits_it() {
        let (mut game, mut handle) = game(Side::Corp);
        until_awaiting(&mut game, &mut handle);
        let view = game.view.as_ref().unwrap();
        assert!(matches!(view.phase, GamePhase::Mulligan(Side::Corp)));
        assert_eq!(game.actions.entries.len(), view.legal_actions.len());
        assert!(game.prompt.as_ref().is_some_and(|p| p.title.contains("mulligan")), "{:?}", game.prompt);
        let keep = game.actions.entries.iter().position(|e| e.action == PlayerAction::KeepHand).unwrap();
        assert_eq!(game.apply(Intent::Choose(keep)), Outcome::Submit(PlayerAction::KeepHand));
        assert!(!game.awaiting, "a submit in flight closes the panel");
        assert_eq!(game.apply(Intent::Choose(keep)), Outcome::Nothing, "and a second click sends nothing");
        handle.submit(PlayerAction::KeepHand).unwrap();
        // The own action comes back as Applied, with a log line and no
        // transition yet the first time (there was no earlier view to
        // diff against; from the second on there is).
        let applied = handle.wait().unwrap();
        assert!(matches!(applied, MatchMessage::Applied { .. }));
        game.apply(Intent::Message(MatchMessageRef(applied)));
        assert_eq!(game.log.len(), 1);
        assert!(game.log[0].text.contains("Keep hand"), "{}", game.log[0].text);
        until_awaiting(&mut game, &mut handle);
        assert!(game.applied >= 2, "the Runner's mulligan decision was applied too");
        handle.join();
    }

    /// A click never submits: a hand card opens its menu with its entries,
    /// a press on one of those submits, a zone opens its menu even with
    /// nothing to do there, the bar submits what the engine lists and
    /// nothing else, and Escape closes things in order.
    #[test]
    fn a_click_opens_a_menu_and_never_submits() {
        let (mut game, mut handle) = game(Side::Corp);
        until_awaiting(&mut game, &mut handle);
        // At the mulligan the bar has nothing: keep and mulligan are the
        // prompt's decisions, not controls.
        assert_eq!(game.apply(Intent::Control(Control::EndTurn)), Outcome::Nothing);
        assert!(game.awaiting, "a control with no entry submits nothing");
        let decisions = game.actions.decisions();
        assert_eq!(decisions.len(), 2, "keep and mulligan: {:?}", game.actions.entries);
        until_the_corps_action_phase(&mut game, &mut handle);
        let view = game.view.clone().unwrap();
        let hand = view.corp.hq_cards.clone().unwrap();
        let over = Anchor { x: 300.0, y: 700.0, width: 120.0, height: 168.0 };
        let mut pressed_one = false;
        for card in &hand {
            let target = Target::HandCard(card.clone());
            let entries = game.actions.for_hand_card(card);
            let outcome = game.apply(Intent::Click { target: target.clone(), over });
            assert_eq!(outcome, Outcome::Redraw, "a click opens, whatever the card can do");
            assert_eq!(game.menu, Some(Menu { target: target.clone(), entries: entries.clone(), over }));
            assert!(game.awaiting && game.sheet.is_none(), "nothing was sent, and nothing to read opened");
            if let (Some(first), false) = (entries.first(), pressed_one) {
                let Outcome::Submit(action) = game.apply(Intent::Choose(*first)) else { panic!("the menu's button submits") };
                assert_eq!(&action, &game.actions.entries[*first].action);
                assert!(game.menu.is_none() && !game.awaiting);
                // Undo the in-flight state for the rest of the loop.
                game.awaiting = true;
                pressed_one = true;
            } else {
                assert_eq!(game.apply(Intent::Back), Outcome::Redraw, "Escape closes the menu");
                assert!(game.menu.is_none() && !game.confirm_quit);
            }
        }
        assert!(pressed_one, "an opening Corp hand has something playable");
        // A second click on the same card closes its menu; a click on
        // another moves it.
        let target = Target::HandCard(hand[0].clone());
        game.apply(Intent::Click { target: target.clone(), over });
        assert_eq!(game.apply(Intent::Click { target: target.clone(), over }), Outcome::Redraw);
        assert!(game.menu.is_none(), "the second click closes it");
        // R&D: the menu offers the draw, and no zone click ever draws.
        let before = game.applied;
        game.apply(Intent::Click { target: Target::Server(ServerId::RnD), over });
        let menu = game.menu.clone().unwrap();
        assert!(menu.entries.iter().any(|i| matches!(game.actions.entries[*i].action, PlayerAction::DrawCardClick { .. })));
        assert!(game.card_of(&menu.target).is_none(), "a zone is not a card");
        assert!(game.awaiting && game.applied == before);
        // Archives offers the installs into it and nothing else; a zone
        // with nothing to do still opens (the identity, say).
        game.apply(Intent::Click { target: Target::Server(ServerId::Archives), over });
        assert_eq!(game.menu.as_ref().map(|m| m.entries.clone()), Some(game.actions.for_server(ServerId::Archives)));
        game.apply(Intent::Click { target: Target::Identity(Side::Runner), over });
        assert_eq!(game.menu.as_ref().map(|m| m.entries.len()), Some(0), "the opponent's identity has nothing to do");
        game.apply(Intent::CloseMenu);
        // The bar: a credit is listed and submits; End turn is not while
        // the Corp has clicks (CR 5.6.2b); Jack out is the Runner's and
        // never on the Corp's map.
        assert_eq!(game.apply(Intent::Control(Control::JackOut)), Outcome::Nothing);
        assert_eq!(game.apply(Intent::Control(Control::EndTurn)), Outcome::Nothing);
        assert_eq!(game.apply(Intent::Control(Control::GainCredit)), Outcome::Submit(PlayerAction::GainCreditClick { side: Side::Corp }));
        game.awaiting = true;
        // The options close before the quit prompt opens; quitting
        // mid-game asks first; Escape again withdraws.
        assert_eq!(game.apply(Intent::ToggleOptions), Outcome::Redraw);
        assert!(game.options_open);
        assert_eq!(game.apply(Intent::Back), Outcome::Redraw);
        assert!(!game.options_open && !game.confirm_quit);
        assert_eq!(game.apply(Intent::Back), Outcome::Redraw);
        assert!(game.confirm_quit);
        assert_eq!(game.apply(Intent::Back), Outcome::Redraw);
        assert!(!game.confirm_quit);
        game.apply(Intent::RequestQuit);
        assert_eq!(game.apply(Intent::ConfirmQuit), Outcome::Quit);
        handle.join();
    }

    /// A secondary click opens the target's sheet to read, with no
    /// entries: a hand card, an install's card and a zone alike. It
    /// closes the menu, a face in a pile reads over it, Escape closes the
    /// reading then the sheet, nothing opens through it, and the board
    /// moving closes a menu but keeps the sheet.
    #[test]
    fn a_secondary_click_opens_a_sheet_to_read_and_never_offers() {
        let (mut game, mut handle) = game(Side::Corp);
        until_awaiting(&mut game, &mut handle);
        until_the_corps_action_phase(&mut game, &mut handle);
        let hand = game.view.as_ref().unwrap().corp.hq_cards.clone().unwrap();
        let (card, entries) = hand.iter().map(|c| (c.clone(), game.actions.for_hand_card(c))).find(|(_, e)| !e.is_empty()).expect("something playable");
        let target = Target::HandCard(card.clone());
        let before = game.applied;
        game.apply(Intent::Click { target: target.clone(), over: Anchor::default() });
        assert_eq!(game.apply(Intent::Inspect(target.clone())), Outcome::Redraw);
        assert_eq!(game.sheet, Some(Sheet { target: target.clone() }));
        assert_eq!(game.card_of(&target).as_ref(), Some(&card));
        assert!(game.menu.is_none(), "reading closes the menu");
        assert!(game.awaiting && game.applied == before, "nothing was sent");
        // Through the sheet, neither click opens anything.
        assert_eq!(game.apply(Intent::Click { target: target.clone(), over: Anchor::default() }), Outcome::Nothing);
        assert_eq!(game.apply(Intent::Inspect(Target::Server(ServerId::RnD))), Outcome::Nothing);
        assert!(game.menu.is_none() && game.sheet == Some(Sheet { target: target.clone() }));
        // A face in a pile reads over the sheet; Escape closes the
        // reading first, then the sheet.
        game.apply(Intent::InspectCard(Some(hand[0].clone())));
        assert_eq!(game.apply(Intent::Back), Outcome::Redraw);
        assert!(game.inspecting.is_none() && game.sheet.is_some());
        game.apply(Intent::Back);
        assert!(game.sheet.is_none());
        // A zone reads as its contents; the opponent's identity as a card.
        game.apply(Intent::Inspect(Target::Server(ServerId::Archives)));
        assert!(game.sheet.as_ref().is_some_and(|s| game.card_of(&s.target).is_none()), "a zone is not a card");
        game.apply(Intent::Back);
        game.apply(Intent::Inspect(Target::Identity(Side::Runner)));
        assert!(game.card_of(&Target::Identity(Side::Runner)).is_some(), "the identity is a card to read");
        // The board moving keeps the sheet (it is worth reading still)
        // and closes a menu (it sat over a card that may be gone).
        let Outcome::Submit(action) = game.apply(Intent::Choose(entries[0])) else { panic!() };
        handle.submit(action).unwrap();
        game.apply(Intent::Inspect(target.clone()));
        game.menu = Some(Menu { target, entries: Vec::new(), over: Anchor::default() });
        until_awaiting(&mut game, &mut handle);
        assert!(game.menu.is_none(), "an applied action closes the menu");
        assert!(game.sheet.is_some(), "and leaves the sheet");
        handle.join();
    }

    /// Which panels a click that misses them closes.
    ///
    /// The rule is not "every overlay": a form keeps its Close because a
    /// setting lost to a stray click is worse than a button nobody
    /// needed, and the three panels that are *asking* something have
    /// nowhere to dismiss to — Escape does not close them either, so a
    /// wash that did would be the only way out and would mean something
    /// different there from everywhere else.
    #[test]
    fn a_click_away_closes_a_reading_surface_and_leaves_a_question_standing() {
        let mut game = Game::new(Arc::new(netrunner_client::decks::sample_deck_registry()), Side::Runner);
        assert!(!game.dismissed_by_a_click_away(), "nothing is open");

        game.sheet = Some(Sheet { target: Target::Server(ServerId::RnD) });
        assert!(game.dismissed_by_a_click_away(), "a sheet closes");
        game.inspecting = Some(CardId("01001".into()));
        assert!(game.dismissed_by_a_click_away(), "a card read over it closes");
        game.sheet = None;
        assert!(game.dismissed_by_a_click_away(), "a card read on its own closes");
        game.inspecting = None;

        // The forms keep their Close, so the wash is inert over them.
        game.options_open = true;
        assert!(!game.dismissed_by_a_click_away(), "the options are a form");
        game.options_open = false;
        game.help_open = true;
        assert!(!game.dismissed_by_a_click_away(), "the list of keys is a form");
        game.help_open = false;

        // And the questions stand: note each of these is true even with a
        // sheet underneath, because the panel on top is the one asking.
        game.sheet = Some(Sheet { target: Target::Server(ServerId::RnD) });
        game.confirm_quit = true;
        assert!(!game.dismissed_by_a_click_away(), "the quit prompt is asking");
        game.confirm_quit = false;
        game.stalled = Some("the match stopped".to_string());
        assert!(!game.dismissed_by_a_click_away(), "a stall has nowhere to dismiss to");
        game.stalled = None;
        assert!(game.dismissed_by_a_click_away(), "and the sheet under them still closes");
    }

    /// The timing chart is a reading surface: T or the phase panel opens
    /// it, it covers the board, a click away or Escape closes it, and it
    /// does not open over another panel.
    #[test]
    fn the_timing_chart_opens_on_t_and_closes_like_a_sheet() {
        let (mut game, mut handle) = game(Side::Corp);
        until_awaiting(&mut game, &mut handle);
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Timing)), Outcome::Redraw);
        assert!(game.timing_open && game.covered());
        assert!(game.dismissed_by_a_click_away(), "a reading surface closes on a click away");
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Decision(0))), Outcome::Nothing, "no key reaches through it");
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Timing)), Outcome::Redraw);
        assert!(!game.timing_open);
        game.apply(Intent::ToggleTiming);
        assert_eq!(game.apply(Intent::Back), Outcome::Redraw);
        assert!(!game.timing_open && !game.confirm_quit, "Escape closes it and asks nothing");
        game.help_open = true;
        assert_eq!(game.apply(Intent::ToggleTiming), Outcome::Nothing, "not over the list of keys");
        assert!(!game.timing_open);
    }

    /// A key is the button it stands for: at the mulligan Space and C do
    /// nothing and 1 keeps (the pop-up's first button); on the Corp's turn
    /// C takes the credit, a digit presses the open menu's button, Enter
    /// with clicks left does nothing because the engine does not list End
    /// turn then (CR 5.6.2b);
    /// the list of keys opens and closes on its own key and on Escape, and
    /// no other key acts while the board is covered.
    #[test]
    fn a_key_presses_the_button_it_stands_for_and_nothing_through_an_overlay() {
        let (mut game, mut handle) = game(Side::Corp);
        until_awaiting(&mut game, &mut handle);
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Control(Control::Continue))), Outcome::Nothing, "nothing to pass at the mulligan");
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Control(Control::GainCredit))), Outcome::Nothing);
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Decision(7))), Outcome::Nothing, "no eighth button");
        let first = game.actions.decisions()[0];
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Decision(0))), Outcome::Submit(game.actions.entries[first].action.clone()));
        game.awaiting = true;
        // The list of keys: it covers the board, its own key and Escape
        // close it.
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Help)), Outcome::Redraw);
        assert!(game.help_open && game.covered());
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Decision(0))), Outcome::Nothing, "no key reaches through it");
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Help)), Outcome::Redraw);
        assert!(!game.help_open);
        game.apply(Intent::Shortcut(Shortcut::Help));
        assert_eq!(game.apply(Intent::Back), Outcome::Redraw);
        assert!(!game.help_open && !game.confirm_quit, "Escape closes the list and asks nothing");
        until_the_corps_action_phase(&mut game, &mut handle);
        assert!(game.clicks_left() > 0);
        // C is Take 1 credit.
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Control(Control::GainCredit))), Outcome::Submit(PlayerAction::GainCreditClick { side: Side::Corp }));
        game.awaiting = true;
        // A digit presses the open menu's button, not the pop-up's.
        let hand = game.view.as_ref().unwrap().corp.hq_cards.clone().unwrap();
        let (card, entries) = hand.iter().map(|c| (c.clone(), game.actions.for_hand_card(c))).find(|(_, e)| !e.is_empty()).expect("something playable");
        game.apply(Intent::Click { target: Target::HandCard(card), over: Anchor::default() });
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Decision(0))), Outcome::Submit(game.actions.entries[entries[0]].action.clone()));
        assert!(game.menu.is_none());
        game.awaiting = true;
        // Enter is the bar's End turn, and with clicks left the engine
        // does not offer one (CR 5.6.2b): nothing to ask twice about.
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Control(Control::EndTurn))), Outcome::Nothing);
        assert!(game.awaiting, "and nothing was sent");
        // Tab opens a score area; under it, no key acts.
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::ScoreArea(Side::Runner))), Outcome::Redraw);
        assert_eq!(game.sheet.as_ref().map(|s| s.target.clone()), Some(Target::Pile(Pile::Agendas(Side::Runner))));
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Control(Control::GainCredit))), Outcome::Nothing);
        handle.join();
    }

    /// The hand keeps the person's order: a card dragged along it moves,
    /// a drawn card joins the end, a played one drops out, and two copies
    /// are interchangeable. The view stays the authority on what is in
    /// hand.
    #[test]
    fn the_hand_keeps_the_persons_order_across_draws_and_plays() {
        let card = |name: &str| CardId(name.to_string());
        let mut hand = HandOrder::default();
        hand.sync(&[card("a"), card("b"), card("c")]);
        assert_eq!(hand.cards(), [card("a"), card("b"), card("c")]);
        // Dropped before the first card: the row shifts along.
        assert!(hand.move_card(2, 0));
        assert_eq!(hand.cards(), [card("c"), card("a"), card("b")]);
        // Dropped where it already is: nothing moved, and nothing redraws.
        assert!(!hand.move_card(0, 0));
        assert!(!hand.move_card(0, 1), "before the card after it is where it is");
        // Dropped past the end.
        assert!(hand.move_card(0, 3));
        assert_eq!(hand.cards(), [card("a"), card("b"), card("c")]);
        assert!(!hand.move_card(9, 0), "a slot that is not there moves nothing");
        // A draw joins the end; the order that is left is kept.
        hand.sync(&[card("c"), card("a"), card("b"), card("d")]);
        assert_eq!(hand.cards(), [card("a"), card("b"), card("c"), card("d")]);
        // A play drops out, and the rest keep their places.
        hand.sync(&[card("a"), card("c"), card("d")]);
        assert_eq!(hand.cards(), [card("a"), card("c"), card("d")]);
        // Two copies: one leaves and the other stays.
        hand.sync(&[card("a"), card("a"), card("c"), card("d")]);
        assert_eq!(hand.cards(), [card("a"), card("c"), card("d"), card("a")], "the second copy joins the end");
        hand.sync(&[card("a"), card("c"), card("d")]);
        assert_eq!(hand.cards(), [card("a"), card("c"), card("d")]);
        // New cards join in the view's own order, not a scrambled one.
        let mut fresh = HandOrder::default();
        fresh.sync(&[card("a"), card("b"), card("c"), card("d")]);
        assert_eq!(fresh.cards(), [card("a"), card("b"), card("c"), card("d")]);
    }

    /// A press on a hand card is armed: released still it is the click that
    /// opens the menu, released after travelling it drops the card into the
    /// row, and either way it never submits.
    #[test]
    fn a_hand_cards_press_is_a_click_when_still_and_a_drag_when_moved() {
        let (mut game, mut handle) = game(Side::Corp);
        until_awaiting(&mut game, &mut handle);
        let hand = game.view.as_ref().unwrap().corp.hq_cards.clone().unwrap();
        assert_eq!(game.hand.cards(), hand.as_slice(), "the view's order until the person changes it");
        let slots: Vec<f32> = (0..hand.len()).map(|n| 100.0 * n as f32).collect();
        let before = game.applied;
        // Still: the click that opens the menu.
        assert_eq!(game.apply(Intent::DragPress { slot: 1, at: (100.0, 500.0) }), Outcome::Nothing, "the press alone opens nothing");
        assert!(game.menu.is_none() && game.dragging.is_some());
        assert_eq!(game.apply(Intent::DragMove { at: (102.0, 501.0) }), Outcome::Nothing, "a wobble is not a drag");
        assert_eq!(game.dragged_slot(), None);
        assert_eq!(game.apply(Intent::DragRelease { over: Anchor::default(), slots: slots.clone() }), Outcome::Redraw);
        assert_eq!(game.menu.as_ref().map(|m| m.target.clone()), Some(Target::HandCard(hand[1].clone())));
        assert!(game.dragging.is_none() && game.applied == before, "and nothing was sent");
        assert_eq!(game.hand.cards(), hand.as_slice(), "the order is untouched");
        game.apply(Intent::CloseMenu);
        // Moved: the card is dropped into the row, and the card lifted is
        // known while the pointer is down.
        game.apply(Intent::DragPress { slot: 2, at: (200.0, 500.0) });
        assert_eq!(game.apply(Intent::DragMove { at: (20.0, 500.0) }), Outcome::Redraw, "the move that starts the drag redraws once");
        assert_eq!(game.dragged_slot(), Some(2));
        // Left of the first card's centre, which is where it is dropped.
        assert_eq!(game.apply(Intent::DragMove { at: (-10.0, 500.0) }), Outcome::Nothing, "and not again");
        assert_eq!(game.apply(Intent::DragRelease { over: Anchor::default(), slots }), Outcome::Redraw);
        let mut expected = hand.clone();
        let moved = expected.remove(2);
        expected.insert(0, moved);
        assert_eq!(game.hand.cards(), expected.as_slice(), "dropped left of the first card");
        assert!(game.menu.is_none(), "a drag opens no menu");
        assert!(game.dragging.is_none() && game.applied == before);
        handle.join();
    }

    /// A card dragged onto a place the board lit for it is played there:
    /// one entry submits, a place that offers two asks with a menu, and a
    /// drop on a place the card does not name puts it back.
    #[test]
    fn a_card_dropped_on_a_lit_place_is_played_there() {
        let (mut game, mut handle) = game(Side::Corp);
        until_awaiting(&mut game, &mut handle);
        until_the_action_phase(&mut game, &mut handle, Side::Corp);
        // A card the engine offers an install for, and where it may go.
        let hand = game.hand.cards().to_vec();
        let (slot, card, places) = hand
            .iter()
            .enumerate()
            .find_map(|(slot, card)| {
                let places = game.actions.destinations_for_hand_card(card);
                places.iter().any(|place| matches!(place, Target::Server(_))).then(|| (slot, card.clone(), places))
            })
            .expect("an opening Corp hand has something to install");
        let place = places[0].clone();
        // Nothing is lit until the press has become a drag.
        assert!(game.drop_places().is_empty(), "nothing is held");
        game.apply(Intent::DragPress { slot, at: (100.0, 900.0) });
        assert!(game.drop_places().is_empty(), "a press alone lights nothing");
        game.apply(Intent::DragMove { at: (400.0, 400.0) });
        assert_eq!(game.drop_places(), places, "the card's own places are lit");
        assert!(!places.contains(&Target::Table), "an install is not played on the table");
        let expected = game.actions.for_hand_card_at(&card, &place);
        let outcome = game.apply(Intent::DragDrop { target: place.clone(), over: Anchor::default() });
        match expected.as_slice() {
            [index] => assert_eq!(outcome, Outcome::Submit(game.actions.entries[*index].action.clone())),
            several => {
                assert_eq!(outcome, Outcome::Redraw);
                assert_eq!(game.menu.as_ref().map(|m| m.entries.clone()), Some(several.to_vec()), "a place with two ways asks");
                assert_eq!(game.menu.as_ref().map(|m| m.target.clone()), Some(place));
            }
        }
        assert!(game.dragging.is_none(), "the card was let go");
        assert!(game.drop_places().is_empty());
        // An operation lights the table, and nothing else: a drop on a
        // server puts it back, and a drop on the table plays it.
        game.awaiting = true;
        game.menu = None;
        let operation = hand.iter().enumerate().find(|(_, card)| game.actions.for_hand_card(card).iter().any(|&i| matches!(game.actions.entries[i].action, PlayerAction::PlayOperation { .. })));
        let (slot, card) = operation.expect("the opening Corp hand has an operation it can play");
        game.apply(Intent::DragPress { slot, at: (100.0, 900.0) });
        game.apply(Intent::DragMove { at: (400.0, 400.0) });
        assert_eq!(game.drop_places(), vec![Target::Table], "{} is played on the table", card.0);
        assert_eq!(game.apply(Intent::DragDrop { target: Target::Server(ServerId::Archives), over: Anchor::default() }), Outcome::Redraw);
        assert!(game.dragging.is_none() && game.menu.is_none(), "a drop it does not name puts it back");
        game.apply(Intent::DragPress { slot, at: (100.0, 900.0) });
        game.apply(Intent::DragMove { at: (400.0, 400.0) });
        assert_eq!(game.apply(Intent::DragDrop { target: Target::Table, over: Anchor::default() }), Outcome::Submit(PlayerAction::PlayOperation { card_id: card.clone() }));
        handle.join();
    }

    /// The Runner's installs are dropped on the rig and their events on
    /// the table, and neither is lit for the other's place.
    #[test]
    fn a_runner_drops_an_install_on_the_rig_and_an_event_on_the_table() {
        let (mut game, mut handle) = game(Side::Runner);
        until_awaiting(&mut game, &mut handle);
        until_the_action_phase(&mut game, &mut handle, Side::Runner);
        let hand = game.hand.cards().to_vec();
        let kind = |game: &Game, card: &CardId| {
            game.actions.for_hand_card(card).into_iter().map(|i| game.actions.entries[i].action.clone()).find(|action| {
                matches!(
                    action,
                    PlayerAction::PlayEvent { .. } | PlayerAction::InstallProgram { trash_first: false, .. } | PlayerAction::InstallHardware { .. } | PlayerAction::InstallResource { .. }
                )
            })
        };
        let mut seen = (false, false);
        for (slot, card) in hand.iter().enumerate() {
            let Some(action) = kind(&game, card) else { continue };
            let (place, other) = match action {
                PlayerAction::PlayEvent { .. } => (Target::Table, Target::Rig),
                _ => (Target::Rig, Target::Table),
            };
            game.awaiting = true;
            game.apply(Intent::DragPress { slot, at: (100.0, 900.0) });
            game.apply(Intent::DragMove { at: (400.0, 400.0) });
            let lit = game.drop_places();
            assert!(lit.contains(&place) && !lit.contains(&other), "{}: {lit:?}", card.0);
            // The one entry that lands there, or a menu of them (a program
            // that may trash first).
            match game.apply(Intent::DragDrop { target: place.clone(), over: Anchor::default() }) {
                Outcome::Submit(submitted) => assert_eq!(submitted, action),
                Outcome::Redraw => assert!(game.menu.as_ref().is_some_and(|menu| menu.target == place && menu.entries.len() > 1)),
                other => panic!("{other:?}"),
            }
            game.menu = None;
            if place == Target::Table { seen.0 = true } else { seen.1 = true }
        }
        assert!(seen.0 && seen.1, "the opening Runner hand has something to play and something to install: {seen:?}");
        handle.join();
    }

    /// A click on the heap opens its contents, from either chair and
    /// whether or not the seat is on priority: the heap is public, and
    /// no action is ever on it, so its menu could only ever be empty.
    #[test]
    fn a_click_on_the_heap_opens_every_card_in_it() {
        let heap = Target::Pile(netrunner_client::board::Pile::Heap);
        for chair in [Side::Runner, Side::Corp] {
            for awaiting in [false, true] {
                let mut game = Game::new(Arc::new(netrunner_client::decks::sample_deck_registry()), chair);
                game.awaiting = awaiting;
                assert_eq!(game.apply(Intent::Click { target: heap.clone(), over: Anchor::default() }), Outcome::Redraw);
                assert_eq!(game.sheet.as_ref().map(|s| s.target.clone()), Some(heap.clone()), "{chair:?}, awaiting {awaiting}");
                assert!(game.menu.is_none(), "a sheet, not an empty menu");
            }
        }
    }

    /// The score area opens as a sheet from the HUD, one row's details
    /// open at a time, a second press on the open row closes it, and a
    /// sheet opened afresh starts with every row closed.
    #[test]
    fn a_score_area_row_expands_in_place_one_at_a_time() {
        let mut game = Game::new(Arc::new(netrunner_client::decks::sample_deck_registry()), Side::Runner);
        assert_eq!(game.apply(Intent::Expand(0)), Outcome::Nothing, "no sheet, nothing to expand");
        let agendas = Target::Pile(netrunner_client::board::Pile::Agendas(Side::Corp));
        assert_eq!(game.apply(Intent::Click { target: agendas.clone(), over: Anchor::default() }), Outcome::Redraw, "the readout opens the score area");
        assert_eq!(game.sheet.as_ref().map(|s| s.target.clone()), Some(agendas.clone()));
        game.apply(Intent::Expand(1));
        assert_eq!(game.expanded, Some(1));
        game.apply(Intent::Expand(0));
        assert_eq!(game.expanded, Some(0), "one row at a time");
        game.apply(Intent::Expand(0));
        assert_eq!(game.expanded, None, "the open row's second press closes it");
        game.apply(Intent::Expand(2));
        game.apply(Intent::Back);
        game.apply(Intent::Inspect(agendas));
        assert_eq!(game.expanded, None, "a sheet opens with every row closed");
    }

    #[test]
    fn a_rejection_reopens_the_panel_and_an_end_closes_it() {
        let (mut game, mut handle) = game(Side::Runner);
        until_awaiting(&mut game, &mut handle);
        game.apply(Intent::Choose(0));
        assert!(!game.awaiting);
        game.apply(Intent::Message(MatchMessageRef(MatchMessage::Rejected { reason: "no".to_string() })));
        assert!(game.awaiting);
        assert_eq!(game.rejection.as_deref(), Some("no"));
        let view = Box::new(game.view.clone().unwrap());
        game.apply(Intent::Message(MatchMessageRef(MatchMessage::Ended { winner: Side::Corp, reason: GameEndReason::AgendaThreshold, view, report: None, notice: None })));
        assert!(game.finished() && !game.awaiting && game.actions.is_empty());
        assert_eq!(game.apply(Intent::RequestQuit), Outcome::Quit, "no confirmation once it is over");
        handle.join();
    }

    /// A decision whose only action is a pass is submitted as it arrives
    /// and never opens the bar; a rejection of it reopens the bar with
    /// the pass on it, so a person is never left with nothing.
    #[test]
    fn a_lone_pass_is_submitted_without_a_click() {
        let (mut game, mut handle) = game(Side::Runner);
        let pass = loop {
            let message = handle.wait().expect("the match is alive");
            let lone = matches!(&message, MatchMessage::Awaiting { view } if view.legal_actions.len() == 1 && matches!(view.legal_actions[0], PlayerAction::PassPriority { .. }));
            match game.apply(Intent::Message(MatchMessageRef(message))) {
                Outcome::Submit(pass) => {
                    assert!(lone, "only a lone pass is taken for the person");
                    break pass;
                }
                _ if game.awaiting => {
                    assert!(!lone);
                    let first = game.apply(Intent::Choose(0));
                    let Outcome::Submit(action) = first else { panic!("{first:?}") };
                    handle.submit(action).unwrap();
                }
                _ => assert!(!lone),
            }
        };
        assert_eq!(pass, PlayerAction::PassPriority { side: Side::Runner });
        assert!(!game.awaiting, "nothing is offered while the pass is in flight");
        game.apply(Intent::Message(MatchMessageRef(MatchMessage::Rejected { reason: "no".to_string() })));
        assert!(game.awaiting && game.actions.for_control(Control::Continue).is_some(), "a rejected pass is back on the bar");
        handle.join();
    }

    /// The Corp's own move, from the entries on offer: an install before
    /// anything else and never a rez, so it holds unrezzed ICE when the
    /// Runner runs and a window's pass sits beside a choice; a selection
    /// confirmed as soon as it can be.
    fn corp_move(game: &Game) -> usize {
        let entries = &game.actions.entries;
        let find = |wanted: &dyn Fn(&PlayerAction) -> bool| entries.iter().position(|entry| wanted(&entry.action));
        let parked = game.view.as_ref().is_some_and(|view| view.pending_decision.is_some());
        if parked {
            return find(&|action| matches!(action, PlayerAction::ConfirmCardSelection)).unwrap_or(0);
        }
        find(&|action| matches!(action, PlayerAction::InstallCard { .. })).or_else(|| find(&|action| !matches!(action, PlayerAction::RezIce { .. }))).unwrap_or(0)
    }

    /// Plays the Corp until the rail offers to pass the rest of a run.
    fn until_a_run_pass_is_offered(game: &mut Game, handle: &mut MatchHandle) {
        loop {
            until_awaiting(game, handle);
            if game.run_pass_label() == Some("Pass for the rest of this run") {
                return;
            }
            let Outcome::Submit(action) = game.apply(Intent::Choose(corp_move(game))) else { panic!("an entry submits") };
            handle.submit(action).unwrap();
        }
    }

    /// One press passes this window and every later one of the run with
    /// no click, the button turns into the way to stop, and the run's end
    /// turns it off — so the next run offers it again.
    #[test]
    fn the_corp_passes_the_rest_of_a_run_with_one_press() {
        let (mut game, mut handle) = game(Side::Corp);
        let mut taken = 0;
        // Run after run, until one has a window after the pressed one.
        while taken == 0 {
            until_a_run_pass_is_offered(&mut game, &mut handle);
            let Outcome::Submit(pass) = game.apply(Intent::PassTheRun) else { panic!("the press passes this window") };
            assert_eq!(pass, PlayerAction::PassPriority { side: Side::Corp });
            assert_eq!(game.run_pass_label(), Some("Stop passing: ask me again this run"));
            handle.submit(pass).unwrap();
            while game.view.as_ref().is_some_and(|view| view.active_run.is_some()) {
                let message = handle.wait().expect("the match is alive");
                match game.apply(Intent::Message(MatchMessageRef(message))) {
                    Outcome::Submit(action) => {
                        if matches!(action, PlayerAction::PassPriority { .. }) {
                            taken += 1;
                        }
                        handle.submit(action).unwrap();
                    }
                    // A question the pass does not answer is still asked.
                    _ if game.awaiting => {
                        let view = game.view.as_ref().unwrap();
                        assert!(view.active_run.is_none() || view.pending_decision.is_some() || view.pending_paid_choice.is_some(), "a window was left to the person");
                        let Outcome::Submit(action) = game.apply(Intent::Choose(corp_move(&game))) else { panic!() };
                        handle.submit(action).unwrap();
                    }
                    _ => {}
                }
            }
            assert!(!game.run_pass.is_on(), "it ends with the run");
        }
        // And the next run is asked about afresh.
        until_a_run_pass_is_offered(&mut game, &mut handle);
        handle.join();
    }

    /// Pressed again while it is on, it stops, and the next window asks.
    #[test]
    fn a_run_pass_stops_on_a_second_press() {
        let (mut game, mut handle) = game(Side::Corp);
        until_a_run_pass_is_offered(&mut game, &mut handle);
        let Outcome::Submit(pass) = game.apply(Intent::PassTheRun) else { panic!() };
        assert_eq!(game.apply(Intent::PassTheRun), Outcome::Redraw);
        assert!(!game.run_pass.is_on());
        handle.submit(pass).unwrap();
        handle.join();
    }

    /// From the second view on, an applied action yields transitions
    /// the screen can highlight, and they are handed over once.
    #[test]
    fn applied_actions_accumulate_transitions_until_taken() {
        let (mut game, mut handle) = game(Side::Runner);
        until_awaiting(&mut game, &mut handle);
        let mut total = 0;
        for _ in 0..3 {
            let Outcome::Submit(action) = game.apply(Intent::Choose(0)) else { panic!() };
            handle.submit(action).unwrap();
            until_awaiting(&mut game, &mut handle);
            total += game.take_transitions().len();
            assert!(game.transitions.is_empty());
        }
        assert!(total > 0, "three decisions in, something moved");
        handle.join();
    }

    /// A card-selection prompt, as the pop-up draws it: a button per card by
    /// name — the person's report was `Toggle selection of card N` — with a
    /// second copy folded into the first's, the chosen cards named under
    /// the prompt, and, once the one card allowed is chosen, "Confirm" and
    /// "Select a different card", in that order.
    #[test]
    fn a_card_selection_pops_up_its_cards_by_name() {
        use netrunner_core::dsl::{CardFilter, CardZoneRef};
        use netrunner_core::rules::{GameState, PendingChoiceResume, PendingDecision};
        use netrunner_core::view::build_client_view;

        let registry = Arc::new(netrunner_client::decks::sample_deck_registry());
        let corp = netrunner_core::decks::by_id("discretion_advised").unwrap().to_deck();
        let runner = netrunner_core::decks::by_id("stolen_goods").unwrap().to_deck();
        let (mut state, _) = GameState::setup(&corp, &runner, &registry, 11).unwrap();
        state.phase = GamePhase::Action(Side::Runner);
        let first = state.runner.grip[0].clone();
        let other = state.runner.grip.iter().find(|c| **c != first).cloned().expect("an opening grip of two titles");
        state.runner.grip = vec![first.clone(), other.clone(), first.clone()];
        let title = |id: &CardId| registry.get(id).unwrap().title.clone();
        let parked = |selected: Vec<usize>| PendingDecision::ChooseCards {
            side: Side::Runner,
            source: CardZoneRef::OwnGrip,
            filter: CardFilter::Any,
            min: 1,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: None,
            selected,
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        };
        let mut game = Game::new(registry.clone(), Side::Runner);
        let labels = |game: &Game| game.actions.decisions().iter().map(|i| game.actions.entries[*i].label.clone()).collect::<Vec<_>>();

        state.pending_decision = Some(parked(Vec::new()));
        let view = build_client_view(&state, &registry, Side::Runner);
        game.apply(Intent::Message(MatchMessageRef(MatchMessage::Awaiting { view: Box::new(view) })));
        assert_eq!(labels(&game), vec![format!("Select {}", title(&first)), format!("Select {}", title(&other))], "the second copy is the first one's button");
        assert_eq!(game.prompt.as_ref().map(|p| p.detail.as_str()), Some("Nothing selected yet"));
        assert_eq!(game.actions.entries.len(), 3, "the collapsed copy is still an entry, for the play helper");

        state.pending_decision = Some(parked(vec![1]));
        let view = build_client_view(&state, &registry, Side::Runner);
        game.apply(Intent::Message(MatchMessageRef(MatchMessage::Awaiting { view: Box::new(view) })));
        assert_eq!(labels(&game), vec![format!("Confirm {}", title(&other)), "Select a different card".to_string()]);
        assert_eq!(game.prompt.as_ref().map(|p| p.detail.clone()), Some(format!("Selected: {}", title(&other))));
    }

    /// Scatter Field's "You may install 1 card from HQ", from the Corp's
    /// chair with an asset already earning credits in Remote 0: the choice
    /// of server is under the prompt as buttons — the new remote among them,
    /// which has no column on the board to click — and each says what it
    /// does. The person's report was that overwriting looked like the only
    /// way; the new remote was offered and nowhere to be seen.
    #[test]
    fn a_card_effect_install_pops_up_where_it_can_go() {
        use netrunner_core::dsl::{CardDefinition, CardFilter, CardType, CardZoneRef, Effect, IceType};
        use netrunner_core::rules::{apply_action, GameState, InstallSlot, InstalledCard, PendingChoiceResume, PendingDecision};
        use netrunner_core::view::build_client_view;

        let def = |id: &str, title: &str, card_type: CardType| CardDefinition { id: CardId(id.to_string()), title: title.to_string(), side: Side::Corp, card_type, is_playable: true, ..Default::default() };
        let registry = Arc::new(CardRegistry::from_cards(vec![
            def("nico_campaign", "Nico Campaign", CardType::Asset),
            def("pad_campaign", "PAD Campaign", CardType::Asset),
            def("scatter_field", "Scatter Field", CardType::Ice(IceType::CodeGate)),
        ]));
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.hq = vec![CardId("pad_campaign".to_string())];
        state.corp.installed = vec![InstalledCard { install_id: InstallId(1), card: CardId("nico_campaign".to_string()), server: ServerId::Remote(0), slot: InstallSlot::Root, rezzed: true, ..Default::default() }];
        state.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Corp,
            source: CardZoneRef::OwnHq,
            filter: CardFilter::Any,
            min: 0,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: Some(Box::new(Effect::PromptInstallCorpCard { origin_zone: CardZoneRef::OwnHq, ignore_costs: false, discount: 0, then: None, remote_only: false })),
            selected: Vec::new(),
            source_card: Some(CardId("scatter_field".to_string())),
            prompting_card: Some(CardId("scatter_field".to_string())),
            source_install: None,
            resume: PendingChoiceResume::None,
        });
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).unwrap();
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).unwrap();

        let mut game = Game::new(registry.clone(), Side::Corp);
        game.apply(Intent::Message(MatchMessageRef(MatchMessage::Awaiting { view: Box::new(build_client_view(&state, &registry, Side::Corp)) })));
        let labels: Vec<String> = game.actions.decisions().iter().map(|i| game.actions.entries[*i].label.clone()).collect();
        assert_eq!(labels, vec!["Install in Remote 0 — trashes Nico Campaign".to_string(), "Install in a new remote server".to_string()]);
        let prompt = game.prompt.as_ref().expect("an install is a prompt");
        assert_eq!(prompt.title, "Scatter Field: where to install PAD Campaign?");
        assert!(prompt.detail.starts_with("A new remote server is an option."), "{}", prompt.detail);
    }

    /// Mid-encounter with Wall of Static (a strength-3 barrier) and a
    /// Corroder: the rail offers the route with its price, the number key
    /// starts it, and every later step goes back without a press on the
    /// view where the Runner has priority again — the engine applying each
    /// one and the Corp passing between, as the match thread does. The
    /// last break leaves the person on the encounter with nothing pending
    /// and their controls back.
    #[test]
    fn a_route_through_the_ice_is_one_press_and_runs_one_step_a_view() {
        use netrunner_core::dsl::{CardType, CardId};
        use netrunner_core::rules::{
            apply_action, EncounteredSubroutine, GameState, InstallSlot, InstalledCard, InstalledRunnerCard, PaidAbilityWindow, RunIce,
            RunPhase, RunState, ServerId, SubroutineStatus, WindowCheckpoint,
        };
        use netrunner_core::view::build_client_view;

        let registry = Arc::new(netrunner_client::decks::sample_deck_registry());
        let wall = registry.get(&CardId("wall_of_static".to_string())).unwrap();
        let CardType::Ice(ice_type) = wall.card_type else { panic!() };
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.credits.0 = 5;
        state.runner.rig.push(InstalledRunnerCard { card: CardId("corroder".to_string()), install_id: InstallId(100), base_strength: 2, ..Default::default() });
        state.corp.installed.push(InstalledCard { install_id: InstallId(1), card: wall.id.clone(), server: ServerId::Hq, slot: InstallSlot::Ice, rezzed: true, ..Default::default() });
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce {
                card_id: wall.id.clone(),
                install_id: InstallId(1),
                ice_type,
                subroutines: wall.subroutines.iter().enumerate().map(|(id, sub)| EncounteredSubroutine { id, definition: sub.clone(), status: SubroutineStatus::Pending }).collect(),
                rezzed: true,
            }],
            ..Default::default()
        });
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::Run,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
        });
        let awaiting = |game: &mut Game, state: &GameState| {
            let view = build_client_view(state, &registry, Side::Runner);
            game.apply(Intent::Message(MatchMessageRef(MatchMessage::Awaiting { view: Box::new(view) })))
        };

        let mut game = Game::new(registry.clone(), Side::Runner);
        assert_eq!(awaiting(&mut game, &state), Outcome::Redraw);
        // The rail's marks, before anything is broken.
        let met = game.encounter().expect("the run is encountering the wall");
        assert_eq!(met.card.as_ref().map(|id| id.0.as_str()), Some("wall_of_static"));
        assert_eq!(met.strength, 3);
        assert_eq!(met.strength_line(&registry), "Strength 3", "unmoved, so no printed number beside it");
        assert_eq!(met.server, ServerId::Hq, "the panel's heading");
        assert_eq!(met.subroutines.iter().map(|sub| (sub.text.as_str(), sub.word())).collect::<Vec<_>>(), [("End the run.", "pending")]);
        assert_eq!(game.breaks.len(), 1);
        let view = game.view.clone().unwrap();
        assert_eq!(game.breaks[0].label(&view, &registry), "Break Wall of Static with Corroder · 2 credits");

        let mut outcome = game.apply(Intent::Shortcut(Shortcut::Decision(0)));
        let mut steps = 0;
        while let Outcome::Submit(action) = outcome {
            steps += 1;
            assert!(steps <= 2, "one pump and one break");
            state = apply_action(&state, &registry, action).expect("each step is legal").0;
            state = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).unwrap().0;
            outcome = awaiting(&mut game, &state);
        }
        assert_eq!(steps, 2);
        assert_eq!(state.runner.resources.credits.0, 3, "the price on the button");
        assert!(game.awaiting && game.breaking.is_none() && game.breaks.is_empty(), "done: the person's controls, and nothing left to break");
        assert!(game.break_stopped.is_none());
        // And the same line now reads broken: the state of the encounter
        // off the rail, without the log.
        let met = game.encounter().expect("still encountering — breaking a subroutine does not pass the ice");
        assert_eq!(met.subroutines.iter().map(|sub| (sub.text.as_str(), sub.word())).collect::<Vec<_>>(), [("End the run.", "broken")]);
    }

    /// Red Team's entry on its menu says which servers it has left, before
    /// the click is spent (Phase 7 §8 item 4a) — the report was a person
    /// finding out by paying.
    #[test]
    fn a_card_that_opens_on_a_choice_says_what_it_will_ask() {
        use netrunner_core::dsl::CardId;
        use netrunner_core::rules::{GameState, InstalledRunnerCard, ServerId};
        use netrunner_core::view::build_client_view;

        let registry = Arc::new(netrunner_client::decks::sample_deck_registry());
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks.0 = 3;
        state.runner.rig.push(InstalledRunnerCard { card: CardId("red_team".to_string()), install_id: InstallId(100), counters: 12, ..Default::default() });
        state.runner.servers_run_this_turn.push(ServerId::Archives);

        let mut game = Game::new(registry.clone(), Side::Runner);
        let view = build_client_view(&state, &registry, Side::Runner);
        game.apply(Intent::Message(MatchMessageRef(MatchMessage::Awaiting { view: Box::new(view) })));
        let entries = game.entries_for(&Target::Install(InstallId(100)));
        let labels: Vec<&str> = entries.iter().map(|index| game.actions.entries[*index].label.as_str()).collect();
        assert_eq!(labels.len(), 1, "{labels:?}");
        assert!(labels[0].ends_with(" — then asks: Run on HQ / Run on R&D"), "{}", labels[0]);
    }

    /// Every take-back goes at once, whichever kind the session says it
    /// is, and the log loses what no longer happened. An undo past
    /// something newly seen once asked twice, because it cost the game
    /// its rating; a game against a bot has none to lose.
    #[test]
    fn every_take_back_goes_at_once() {
        use netrunner_client::play::Rewind;
        use netrunner_core::rules::GameState;
        use netrunner_core::view::build_client_view;

        let registry = Arc::new(netrunner_client::decks::sample_deck_registry());
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks.0 = 3;
        let view = || Box::new(build_client_view(&state, &registry, Side::Runner));
        let say = |game: &mut Game, message: MatchMessage| game.apply(Intent::Message(MatchMessageRef(message)));

        let mut game = Game::new(registry.clone(), Side::Runner);
        say(&mut game, MatchMessage::Awaiting { view: view() });
        assert_eq!(game.back_label(), None, "nothing done, nothing to take back");
        assert_eq!(game.apply(Intent::TakeBack), Outcome::Nothing);

        say(&mut game, MatchMessage::Back { rewind: Some(Rewind::Free) });
        say(&mut game, MatchMessage::Awaiting { view: view() });
        assert_eq!(game.back_label(), Some("Take it back"));
        assert_eq!(game.apply(Intent::TakeBack), Outcome::Rewind);
        assert!(!game.awaiting, "the controls come back with the next Awaiting");

        game.log = ["[turn 2] Runner: one", "[turn 2] Runner: two", "           and what it did"].map(|line| LogLine::from(line.to_string())).to_vec();
        say(&mut game, MatchMessage::Rewound { view: view(), removed: 1, kind: Rewind::Free });
        assert_eq!(game.log.len(), 2, "{:?}", game.log);
        assert_eq!(game.log[0].text, "[turn 2] Runner: one");
        assert!(game.log[1].text.contains("took that back"));
        assert!(!game.back);

        say(&mut game, MatchMessage::Back { rewind: Some(Rewind::Undo) });
        say(&mut game, MatchMessage::Awaiting { view: view() });
        assert_eq!(game.back_label(), Some("Take it back"), "one wording, whatever the move had shown");
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::TakeBack)), Outcome::Rewind, "the first press goes");
        say(&mut game, MatchMessage::Rewound { view: view(), removed: 0, kind: Rewind::Undo });
        assert!(game.log.last().is_some_and(|line| line.text.contains("took that back")));
    }
    /// The end-of-match table counts what still happened: a take-back
    /// drops the moves it undid from the tally as it does from the log.
    #[test]
    fn a_take_back_takes_its_counts_off_the_tally() {
        use netrunner_client::play::Rewind;
        use netrunner_core::rules::GameState;
        use netrunner_core::view::build_client_view;

        let registry = Arc::new(netrunner_client::decks::sample_deck_registry());
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Runner);
        let view = || Box::new(build_client_view(&state, &registry, Side::Runner));
        let say = |game: &mut Game, message: MatchMessage| game.apply(Intent::Message(MatchMessageRef(message)));
        let click_for_a_credit = || PublicHistoryEntry {
            turn_number: 1,
            side: Side::Runner,
            action: netrunner_core::rules::PublicAction::Visible(PlayerAction::GainCreditClick { side: Side::Runner }),
            events: vec![GameEvent::ClickSpent { side: Side::Runner }, GameEvent::CreditsGained { side: Side::Runner, amount: 1 }],
        };

        let mut game = Game::new(registry.clone(), Side::Runner);
        say(&mut game, MatchMessage::Applied { entry: click_for_a_credit(), view: view() });
        say(&mut game, MatchMessage::Applied { entry: click_for_a_credit(), view: view() });
        assert_eq!((game.tally.runner.clicks_spent, game.tally.runner.credits_gained), (2, 2));
        say(&mut game, MatchMessage::Rewound { view: view(), removed: 1, kind: Rewind::Free });
        assert_eq!((game.tally.runner.clicks_spent, game.tally.runner.credits_gained), (1, 1), "the second click no longer happened");
    }

    /// A card's "you may" answered Always is answered so from then on,
    /// without a click; a take-back puts the question back in front of
    /// the person rather than answering it again at once.
    #[test]
    fn an_optional_trigger_answered_always_is_answered_without_a_click() {
        use netrunner_client::play::Rewind;
        use netrunner_core::dsl::Effect;
        use netrunner_core::rules::{GameState, PendingChoiceResume, PendingDecision};
        use netrunner_core::view::build_client_view;

        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let registry = Arc::new(registry);
        let cookbook = CardId("cookbook".to_string());
        let view = || {
            let mut view = build_client_view(&GameState::new(1), &registry, Side::Runner);
            view.pending_decision = Some(PendingDecision::ChooseEffect {
                chooser: Side::Runner,
                options: vec![Effect::AddCounters(1), Effect::Sequence(Vec::new())],
                option_texts: vec!["place 1 virus counter on it".to_string(), String::new()],
                source_card: Some(cookbook.clone()),
                prompting_card: None,
                source_install: None,
                resume: PendingChoiceResume::None,
            });
            view.legal_actions = (0..2).map(|option_index| PlayerAction::ResolvePendingChoice { option_index }).collect();
            Box::new(view)
        };
        let say = |game: &mut Game, message: MatchMessage| game.apply(Intent::Message(MatchMessageRef(message)));
        let yes = PlayerAction::ResolvePendingChoice { option_index: 0 };

        let mut game = Game::new(registry.clone(), Side::Runner);
        assert_eq!(say(&mut game, MatchMessage::Awaiting { view: view() }), Outcome::Redraw, "asked the first time");
        let prompt = game.optional_prompt().expect("the pop-up offers to remember it");
        assert_eq!(prompt.offered(), vec![Answer::Always, Answer::Never]);
        assert_eq!(game.apply(Intent::Remember(Answer::Always)), Outcome::Submit(yes.clone()));
        assert_eq!(game.answers.get(&prompt.key), Some(Answer::Always));

        assert_eq!(say(&mut game, MatchMessage::Awaiting { view: view() }), Outcome::Submit(yes.clone()), "and not again");
        assert!(!game.awaiting);

        say(&mut game, MatchMessage::Rewound { view: view(), removed: 1, kind: Rewind::Free });
        assert_eq!(say(&mut game, MatchMessage::Awaiting { view: view() }), Outcome::Redraw, "a take-back asks");
        assert_eq!(game.apply(Intent::Remember(Answer::Never)), Outcome::Submit(PlayerAction::ResolvePendingChoice { option_index: 1 }));
        assert_eq!(game.answers.get(&prompt.key), Some(Answer::Never), "the new answer replaces the old");
        assert_eq!(say(&mut game, MatchMessage::Awaiting { view: view() }), Outcome::Submit(PlayerAction::ResolvePendingChoice { option_index: 1 }));
    }
}
