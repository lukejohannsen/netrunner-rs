//! An authoritative single-match host: owns a `netrunner_session::Session`,
//! pumps it inside `tokio`, and pushes a fresh masked `ClientView` to every
//! channel-backed side after each applied action.
//!
//! **The decision loop itself no longer lives here.** `current_actor` →
//! action → `apply_action` → `GameOver`, and the `MAX_STEPS` budget, are all
//! `netrunner_session::Session`'s; this module is purely the *async pump*
//! around it — the part that knows about channels, rejection messages and
//! surrenders. That split is the point of the shared driver: a bot seat and
//! a channel seat differ in who supplies the action, never in how the rules
//! advance.

use tokio::sync::mpsc;

use netrunner_bots::BotAgent;
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{GameState, Side};
use netrunner_core::view::build_client_view;
use netrunner_session::{Seat, Session, SessionStep};

use crate::protocol::{ClientMessage, GameEndReason, ServerMessage};

/// How a caller describes one side when constructing a `MatchSession`.
///
/// Kept as this crate's own type rather than exposing
/// `netrunner_session::Seat` directly: a `Seat::External` says only "someone
/// else supplies the action", while `Channel` also carries the two channel
/// halves this pump needs. Construction splits one into the other.
pub enum PlayerSlot {
    Bot(Box<dyn BotAgent>),
    Channel { tx: mpsc::UnboundedSender<ServerMessage>, rx: mpsc::UnboundedReceiver<ClientMessage> },
}

/// The channel halves for a `PlayerSlot::Channel` seat. `None` for a bot
/// seat, which never goes through a channel at all.
struct ChannelSeat {
    tx: mpsc::UnboundedSender<ServerMessage>,
    rx: mpsc::UnboundedReceiver<ClientMessage>,
}

impl PlayerSlot {
    fn split(self) -> (Seat, Option<ChannelSeat>) {
        match self {
            PlayerSlot::Bot(agent) => (Seat::Agent(agent), None),
            PlayerSlot::Channel { tx, rx } => (Seat::External, Some(ChannelSeat { tx, rx })),
        }
    }
}

pub struct MatchSession {
    session: Session,
    corp: Option<ChannelSeat>,
    runner: Option<ChannelSeat>,
}

impl MatchSession {
    pub fn new(state: GameState, registry: CardRegistry, corp: PlayerSlot, runner: PlayerSlot) -> Self {
        let (corp_seat, corp_channel) = corp.split();
        let (runner_seat, runner_channel) = runner.split();
        MatchSession {
            session: Session::new(state, registry, corp_seat, runner_seat),
            corp: corp_channel,
            runner: runner_channel,
        }
    }

    /// Runs the match to completion (or until disconnected / the step
    /// budget is exhausted) and returns the final `GameState` — callers
    /// check `matches!(final.phase, GamePhase::GameOver(_))` to tell a real
    /// conclusion from an early exit.
    pub async fn run(mut self) -> GameState {
        // Without this, a channel-backed side would have nothing to act on
        // for its very first decision: every subsequent `StateUpdate` is
        // only sent *after* an action is applied, but nothing has been
        // applied yet — the loop below would immediately block on that
        // side's `rx.recv().await` while the client blocks waiting for a
        // view to submit an action against. Deadlock without this.
        self.broadcast_state_updates();

        loop {
            match self.session.step() {
                // A bot seat resolved itself; tell the channel sides what
                // happened. Broadcasting on `Applied` rather than on the
                // next `Awaiting` is deliberate: a window or a decision can
                // hand several consecutive actions to one side, and the
                // *other* side's board must not freeze meanwhile.
                SessionStep::Applied { .. } => self.broadcast_applied(),
                SessionStep::Awaiting { side, .. } => {
                    // The recv borrow has to end before `submit` touches
                    // `self.session`, so take the message first.
                    let message = match self.channel_mut(side) {
                        Some(seat) => seat.rx.recv().await,
                        // `Awaiting` only ever names a `Seat::External`,
                        // and every External seat here came from a
                        // `PlayerSlot::Channel`.
                        None => unreachable!("a bot seat never yields Awaiting"),
                    };
                    match message {
                        Some(ClientMessage::SubmitAction(action)) => match self.session.submit(action) {
                            Ok(()) => self.broadcast_applied(),
                            // A bot slot only ever picks from
                            // `view.legal_actions`, so this is only
                            // reachable for a misbehaving channel client.
                            // The session leaves the state untouched and
                            // this same side still awaiting, so the next
                            // `step` re-offers the decision.
                            Err(error) => {
                                if let Some(seat) = self.channel_mut(side) {
                                    let _ = seat.tx.send(ServerMessage::ActionRejected { reason: format!("{error:?}") });
                                }
                            }
                        },
                        Some(ClientMessage::Surrender) => {
                            // Not a rules outcome — the engine never
                            // reaches `GameOver` here, so only this pump
                            // can report it.
                            self.send_game_ended(side.other(), GameEndReason::Surrender);
                            break;
                        }
                        Some(ClientMessage::Connect { .. }) => continue,
                        None => break,
                    }
                }
                SessionStep::Ended { winner, reason } => {
                    self.send_game_ended(winner, reason);
                    break;
                }
                SessionStep::Stalled(_) => break,
            }
        }
        self.session.into_parts().0
    }

