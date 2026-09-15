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
//! **One click, one route.** A card or a server with exactly one entry
//! submits it; with several, opens a popup of them; with none, opens the
//! card in the inspector. The flat panel lists every entry regardless,
//! so nothing depends on a card being placed on the board.

use std::sync::Arc;

use netrunner_client::actions::push_log_line;
use netrunner_client::board::{transitions, ActionMap, Prompt, Target, Transition};
use netrunner_client::play::{GameEndReason, MatchMessage};
use netrunner_client::ratings::RatingReport;
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{InstallId, PlayerAction, ServerId, Side};
use netrunner_core::view::ClientView;

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Message(MatchMessageRef),
    Click(Target),
    /// An entry of the action map, by index — from the panel or a popup.
    Choose(usize),
    /// Open a card's text, or close it with `None`.
    Inspect(Option<CardId>),
    /// Escape: closes the popup, the inspector or the quit prompt, else
    /// asks to quit.
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

/// A popup listing the entries a click on `target` could mean.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    pub target: Target,
    pub entries: Vec<usize>,
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
    pub popup: Option<Popup>,
    pub inspecting: Option<CardId>,
    pub rejection: Option<String>,
    pub over: Option<Over>,
    pub stalled: Option<String>,
    pub confirm_quit: bool,
    /// How many actions have been applied, for a screen to know the
    /// board moved without comparing views.
    pub applied: usize,
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
            popup: None,
            inspecting: None,
            rejection: None,
            over: None,
            stalled: None,
            confirm_quit: false,
            applied: 0,
        }
    }

    pub fn registry(&self) -> &CardRegistry {
        &self.registry
    }

    /// Whether the match is over or gone, so the panel offers nothing.
    pub fn finished(&self) -> bool {
        self.over.is_some() || self.stalled.is_some()
    }

    pub fn take_transitions(&mut self) -> Vec<Transition> {
        std::mem::take(&mut self.transitions)
    }

    pub fn apply(&mut self, intent: Intent) -> Outcome {
        match intent {
            Intent::Message(MatchMessageRef(message)) => self.message(message),
            Intent::Click(target) => self.click(target),
            Intent::Choose(index) => {
                self.popup = None;
                match self.actions.entries.get(index) {
                    Some(entry) if self.awaiting => {
                        self.awaiting = false;
                        self.rejection = None;
                        Outcome::Submit(entry.action.clone())
                    }
                    _ => Outcome::Nothing,
                }
            }
            Intent::Inspect(card) => {
                self.inspecting = card;
                Outcome::Redraw
            }
            Intent::Back => {
                if self.popup.take().is_some() || self.inspecting.take().is_some() {
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
                self.view = Some(*view);
                self.applied += 1;
                // The board moved under any popup, and a click on it now
                // would mean something else.
                self.popup = None;
                self.awaiting = false;
                self.actions = ActionMap::default();
                self.prompt = None;
                Outcome::Redraw
            }
            MatchMessage::Awaiting { view } => {
                self.actions = ActionMap::build(&view, &self.registry);
                self.prompt = Prompt::of(&view, &self.registry);
                self.view = Some(*view);
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
                self.awaiting = false;
                self.actions = ActionMap::default();
                self.prompt = None;
                self.popup = None;
                self.confirm_quit = false;
                self.over = Some(Over { winner, reason, report, notice });
                Outcome::Redraw
            }
            MatchMessage::Stalled { reason } => {
                self.awaiting = false;
                self.actions = ActionMap::default();
                self.stalled = Some(reason);
                Outcome::Redraw
            }
        }
    }

    fn click(&mut self, target: Target) -> Outcome {
        let entries: Vec<usize> = if self.awaiting {
            match &target {
                Target::HandCard(card) => self.actions.for_hand_card(card),
                Target::Install(id) => self.actions.for_install(*id),
                Target::Server(server) => self.actions.for_server(*server),
                Target::Identity(side) => self.actions.for_identity(*side),
                Target::Position(position) => self.actions.for_position(*position),
            }
        } else {
            Vec::new()
        };
        match entries.len() {
            0 => {
                // Nothing to do with it: read it, if it is a card the
                // viewer can see.
                let card = match &target {
                    Target::HandCard(card) => Some(card.clone()),
                    Target::Install(id) => self.view.as_ref().and_then(|view| netrunner_client::actions::installed_card_id(view, id)),
                    Target::Identity(side) => self.view.as_ref().and_then(|view| match side {
                        Side::Corp => view.corp.identity.clone(),
                        Side::Runner => view.runner.identity.clone(),
                    }),
                    Target::Server(_) | Target::Position(_) => None,
                };
                match card {
                    Some(card) => self.apply(Intent::Inspect(Some(card))),
                    None => Outcome::Nothing,
                }
            }
            1 => self.apply(Intent::Choose(entries[0])),
            _ => {
                self.popup = Some(Popup { target, entries });
                Outcome::Redraw
            }
        }
    }

    /// The card at an install, when the viewer may see it.
    pub fn card_at(&self, id: InstallId) -> Option<CardId> {
        self.view.as_ref().and_then(|view| netrunner_client::actions::installed_card_id(view, &id))
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

    #[test]
    fn a_click_submits_one_entry_pops_up_several_and_inspects_none() {
        let (mut game, mut handle) = game(Side::Corp);
        until_awaiting(&mut game, &mut handle);
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
        // A hand card: one entry submits, several pop up, none inspects.
        let mut seen = (false, false, false);
        for card in &hand {
            let entries = game.actions.for_hand_card(card);
            let outcome = game.apply(Intent::Click(Target::HandCard(card.clone())));
            match entries.len() {
                0 => {
                    assert_eq!(outcome, Outcome::Redraw);
                    assert_eq!(game.inspecting, Some(card.clone()));
                    game.apply(Intent::Back);
                    seen.0 = true;
                }
                1 => {
                    assert!(matches!(outcome, Outcome::Submit(_)));
                    // Undo the in-flight state for the rest of the loop.
                    game.awaiting = true;
                    seen.1 = true;
                }
                _ => {
                    assert_eq!(outcome, Outcome::Redraw);
                    assert_eq!(game.popup.as_ref().map(|p| p.entries.len()), Some(entries.len()));
                    assert_eq!(game.apply(Intent::Back), Outcome::Redraw, "Escape closes the popup");
                    assert!(game.popup.is_none());
                    seen.2 = true;
                }
            }
        }
        assert!(seen.1 || seen.2, "an opening Corp hand has something playable");
        // A server: HQ can always be run by a Runner but not by the Corp,
        // and an install into a new remote is offered from the server.
        let remote = game.actions.entries.iter().find_map(|e| match &e.action {
            PlayerAction::InstallCard { zone, .. } => Some(*zone),
            _ => None,
        });
        if let Some(server) = remote {
            assert!(!game.actions.for_server(server).is_empty());
        }
        // Quitting mid-game asks first; Escape again withdraws.
        assert_eq!(game.apply(Intent::Back), Outcome::Redraw);
        assert!(game.confirm_quit);
        assert_eq!(game.apply(Intent::Back), Outcome::Redraw);
        assert!(!game.confirm_quit);
        game.apply(Intent::RequestQuit);
        assert_eq!(game.apply(Intent::ConfirmQuit), Outcome::Quit);
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
}
