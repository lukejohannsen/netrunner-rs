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
//! **A click never acts; it opens.** A card or a zone clicked opens its
//! [`Sheet`] — the card large with its legal actions beside it, or the
//! zone's contents with what may be done there — and a press on one of
//! those submits. The first cut submitted a card's single action on the
//! click meant to examine it and lost a game to it (Phase 7 §3, item 8);
//! the sheet is the deliberate second click. The basic actions live on
//! a fixed control bar (`board::Control`) and the prompt's decisions
//! under the prompt, so nothing depends on the flat panel, which is an
//! aid a person turns on (`DesktopPrefs::play_helper`).
//!
//! **A secondary click opens the same actions as a [`Menu`] above the
//! card** — the card game's "pick the card up and see what it can do"
//! without the sheet's reading. It lists exactly the entries the sheet
//! would (`entries_for`), so the two are one rule with two doors, and
//! it sits just above the card's own box (the [`Anchor`] the screen
//! read off the node), never at the pointer, so it is in the same
//! place however the card was clicked and the card stays in view; it never opens over a sheet,
//! the options or the quit prompt
//! (the board still reports hovers through an overlay's ground), and
//! it closes on the next click anywhere, on Escape, and when the board
//! moves under it.
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
use netrunner_client::board::{transitions, ActionMap, Control, Prompt, RunTrail, Target, Transition};
use netrunner_client::play::{GameEndReason, MatchMessage};
use netrunner_client::ratings::RatingReport;
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{GameEvent, InstallId, PlayerAction, ServerId, Side};
use netrunner_core::view::ClientView;

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Message(MatchMessageRef),
    /// A beat of a run: the events, in order, that the trail observes
    /// before the message that applied them arrives.
    RunStep(Vec<GameEvent>),
    /// A card or a zone on the board: opens its sheet.
    Click(Target),
    /// A secondary click on a card or a zone: opens its actions as a
    /// menu just above the node that was clicked.
    Menu { target: Target, over: Anchor },
    /// A click that landed on nothing of the menu's: closes it.
    CloseMenu,
    /// An entry of the action map, by index — from a sheet, the rail or
    /// the flat panel.
    Choose(usize),
    /// A control-bar button: the entry it means, if the engine lists one.
    Control(Control),
    /// Open a card's text over whatever is open (a face in a zone sheet),
    /// or close it with `None`.
    Inspect(Option<CardId>),
    /// The gear: open or close the game options.
    ToggleOptions,
    /// Escape: closes the options, the menu, the inspector, the sheet or
    /// the quit prompt, in that order, else asks to quit.
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

/// What a click opened: the target and the entries a press there could
/// mean. A card target is drawn as the card with its actions; a zone
/// target as the zone's contents with its actions. `entries` is empty
/// while the person is not awaiting and is rebuilt on the next
/// `Awaiting`, so a sheet left open while the opponent acts stays open
/// and offers the new list.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub target: Target,
    pub entries: Vec<usize>,
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