    /// Everything a channel seat needs after an action resolves: the new
    /// board, then the log line describing how it got there.
    fn broadcast_applied(&self) {
        self.broadcast_state_updates();
        if let Some(entry) = self.session.last_entry() {
            self.broadcast(ServerMessage::ActionLog(Box::new(entry.clone())));
        }
    }

    fn channel_mut(&mut self, side: Side) -> Option<&mut ChannelSeat> {
        match side {
            Side::Corp => self.corp.as_mut(),
            Side::Runner => self.runner.as_mut(),
        }
    }

    fn channel(&self, side: Side) -> Option<&ChannelSeat> {
        match side {
            Side::Corp => self.corp.as_ref(),
            Side::Runner => self.runner.as_ref(),
        }
    }

    fn broadcast(&self, message: ServerMessage) {
        for side in [Side::Corp, Side::Runner] {
            if let Some(seat) = self.channel(side) {
                let _ = seat.tx.send(message.clone());
            }
        }
    }

    fn broadcast_state_updates(&self) {
        for side in [Side::Corp, Side::Runner] {
            if let Some(seat) = self.channel(side) {
                let view = build_client_view(self.session.state(), self.session.registry(), side);
                let _ = seat.tx.send(ServerMessage::StateUpdate(Box::new(view)));
            }
        }
    }

