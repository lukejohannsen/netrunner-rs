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
use netrunner_core::rules::{InstallId, InstallSlot, PlayerAction, Side};
use netrunner_core::view::ClientView;
use netrunner_server::protocol::GameEndReason;
use netrunner_server::{ClientMessage, HistoryEntry, ServerMessage};

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
    /// Rendered log of every resolved action, from `ServerMessage::
    /// ActionLog`. Remote play had no log at all until the match driver
    /// grew a `MatchHistory` the server could forward.
    pub action_log: Vec<String>,
}

/// Cap on retained log lines, shared by both TUI paths.
pub const MAX_LOG_LINES: usize = 200;

/// Appends one resolved action to a capped log. Shared by the remote path
/// (`App`, fed by `ServerMessage::ActionLog`) and the local one
/// (`tui::LocalUiState`, fed straight off `Session`'s history) so the two
/// render identically.
pub fn push_log_line(log: &mut Vec<String>, entry: &HistoryEntry, registry: &CardRegistry, view: Option<&ClientView>) {
    log.push(format!(
        "[turn {}] {:?}: {}",
        entry.turn_number,
        entry.side,
        describe_action(&entry.action, registry, view)
    ));
    if log.len() > MAX_LOG_LINES {
        let excess = log.len() - MAX_LOG_LINES;
        log.drain(0..excess);
    }
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
            action_log: Vec::new(),
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
                ServerMessage::ActionLog(entry) => {
                    push_log_line(&mut self.action_log, &entry, &self.registry, self.view.as_ref())
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

/// What `tui::{draw_header, draw_board, draw_actions}` need to render one
/// frame — implemented by `App` (the remote/channel-backed path) and by
/// `tui::LocalUiState` (the local `netrunner_single_player`-backed path),
/// so both share the same rendering code instead of duplicating it.
pub trait RenderableView {
    fn registry(&self) -> &CardRegistry;
    fn human_side(&self) -> Side;
    fn view(&self) -> Option<&ClientView>;
    fn selected(&self) -> usize;
    fn legal_action_labels(&self) -> Vec<String>;
    /// The running action log. Both paths have one now, so both render the
    /// same four-region layout.
    fn action_log(&self) -> &[String];
}

impl RenderableView for App {
    fn registry(&self) -> &CardRegistry {
        &self.registry
    }

    fn human_side(&self) -> Side {
        self.human_side
    }

    fn view(&self) -> Option<&ClientView> {
        self.view.as_ref()
    }

    fn selected(&self) -> usize {
        self.selected
    }

    fn legal_action_labels(&self) -> Vec<String> {
        self.legal_actions().iter().map(|action| describe_action(action, &self.registry, self.view.as_ref())).collect()
    }

    fn action_log(&self) -> &[String] {
        &self.action_log
    }
}

/// Human-readable label for a `PlayerAction`, resolving `CardId`s to
/// registry titles where available.
///
/// `view` is what resolves an `InstallId` to something a person can read.
/// It is optional because the action log has no view to hand — see
/// `install_label` for what each case renders.
pub fn describe_action(action: &PlayerAction, registry: &CardRegistry, view: Option<&ClientView>) -> String {
    let title = |card_id: &netrunner_core::dsl::CardId| -> String {
        registry.get(card_id).map(|c| c.title.clone()).unwrap_or_else(|| card_id.0.clone())
    };

    // An `InstallId` names a position on the table, not a card, so this
    // renders whatever the *viewer* is entitled to see there:
    //
    // - a card they can identify → its title;
    // - a card the view masks (an unrezzed Corp install) → where it sits,
    //   never its title. Naming it here would reintroduce, in the UI, the
    //   exact leak `InstallId` exists to close;
    // - no view, or an install no longer on the table (a scored agenda) →
    //   the bare id. Only the action log hits this, and only for a card
    //   that has already left; a log line naming it would need the
    //   recorded `GameEvent`s rather than the action alone.
    let install_label = |id: &InstallId| -> String {
        let Some(view) = view else { return format!("install #{}", id.0) };
        for server in &view.corp.servers {
            for card in server.ice.iter().chain(server.root.iter()) {
                if card.install_id != *id {
                    continue;
                }
                return match &card.card {
                    Some(card_id) => title(card_id),
                    None => {
                        let kind = if card.slot == InstallSlot::Ice { "ice" } else { "card" };
                        format!("the unrezzed {kind} at {:?}", server.server)
                    }
                };
            }
        }
        match view.runner.rig.iter().find(|c| c.install_id == *id) {
            Some(rig_card) => title(&rig_card.card),
            None => format!("install #{}", id.0),
        }
    };

    match action {
        PlayerAction::GainCreditClick { side } => format!("Gain 1 credit ({side:?})"),
        PlayerAction::DrawCardClick => "Draw a card".to_string(),
        PlayerAction::InstallCard { card_id, zone, slot } => {
            format!("Install {} into {:?} ({:?})", title(card_id), zone, slot)
        }
        PlayerAction::RezIce { ice } => format!("Rez {}", install_label(ice)),
        PlayerAction::InitiateRun { server } => format!("Run {server:?}"),
        PlayerAction::ContinueRun => "Continue run".to_string(),
        PlayerAction::JackOut => "Jack out".to_string(),
        PlayerAction::CompleteRun => "Complete run".to_string(),
        PlayerAction::PlayEvent { card_id } => format!("Play {}", title(card_id)),
        PlayerAction::PlayOperation { card_id } => format!("Play {}", title(card_id)),
        PlayerAction::InstallHardware { card_id } => format!("Install {}", title(card_id)),
        PlayerAction::InstallProgram { card_id, .. } => format!("Install {}", title(card_id)),
        PlayerAction::InstallResource { card_id } => format!("Install {}", title(card_id)),
        PlayerAction::InstallProgramOnIce { card_id, host, .. } => {
            format!("Install {} onto {}", title(card_id), install_label(host))
        }
        PlayerAction::BreakSubroutine { ice_id, subroutine_index } => {
            format!("Break subroutine {subroutine_index} on {}", title(ice_id))
        }
        PlayerAction::BreakSubroutineWithClick { ice_id, subroutine_index } => {
            format!("Break subroutine {subroutine_index} on {} (spend a click)", title(ice_id))
        }
        PlayerAction::EndTurn => "End turn".to_string(),
        PlayerAction::DiscardCard { card_id } => format!("Discard {}", title(card_id)),
        PlayerAction::KeepHand => "Keep hand".to_string(),
        PlayerAction::TakeMulligan => "Mulligan".to_string(),
        PlayerAction::ActivateAbility { target, ability_index } => {
            format!("Activate ability {ability_index} on {}", install_label(target))
        }
        PlayerAction::AdvanceCard { target } => format!("Advance {}", install_label(target)),
        PlayerAction::ScoreAgenda { target } => format!("Score {}", install_label(target)),
        PlayerAction::RemoveTag => "Remove a tag".to_string(),
        // Spells out the click cost: it is the Corp's whole turn, which is
        // not obvious from the name alone at the point of choosing it.
        PlayerAction::PurgeVirusCounters => "Purge virus counters (3 clicks)".to_string(),
        PlayerAction::TrashResource { target } => format!("Trash {}", install_label(target)),
        PlayerAction::SelectCardToAccess { card_id } => format!("Access {}", title(card_id)),
        PlayerAction::StealAgenda { card_id } => format!("Steal {}", title(card_id)),
        PlayerAction::TrashAccessedCard { card_id } => format!("Trash {}", title(card_id)),
        PlayerAction::PassAccessedCard { card_id } => format!("Pass on {}", title(card_id)),
        PlayerAction::PayAccessTrigger { card_id } => format!("Pay to avoid {}'s trigger", title(card_id)),
        PlayerAction::DeclineAccessTrigger { card_id } => format!("Decline {}'s trigger", title(card_id)),
        PlayerAction::PassPriority { side } => format!("Pass priority ({side:?})"),
        PlayerAction::SubmitCorpTraceBid { amount } => format!("Bid {amount} (Corp trace)"),
        PlayerAction::SubmitRunnerTraceBid { amount } => format!("Bid {amount} (Runner trace)"),
        PlayerAction::AcceptPendingPaidChoice { cost_option_index: None } => "Accept".to_string(),
        PlayerAction::AcceptPendingPaidChoice { cost_option_index: Some(i) } => format!("Accept (option {i})"),
        PlayerAction::DeclinePendingPaidChoice => "Decline".to_string(),
        PlayerAction::ResolvePendingChoice { option_index } => format!("Choose option {option_index}"),
        // A position, deliberately not resolved to a card: the zone it
        // indexes may hold cards this viewer cannot identify, and the
        // selection prompt renders the zone alongside this list anyway.
        PlayerAction::ToggleCardSelection { position } => format!("Toggle selection of card {position}"),
        PlayerAction::ConfirmCardSelection => "Confirm selection".to_string(),
        PlayerAction::ChooseServerForPendingDecision { server } => format!("Choose {server:?}"),
        PlayerAction::ChooseTriggerToResolve { card_id } => format!("Resolve {} first", title(card_id)),
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
