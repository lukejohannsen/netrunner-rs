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
//! included; dropping on one submits the entry that lands there, or opens
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

use netrunner_client::actions::push_log_line;
use netrunner_client::board::{transitions, ActionMap, Affordance, Control, Pile, Prompt, RunTrail, Target, Transition};
use netrunner_client::play::{lone_pass, GameEndReason, MatchMessage};
use netrunner_client::ratings::RatingReport;
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{GameEvent, InstallId, PlayerAction, ServerId, Side};
use netrunner_core::view::ClientView;

use crate::models::drag::{insert_at, Drag, Release};
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
    /// A key the board reads (`shortcuts`). The ones about the pointer
    /// or the settings file are the screen's; the model answers the rest.
    Shortcut(Shortcut),
    /// The gear: open or close the game options.
    ToggleOptions,
    /// Escape: closes the options, the list of keys, the menu, the
    /// inspector, the sheet or the quit prompt, in that order, else asks
    /// to quit.
    Back,
    RequestQuit,
    ConfirmQuit,
    CancelQuit,
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

/// The box on the window a menu sits above — the clicked node's,
/// in logical pixels: `x, y` its centre, `width, height` its size. A
/// menu at the pointer landed somewhere different on every click; the
/// card's own box is one place.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Anchor {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

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
    pub report: Option<RatingReport>,
    pub notice: Option<String>,
}