    fn send_game_ended(&self, winner: Side, reason: GameEndReason) {
        self.broadcast(ServerMessage::GameEnded { winner, reason });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::RandomAgent;
    use netrunner_core::rules::{GamePhase, GameState as CoreGameState, PlayerAction};

    use crate::fixtures;

    fn bot_vs_bot() -> MatchSession {
        let registry = fixtures::kate_vs_hb_registry();
        let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
        let (state, _events) = CoreGameState::setup(&corp_deck, &runner_deck, &registry, 1).expect("legal decks set up cleanly");
        MatchSession::new(
            state,
            registry,
            PlayerSlot::Bot(Box::new(RandomAgent::new(1))),
            PlayerSlot::Bot(Box::new(RandomAgent::new(2))),
        )
    }

    #[tokio::test]
    async fn bot_vs_bot_reaches_game_over_within_the_step_budget() {
        let session = bot_vs_bot();
        let final_state = session.run().await;
        assert!(matches!(final_state.phase, GamePhase::GameOver(_)), "expected GameOver within the step budget");
    }

    #[tokio::test]
    async fn channel_side_receives_an_initial_view_before_submitting_anything() {
        let registry = fixtures::kate_vs_hb_registry();
        let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
        let (state, _events) = CoreGameState::setup(&corp_deck, &runner_deck, &registry, 7).expect("legal decks set up cleanly");

        let (corp_tx, mut corp_rx) = mpsc::unbounded_channel();
        let (_corp_client_tx, corp_client_rx) = mpsc::unbounded_channel();
        let corp_slot = PlayerSlot::Channel { tx: corp_tx, rx: corp_client_rx };
        let runner_slot = PlayerSlot::Bot(Box::new(RandomAgent::new(8)));

        let session = MatchSession::new(state, registry, corp_slot, runner_slot);
        let handle = tokio::spawn(session.run());

        let first = corp_rx.recv().await.expect("initial StateUpdate sent without waiting for a submitted action first");
        match first {
            ServerMessage::StateUpdate(view) => assert!(!view.legal_actions.is_empty()),
            other => panic!("expected an initial StateUpdate, got {other:?}"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn channel_side_rejects_a_bad_action_and_keeps_running() {
        let registry = fixtures::kate_vs_hb_registry();
        let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
        let (state, _events) = CoreGameState::setup(&corp_deck, &runner_deck, &registry, 3).expect("legal decks set up cleanly");

        let (corp_tx, mut corp_rx) = mpsc::unbounded_channel();
        let (corp_client_tx, corp_client_rx) = mpsc::unbounded_channel();
        let corp_slot = PlayerSlot::Channel { tx: corp_tx, rx: corp_client_rx };
        let runner_slot = PlayerSlot::Bot(Box::new(RandomAgent::new(4)));

        let session = MatchSession::new(state, registry, corp_slot, runner_slot);
        let handle = tokio::spawn(session.run());

        let initial = corp_rx.recv().await.unwrap();
        assert!(matches!(initial, ServerMessage::StateUpdate(_)));

        // Corp's Mulligan decision is a bad action (illegal in Mulligan
        // phase) — should get rejected, not panic or hang the session.
        corp_client_tx.send(ClientMessage::SubmitAction(PlayerAction::EndTurn)).unwrap();
        let rejection = corp_rx.recv().await.unwrap();
        assert!(matches!(rejection, ServerMessage::ActionRejected { .. }));

        // A legal follow-up still gets accepted and the session keeps going.
        corp_client_tx.send(ClientMessage::SubmitAction(PlayerAction::KeepHand)).unwrap();
        let accepted = corp_rx.recv().await.unwrap();
        assert!(matches!(accepted, ServerMessage::StateUpdate(_)));

        drop(corp_client_tx);
        handle.abort();
    }

    /// The network path's game log — the thing `MatchSession` could not
    /// offer at all while `MatchHistory` lived in `netrunner_single_player`
    /// and each action's events were dropped after classifying the ending.
    #[tokio::test]
    async fn channel_side_receives_a_log_entry_for_every_resolved_action() {
        let registry = fixtures::kate_vs_hb_registry();
        let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
        let (state, _events) = CoreGameState::setup(&corp_deck, &runner_deck, &registry, 3).expect("legal decks set up cleanly");

        let (corp_tx, mut corp_rx) = mpsc::unbounded_channel();
        let (corp_client_tx, corp_client_rx) = mpsc::unbounded_channel();
        let corp_slot = PlayerSlot::Channel { tx: corp_tx, rx: corp_client_rx };
        let runner_slot = PlayerSlot::Bot(Box::new(RandomAgent::new(4)));

        let session = MatchSession::new(state, registry, corp_slot, runner_slot);
        let handle = tokio::spawn(session.run());

        assert!(matches!(corp_rx.recv().await.unwrap(), ServerMessage::StateUpdate(_)));
        corp_client_tx.send(ClientMessage::SubmitAction(PlayerAction::KeepHand)).unwrap();

        assert!(matches!(corp_rx.recv().await.unwrap(), ServerMessage::StateUpdate(_)));
        match corp_rx.recv().await.unwrap() {
            ServerMessage::ActionLog(entry) => {
                assert_eq!(entry.side, Side::Corp);
                assert_eq!(entry.action, PlayerAction::KeepHand);
                assert_eq!(entry.turn_number, 0, "mulligan actions are turn 0");
            }
            other => panic!("expected an ActionLog after the StateUpdate, got {other:?}"),
        }

        drop(corp_client_tx);
        handle.abort();
    }

    #[tokio::test]
    async fn state_updates_never_leak_the_other_sides_hidden_cards() {
        let registry = fixtures::kate_vs_hb_registry();
        let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
        let (mut state, _events) = CoreGameState::setup(&corp_deck, &runner_deck, &registry, 5).expect("legal decks set up cleanly");
        state.phase = GamePhase::Action(Side::Corp);

        let corp_view = build_client_view(&state, &registry, Side::Corp);
        let runner_view = build_client_view(&state, &registry, Side::Runner);

        assert!(runner_view.corp.hq_cards.is_none());
        assert!(corp_view.runner.grip_cards.is_none());
        assert_eq!(corp_view.corp.hq_cards, Some(state.corp.hq.clone()));
    }
}
