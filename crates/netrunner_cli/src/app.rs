//! The interactive TUI's state: the human seat's latest `ClientView`
//! (received over a channel from a background `netrunner_server::
//! MatchSession` task — never the raw `GameState`), UI selection state, and
//! the most recent rejection/game-end notice.
//!
//! Exactly one side is the human seat; the other is always bot-controlled
//! (see `config::Config::corp`'s doc comment) — under real per-side
//! masking there's no coherent way for a single local terminal to
//! represent "both sides, simultaneously, from each one's own point of
//! view," so this app doesn't try to.

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc;

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{PlayerAction, Side};
use netrunner_core::view::ClientView;
use netrunner_server::protocol::GameEndReason;
use netrunner_server::{ClientMessage, ServerMessage};

pub struct App {
    pub registry: CardRegistry,
    pub human_side: Side,
    tx: mpsc::UnboundedSender<ClientMessage>,
    rx: mpsc::UnboundedReceiver<ServerMessage>,
    /// `None` until the first `StateUpdate` arrives from the match session
    /// (should be near-instant — the session broadcasts its initial state
    /// before waiting on anything).
    pub view: Option<ClientView>,
    pub selected: usize,
    pub should_quit: bool,
    pub last_rejection: Option<String>,
    pub game_ended: Option<(Side, GameEndReason)>,
}

impl App {
    pub fn new(
        registry: CardRegistry,
        human_side: Side,
        tx: mpsc::UnboundedSender<ClientMessage>,
        rx: mpsc::UnboundedReceiver<ServerMessage>,
    ) -> Self {
        let mut app = App {
            registry,
            human_side,
            tx,
            rx,
            view: None,
            selected: 0,
            should_quit: false,
            last_rejection: None,
            game_ended: None,
        };
        app.drain_messages();
        app
    }

    /// Non-blocking drain of every message the match session has sent
    /// since the last poll — called once at construction and once per TUI
    /// render tick, mirroring the ~100ms `event::poll` cadence the render
    /// loop already uses for keyboard input.
    pub fn drain_messages(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            match message {
                ServerMessage::StateUpdate(view) => {
                    if self.selected >= view.legal_actions.len() {
                        self.selected = 0;
                    }
                    self.view = Some(*view);
                    self.last_rejection = None;
                }
                ServerMessage::ActionRejected { reason } => self.last_rejection = Some(reason),
                ServerMessage::GameEnded { winner, reason } => self.game_ended = Some((winner, reason)),
                ServerMessage::MatchJoined { .. } => {}
            }
        }
    }

    pub fn is_game_over(&self) -> bool {
        self.game_ended.is_some()
    }

    pub fn legal_actions(&self) -> &[PlayerAction] {
        self.view.as_ref().map_or(&[], |view| view.legal_actions.as_slice())
    }

    fn submit_selected_action(&mut self) {
        let Some(action) = self.legal_actions().get(self.selected).cloned() else { return };
        let _ = self.tx.send(ClientMessage::SubmitAction(action));
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.is_game_over() {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                self.should_quit = true;
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.submit_selected_action(),
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.legal_actions().len();
        if len == 0 {
            return;
        }
        let next = (self.selected as i32 + delta).rem_euclid(len as i32);
        self.selected = next as usize;
    }
}