/// A secondary click's menu: the target, the entries a press could
/// mean (the sheet's list, `entries_for`), and the box it sits above.
/// Unlike a sheet it does not outlive the board it was opened over:
/// an applied action closes it, since the card it sits on may have
/// moved.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub target: Target,
    pub entries: Vec<usize>,
    pub over: Anchor,
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
    /// A secondary click's menu, over its card.
    pub menu: Option<Menu>,
    pub options_open: bool,
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
            menu: None,
            options_open: false,
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
    /// the options, the quit prompt, the end of the match — so a click
    /// that reaches a card through it opens nothing.
    pub fn covered(&self) -> bool {
        self.finished() || self.confirm_quit || self.options_open || self.sheet.is_some() || self.inspecting.is_some()
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
        match target {
            Target::HandCard(card) => self.actions.for_hand_card(card),
            Target::Install(id) => self.actions.for_install(*id),
            Target::Server(server) => self.actions.for_server(*server),
            Target::Identity(side) => self.actions.for_identity(*side),
            Target::Position(position) => self.actions.for_position(*position),
            Target::Pile(pile) => self.actions.for_pile(*pile),
        }
    }

    pub fn apply(&mut self, intent: Intent) -> Outcome {
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
            Intent::Click(target) => self.click(target),
            Intent::Menu { target, over } => {
                if self.covered() {
                    return Outcome::Nothing;
                }
                let entries = self.entries_for(&target);
                self.menu = Some(Menu { target, entries, over });
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
            Intent::Inspect(card) => {
                self.inspecting = card;
                Outcome::Redraw
            }
            Intent::ToggleOptions => {
                self.options_open = !self.options_open;
                self.menu = None;
                Outcome::Redraw
            }
            Intent::Back => {
                if self.options_open {
                    self.options_open = false;
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
                self.applied += 1;
                self.awaiting = false;
                self.actions = ActionMap::default();
                self.prompt = None;
                // The board moved under any open sheet: its entries are
                // stale until the next decision, but what it shows —
                // Archives, a card — is still worth reading. A menu is
                // not: it sat at a pointer over a card that may be gone.
                if let Some(sheet) = &mut self.sheet {
                    sheet.entries.clear();
                }
                self.menu = None;
                Outcome::Redraw
            }
            MatchMessage::Awaiting { view } => {
                self.actions = ActionMap::build(&view, &self.registry);
                self.prompt = Prompt::of(&view, &self.registry);
                self.view = Some(*view);
                self.awaiting = true;
                if let Some(target) = self.sheet.as_ref().map(|s| s.target.clone()) {
                    let entries = self.entries_for(&target);
                    self.sheet = Some(Sheet { target, entries });
                }
                Outcome::Redraw
            }
            MatchMessage::Rejected { reason } => {
                self.rejection = Some(reason);
                self.awaiting = true;
                Outcome::Redraw
            }
            MatchMessage::Ended { winner, reason, view, report, notice } => {
                self.view = Some(*view);
                self.awaiting = false;
                self.actions = ActionMap::default();
                self.prompt = None;
                self.sheet = None;
                self.inspecting = None;
                self.menu = None;
                self.options_open = false;
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

    /// Opens the target's sheet. A selection position is the exception:
    /// it is a toggle on a card the prompt lists, not a card to read, so
    /// its one entry is submitted as the rail's button would.
    fn click(&mut self, target: Target) -> Outcome {
        self.menu = None;
        let entries = self.entries_for(&target);
        if let Target::Position(_) = target {
            return match entries.as_slice() {
                [index] => self.apply(Intent::Choose(*index)),
                _ => Outcome::Nothing,
            };
        }
        self.inspecting = None;
        self.sheet = Some(Sheet { target, entries });
        Outcome::Redraw
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
            game.apply(Intent::Message(MatchMessageRef(message)));
            assert!(!game.finished(), "the game ended first");
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

    /// A click never submits: a hand card opens its sheet with its entries,
    /// a press on one of those submits, a zone opens its sheet even with
    /// nothing to do there, the bar submits what the engine lists and
    /// nothing else, and Escape closes things in order.
    #[test]
    fn a_click_opens_a_sheet_and_never_submits() {
        let (mut game, mut handle) = game(Side::Corp);
        until_awaiting(&mut game, &mut handle);
        // At the mulligan the bar has nothing: keep and mulligan are the
        // prompt's decisions, not controls.
        assert_eq!(game.apply(Intent::Control(Control::EndTurn)), Outcome::Nothing);
        assert!(game.awaiting, "a control with no entry submits nothing");
        let decisions = game.actions.decisions();
        assert_eq!(decisions.len(), 2, "keep and mulligan: {:?}", game.actions.entries);
        // Keep, and let the Runner keep, until the Corp's action phase.
        loop {
            let view = game.view.as_ref().unwrap();
            if matches!(view.phase, GamePhase::Action(Side::Corp)) {
                break;
            }
            let Outcome::Submit(action) = game.apply(Intent::Choose(0)) else { panic!() };
            handle.submit(action).unwrap();
            game.awaiting = false;
            until_awaiting(&mut game, &mut handle);
        }
        let view = game.view.clone().unwrap();
        let hand = view.corp.hq_cards.clone().unwrap();
        let mut pressed_one = false;
        for card in &hand {
            let entries = game.actions.for_hand_card(card);
            let outcome = game.apply(Intent::Click(Target::HandCard(card.clone())));
            assert_eq!(outcome, Outcome::Redraw, "a click opens, whatever the card can do");
            assert_eq!(game.sheet, Some(Sheet { target: Target::HandCard(card.clone()), entries: entries.clone() }));
            assert_eq!(game.card_of(&Target::HandCard(card.clone())).as_ref(), Some(card));
            assert!(game.awaiting, "nothing was sent");
            if let (Some(first), false) = (entries.first(), pressed_one) {
                let Outcome::Submit(action) = game.apply(Intent::Choose(*first)) else { panic!("the sheet's button submits") };
                assert_eq!(&action, &game.actions.entries[*first].action);
                assert!(game.sheet.is_none() && !game.awaiting);
                // Undo the in-flight state for the rest of the loop.
                game.awaiting = true;
                pressed_one = true;
            } else {
                assert_eq!(game.apply(Intent::Back), Outcome::Redraw, "Escape closes the sheet");
                assert!(game.sheet.is_none());
            }
        }
        assert!(pressed_one, "an opening Corp hand has something playable");
        // R&D: the sheet offers the draw, and no zone click ever draws.
        let before = game.applied;
        game.apply(Intent::Click(Target::Server(ServerId::RnD)));
        let sheet = game.sheet.clone().unwrap();
        assert!(sheet.entries.iter().any(|i| matches!(game.actions.entries[*i].action, PlayerAction::DrawCardClick { .. })));
        assert!(game.card_of(&sheet.target).is_none(), "a zone is not a card");
        assert!(game.awaiting && game.applied == before);
        // Archives offers the installs into it and nothing else; a zone
        // with nothing to do still opens (the identity, say).
        game.apply(Intent::Click(Target::Server(ServerId::Archives)));
        assert_eq!(game.sheet.as_ref().map(|s| s.entries.clone()), Some(game.actions.for_server(ServerId::Archives)));
        game.apply(Intent::Click(Target::Identity(Side::Runner)));
        assert_eq!(game.sheet.as_ref().map(|s| s.entries.len()), Some(0), "the opponent's identity has nothing to do");
        assert!(game.card_of(&Target::Identity(Side::Runner)).is_some(), "but is a card to read");
        // A face in the pile reads over the sheet; Escape closes the
        // reading first, then the sheet.
        game.apply(Intent::Inspect(Some(hand[0].clone())));
        assert_eq!(game.apply(Intent::Back), Outcome::Redraw);
        assert!(game.inspecting.is_none() && game.sheet.is_some());
        game.apply(Intent::Back);
        assert!(game.sheet.is_none());
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

    /// A secondary click opens the target's entries as a menu above the
    /// card, the same list the sheet would show; a press on one
    /// submits; a click elsewhere, Escape and the board moving close
    /// it; and nothing opens through a sheet.
    #[test]
    fn a_secondary_click_opens_a_menu_of_the_targets_entries_and_never_submits() {
        let (mut game, mut handle) = game(Side::Corp);
        until_awaiting(&mut game, &mut handle);
        loop {
            let view = game.view.as_ref().unwrap();
            if matches!(view.phase, GamePhase::Action(Side::Corp)) {
                break;
            }
            let Outcome::Submit(action) = game.apply(Intent::Choose(0)) else { panic!() };
            handle.submit(action).unwrap();
            game.awaiting = false;
            until_awaiting(&mut game, &mut handle);
        }
        let hand = game.view.as_ref().unwrap().corp.hq_cards.clone().unwrap();
        let (card, entries) = hand.iter().map(|c| (c.clone(), game.actions.for_hand_card(c))).find(|(_, e)| !e.is_empty()).expect("something playable");
        let target = Target::HandCard(card.clone());
        let before = game.applied;
        let over = Anchor { x: 300.0, y: 700.0, width: 120.0, height: 168.0 };
        assert_eq!(game.apply(Intent::Menu { target: target.clone(), over }), Outcome::Redraw);
        assert_eq!(game.menu, Some(Menu { target: target.clone(), entries: entries.clone(), over }));
        assert_eq!(game.menu.as_ref().unwrap().entries, game.entries_for(&target), "the menu is the sheet's list");
        assert!(game.awaiting && game.applied == before && game.sheet.is_none(), "it opened, and opened no sheet");
        // Escape closes the menu and nothing else.
        assert_eq!(game.apply(Intent::Back), Outcome::Redraw);
        assert!(game.menu.is_none() && !game.confirm_quit);
        assert_eq!(game.apply(Intent::CloseMenu), Outcome::Nothing, "nothing to close");
        // A zone has a menu too: R&D offers the draw.
        game.apply(Intent::Menu { target: Target::Server(ServerId::RnD), over: Anchor::default() });
        assert!(game.menu.as_ref().unwrap().entries.iter().any(|i| matches!(game.actions.entries[*i].action, PlayerAction::DrawCardClick { .. })));
        // A click elsewhere closes it; a left click on a card closes it
        // and opens the sheet.
        assert_eq!(game.apply(Intent::CloseMenu), Outcome::Redraw);
        game.apply(Intent::Menu { target: target.clone(), over: Anchor::default() });
        game.apply(Intent::Click(target.clone()));
        assert!(game.menu.is_none() && game.sheet.is_some());
        // Through the sheet, nothing opens.
        assert_eq!(game.apply(Intent::Menu { target: target.clone(), over: Anchor::default() }), Outcome::Nothing);
        assert!(game.menu.is_none());
        game.apply(Intent::Back);
        // The menu's button submits, as the sheet's would, and closes it.
        game.apply(Intent::Menu { target: target.clone(), over: Anchor::default() });
        let Outcome::Submit(action) = game.apply(Intent::Choose(entries[0])) else { panic!("the menu's button submits") };
        assert_eq!(action, game.actions.entries[entries[0]].action);
        assert!(game.menu.is_none() && !game.awaiting);
        handle.submit(action).unwrap();
        // The board moving closes a menu opened meanwhile.
        game.menu = Some(Menu { target, entries: Vec::new(), over: Anchor::default() });
        until_awaiting(&mut game, &mut handle);
        assert!(game.menu.is_none(), "an applied action closes the menu");
        handle.join();
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
