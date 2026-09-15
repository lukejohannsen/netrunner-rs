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

use std::sync::Arc;

use netrunner_client::actions::push_log_line;
use netrunner_client::board::{transitions, ActionMap, Control, Prompt, Target, Transition};
use netrunner_client::play::{GameEndReason, MatchMessage};
use netrunner_client::ratings::RatingReport;
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{InstallId, PlayerAction, ServerId, Side};
use netrunner_core::view::ClientView;

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Message(MatchMessageRef),
    /// A card or a zone on the board: opens its sheet.
    Click(Target),
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
    /// Escape: closes the options, the inspector, the sheet or the quit
    /// prompt, in that order, else asks to quit.
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
    pub options_open: bool,
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
            sheet: None,
            inspecting: None,
            options_open: false,
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
            Intent::Click(target) => self.click(target),
            Intent::Choose(index) => {
                self.sheet = None;
                self.inspecting = None;
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
                Outcome::Redraw
            }
            Intent::Back => {
                if self.options_open {
                    self.options_open = false;
                    Outcome::Redraw
                } else if self.inspecting.take().is_some() || self.sheet.take().is_some() {
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
                self.awaiting = false;
                self.actions = ActionMap::default();
                self.prompt = None;
                // The board moved under any open sheet: its entries are
                // stale until the next decision, but what it shows —
                // Archives, a card — is still worth reading.
                if let Some(sheet) = &mut self.sheet {
                    sheet.entries.clear();
                }
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
                self.options_open = false;
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

    /// Opens the target's sheet. A selection position is the exception:
    /// it is a toggle on a card the prompt lists, not a card to read, so
    /// its one entry is submitted as the rail's button would.
    fn click(&mut self, target: Target) -> Outcome {
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