/// Human-readable label for a `PlayerAction`, resolving `CardId`s to
/// registry titles where available.
pub fn describe_action(action: &PlayerAction, registry: &CardRegistry) -> String {
    let title = |card_id: &netrunner_core::dsl::CardId| -> String {
        registry.get(card_id).map(|c| c.title.clone()).unwrap_or_else(|| card_id.0.clone())
    };

    match action {
        PlayerAction::GainCreditClick { side } => format!("Gain 1 credit ({side:?})"),
        PlayerAction::DrawCardClick => "Draw a card".to_string(),
        PlayerAction::InstallCard { card_id, zone, slot } => {
            format!("Install {} into {:?} ({:?})", title(card_id), zone, slot)
        }
        PlayerAction::RezIce { ice_id } => format!("Rez {}", title(ice_id)),
        PlayerAction::InitiateRun { server } => format!("Run {server:?}"),
        PlayerAction::ContinueRun => "Continue run".to_string(),
        PlayerAction::JackOut => "Jack out".to_string(),
        PlayerAction::CompleteRun => "Complete run".to_string(),
        PlayerAction::PlayEvent { card_id } => format!("Play {}", title(card_id)),
        PlayerAction::PlayOperation { card_id } => format!("Play {}", title(card_id)),
        PlayerAction::InstallHardware { card_id } => format!("Install {}", title(card_id)),
        PlayerAction::InstallProgram { card_id, .. } => format!("Install {}", title(card_id)),
        PlayerAction::BreakSubroutine { ice_id, subroutine_index } => {
            format!("Break subroutine {subroutine_index} on {}", title(ice_id))
        }
        PlayerAction::EndTurn => "End turn".to_string(),
        PlayerAction::DiscardCard { card_id } => format!("Discard {}", title(card_id)),
        PlayerAction::KeepHand => "Keep hand".to_string(),
        PlayerAction::TakeMulligan => "Mulligan".to_string(),
        PlayerAction::ActivateAbility { card_id, ability_index } => {
            format!("Activate ability {ability_index} on {}", title(card_id))
        }
        PlayerAction::AdvanceCard { card_id } => format!("Advance {}", title(card_id)),
        PlayerAction::ScoreAgenda { card_id } => format!("Score {}", title(card_id)),
        PlayerAction::RemoveTag => "Remove a tag".to_string(),
        PlayerAction::TrashResource { card_id } => format!("Trash {}", title(card_id)),
        PlayerAction::SelectCardToAccess { card_id } => format!("Access {}", title(card_id)),
        PlayerAction::StealAgenda { card_id } => format!("Steal {}", title(card_id)),
        PlayerAction::TrashAccessedCard { card_id } => format!("Trash {}", title(card_id)),
        PlayerAction::PassAccessedCard { card_id } => format!("Pass on {}", title(card_id)),
        PlayerAction::PayToAvoidAccessTrigger { card_id } => format!("Pay to avoid {}'s trigger", title(card_id)),
        PlayerAction::DeclineAccessTrigger { card_id } => format!("Decline {}'s trigger", title(card_id)),
        PlayerAction::PassPriority { side } => format!("Pass priority ({side:?})"),
        PlayerAction::SubmitCorpTraceBid { amount } => format!("Bid {amount} (Corp trace)"),
        PlayerAction::SubmitRunnerTraceBid { amount } => format!("Bid {amount} (Runner trace)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::RandomAgent;
    use netrunner_core::rules::GameState;
    use netrunner_server::{MatchSession, PlayerSlot};

    use crate::decks;

    fn setup() -> (GameState, CardRegistry) {
        let registry = decks::kate_vs_hb_registry();
        let (corp_deck, runner_deck) = decks::kate_vs_hb_decks();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 1).expect("legal decks set up cleanly");
        (state, registry)
    }

    fn spawn_session(state: GameState, registry: CardRegistry, corp_slot: PlayerSlot, runner_slot: PlayerSlot) {
        let session = MatchSession::new(state, registry, corp_slot, runner_slot);
        tokio::spawn(session.run());
    }

    #[tokio::test]
    async fn human_seat_receives_its_own_view_and_can_submit_an_action() {
        let (state, registry) = setup();
        let (server_tx, app_rx) = mpsc::unbounded_channel();
        let (app_tx, server_rx) = mpsc::unbounded_channel();
        let corp_slot = PlayerSlot::Channel { tx: server_tx, rx: server_rx };
        let runner_slot = PlayerSlot::Bot(Box::new(RandomAgent::new(2)));

        spawn_session(state, registry.clone(), corp_slot, runner_slot);

        let mut app = App::new(registry, Side::Corp, app_tx, app_rx);
        // Give the background task a moment to deliver the initial view.
        for _ in 0..50 {
            if app.view.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            app.drain_messages();
        }

        let (initial_phase, initial_credits) = {
            let view = app.view.as_ref().expect("initial view delivered");
            assert_eq!(view.side, Side::Corp);
            (view.phase, view.corp.credits)
        };
        assert!(!app.legal_actions().is_empty());
        assert!(app.legal_actions().iter().all(|action| matches!(action, PlayerAction::KeepHand | PlayerAction::TakeMulligan)));

        app.handle_key(KeyEvent::from(KeyCode::Enter));
        for _ in 0..50 {
            app.drain_messages();
            if app.view.as_ref().is_some_and(|v| v.phase != initial_phase || v.corp.credits != initial_credits) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }
}