pub struct Game {
    registry: Arc<CardRegistry>,
    pub side: Side,
    pub view: Option<ClientView>,
    pub actions: ActionMap,
    pub prompt: Option<Prompt>,
    pub log: Vec<String>,
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
    /// Enter was pressed with clicks left: the next Enter ends the turn,
    /// and anything else in between stands it down. The engine lists
    /// `EndTurn` with clicks unspent — correctly, a player may end early —
    /// so one stray Enter would have thrown them away.
    pub end_turn_armed: bool,
    pub rejection: Option<String>,
    pub over: Option<Over>,
    pub stalled: Option<String>,
    pub confirm_quit: bool,
    /// How many actions have been applied, for a screen to know the
    /// board moved without comparing views.
    pub applied: usize,
    /// The run on, or the last one, as a trail of steps; `None` before
    /// the first run and after the turn that followed the last.
    pub trail: Option<RunTrail>,
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
            end_turn_armed: false,
            rejection: None,
            over: None,
            stalled: None,
            confirm_quit: false,
            applied: 0,
            trail: None,
        }
    }

    pub fn registry(&self) -> &CardRegistry {
        &self.registry
    }

    /// Whether the match is over or gone, so the panel offers nothing.
    pub fn finished(&self) -> bool {
        self.over.is_some() || self.stalled.is_some()
    }

    /// Whether something covers the board — a sheet, a card being read,
    /// the options, the list of keys, the quit prompt, the end of the
    /// match — so a click that reaches a card through it opens nothing,
    /// and a key does nothing.
    pub fn covered(&self) -> bool {
        self.finished() || self.confirm_quit || self.options_open || self.help_open || self.sheet.is_some() || self.inspecting.is_some()
    }

    /// Whether a click that misses the panel closes what is open.
    ///
    /// Only a *reading* surface: the card, the install, the zone, the
    /// score area. Those lost their Close button because the name was
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
        !self.finished() && !self.confirm_quit && !self.options_open && !self.help_open && (self.sheet.is_some() || self.inspecting.is_some())
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
        // Whatever comes between two Enters stands the first one down;
        // the notice it put on the rail goes with a redraw.
        let armed = std::mem::take(&mut self.end_turn_armed);
        match self.apply_intent(intent, armed) {
            Outcome::Nothing if armed && !self.end_turn_armed => Outcome::Redraw,
            outcome => outcome,
        }
    }

    fn apply_intent(&mut self, intent: Intent, armed: bool) -> Outcome {
        match intent {
            Intent::Message(MatchMessageRef(message)) => self.message(message),
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
                        Outcome::Submit(entry.action.clone())
                    }
                    _ => Outcome::Nothing,
                }
            }
            Intent::Control(control) => match self.actions.for_control(control) {
                Some(index) if self.awaiting => self.apply(Intent::Choose(index)),
                _ => Outcome::Nothing,
            },
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
            Intent::Shortcut(shortcut) => self.shortcut(shortcut, armed),
            Intent::ToggleOptions => {
                self.options_open = !self.options_open;
                self.menu = None;
                Outcome::Redraw
            }
            Intent::Back => {
                if self.options_open {
                    self.options_open = false;
                    Outcome::Redraw
                } else if self.help_open {
                    self.help_open = false;
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
                if self.finished() {
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
        }
    }

    fn message(&mut self, message: MatchMessage) -> Outcome {
        match message {
            MatchMessage::Applied { entry, view } => {
                if let Some(before) = &self.view {
                    self.transitions.extend(transitions(before, &view, &entry));
                }
                push_log_line(&mut self.log, &entry, &self.registry, Some(&view));
                self.follow_run(&view);
                self.view = Some(*view);
                self.follow_hand();
                self.applied += 1;
                self.awaiting = false;
                self.actions = ActionMap::default();
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
            MatchMessage::Awaiting { view } if lone_pass(&view).is_some() => {
                let pass = lone_pass(&view).expect("matched above");
                self.actions = ActionMap::build(&view, &self.registry);
                self.prompt = Prompt::of(&view, &self.registry);
                self.view = Some(*view);
                self.follow_hand();
                self.awaiting = false;
                Outcome::Submit(pass)
            }
            MatchMessage::Awaiting { view } => {
                self.actions = ActionMap::build(&view, &self.registry);
                self.prompt = Prompt::of(&view, &self.registry);
                self.view = Some(*view);
                self.follow_hand();
                self.awaiting = true;
                Outcome::Redraw
            }
            MatchMessage::Rejected { reason } => {
                self.rejection = Some(reason);
                self.awaiting = true;
                Outcome::Redraw
            }
            MatchMessage::Ended { winner, reason, view, report, notice } => {
                self.view = Some(*view);
                self.follow_hand();
                self.awaiting = false;
                self.actions = ActionMap::default();
                self.prompt = None;
                self.sheet = None;
                self.inspecting = None;
                self.menu = None;
                self.options_open = false;
                self.help_open = false;
                self.confirm_quit = false;
                self.over = Some(Over { winner, reason, report, notice });
                Outcome::Redraw
            }
            MatchMessage::Stalled { reason } => {
                self.awaiting = false;
                self.actions = ActionMap::default();
                self.menu = None;
                self.stalled = Some(reason);
                Outcome::Redraw
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
    fn shortcut(&mut self, shortcut: Shortcut, armed: bool) -> Outcome {
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
        if self.covered() {
            return Outcome::Nothing;
        }
        match shortcut {
            Shortcut::Go => match self.actions.for_control(Control::PassPriority) {
                Some(_) => self.apply(Intent::Control(Control::PassPriority)),
                None => self.apply(Intent::Control(Control::ContinueRun)),
            },
            // Enter with clicks left asks for a second Enter; with none
            // left, or on the second, it is the bar's End turn.
            Shortcut::Control(Control::EndTurn) if !armed && self.clicks_left() > 0 && self.awaiting && self.actions.for_control(Control::EndTurn).is_some() => {
                self.end_turn_armed = true;
                Outcome::Redraw
            }
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
                    None => Outcome::Nothing,
                }
            }
            Shortcut::ScoreArea(side) => self.apply(Intent::Inspect(Target::Pile(Pile::Agendas(side)))),
            Shortcut::ReadHovered | Shortcut::MenuHovered | Shortcut::PlayHelper | Shortcut::PhaseBar | Shortcut::Help => Outcome::Nothing,
        }
    }

    /// Opens the target's menu over `over`, or closes it if it was this
    /// target's. Two exceptions: a selection position is a toggle on a
    /// card the prompt lists, so its one entry is submitted as the rail's
    /// button would; and the Agendas readout is a door to the score area,
    /// not a card, so it opens that list — where a scored agenda's
    /// abilities are.
    fn click(&mut self, target: Target, over: Anchor) -> Outcome {
        if self.covered() {
            return Outcome::Nothing;
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
            Target::Server(_) | Target::Position(_) | Target::Pile(_) => None,
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
    /// and empty when nothing is held or the card is played rather than
    /// placed (an operation, an event, a Runner's own install).
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
        let spec = LocalMatchSpec { registry: registry.clone(), corp, runner, human: side, level: Level::Novice, style: None, seed: 11, rating: None };
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
        while !matches!(game.view.as_ref().unwrap().phase, GamePhase::Action(Side::Corp)) {
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
        assert!(game.log[0].contains("Keep hand"), "{}", game.log[0]);
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
        // The bar: End turn is listed and submits; Jack out is the
        // Runner's and never on the Corp's map.
        assert_eq!(game.apply(Intent::Control(Control::JackOut)), Outcome::Nothing);
        assert_eq!(game.apply(Intent::Control(Control::EndTurn)), Outcome::Submit(PlayerAction::EndTurn));
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

    /// A key is the button it stands for: at the mulligan Space and C do
    /// nothing and 1 keeps (the pop-up's first button); on the Corp's turn
    /// C takes the credit, a digit presses the open menu's button, Enter
    /// with clicks left asks twice and anything between stands it down;
    /// the list of keys opens and closes on its own key and on Escape, and
    /// no other key acts while the board is covered.
    #[test]
    fn a_key_presses_the_button_it_stands_for_and_nothing_through_an_overlay() {
        let (mut game, mut handle) = game(Side::Corp);
        until_awaiting(&mut game, &mut handle);
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Go)), Outcome::Nothing, "nothing to pass at the mulligan");
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
        // Enter with clicks left: the first arms, the second ends.
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Control(Control::EndTurn))), Outcome::Redraw);
        assert!(game.end_turn_armed && game.awaiting, "the first Enter sends nothing");
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Control(Control::EndTurn))), Outcome::Submit(PlayerAction::EndTurn));
        assert!(!game.end_turn_armed);
        game.awaiting = true;
        // Anything between the two stands the first down.
        game.apply(Intent::Shortcut(Shortcut::Control(Control::EndTurn)));
        assert_eq!(game.apply(Intent::CloseMenu), Outcome::Redraw, "the notice goes with a redraw");
        assert!(!game.end_turn_armed);
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Control(Control::EndTurn))), Outcome::Redraw, "and the next Enter asks again");
        // With no clicks left, one Enter ends the turn.
        game.end_turn_armed = false;
        match &mut game.view {
            Some(view) => view.corp.clicks = 0,
            None => unreachable!(),
        }
        assert_eq!(game.apply(Intent::Shortcut(Shortcut::Control(Control::EndTurn))), Outcome::Submit(PlayerAction::EndTurn));
        game.awaiting = true;
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
    /// card with nowhere to go lights nothing.
    #[test]
    fn a_card_dropped_on_a_lit_place_is_played_there() {
        let (mut game, mut handle) = game(Side::Corp);
        until_awaiting(&mut game, &mut handle);
        until_the_corps_action_phase(&mut game, &mut handle);
        // A card the engine offers an install for, and where it may go.
        let hand = game.hand.cards().to_vec();
        let (slot, card, places) = hand
            .iter()
            .enumerate()
            .find_map(|(slot, card)| {
                let places = game.actions.destinations_for_hand_card(card);
                (!places.is_empty()).then(|| (slot, card.clone(), places))
            })
            .expect("an opening Corp hand has something to install");
        let place = places[0].clone();
        // Nothing is lit until the press has become a drag.
        assert!(game.drop_places().is_empty(), "nothing is held");
        game.apply(Intent::DragPress { slot, at: (100.0, 900.0) });
        assert!(game.drop_places().is_empty(), "a press alone lights nothing");
        game.apply(Intent::DragMove { at: (400.0, 400.0) });
        assert_eq!(game.drop_places(), places, "the card's own places are lit");
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
        // A card that is played rather than placed lights nothing, and a
        // drop on a place it does not name does nothing.
        game.awaiting = true;
        if let Some((slot, card)) = hand.iter().enumerate().find(|(_, card)| game.actions.destinations_for_hand_card(card).is_empty() && !game.actions.for_hand_card(card).is_empty()) {
            game.apply(Intent::DragPress { slot, at: (100.0, 900.0) });
            game.apply(Intent::DragMove { at: (400.0, 400.0) });
            assert!(game.drop_places().is_empty(), "{} is played, not placed", card.0);
            assert_eq!(game.apply(Intent::DragDrop { target: Target::Server(ServerId::Archives), over: Anchor::default() }), Outcome::Redraw);
            assert!(game.dragging.is_none() && game.menu.is_none(), "a drop it does not name puts it back");
        }
        handle.join();
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
        assert!(game.awaiting && game.actions.for_control(Control::PassPriority).is_some(), "a rejected pass is back on the bar");
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
}
