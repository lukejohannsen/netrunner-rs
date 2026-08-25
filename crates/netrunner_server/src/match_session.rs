//! An authoritative single-match host: owns the real `GameState`, resolves
//! each decision to whichever side actually has one pending
//! (`netrunner_core::rules::current_actor`), gets that side's action either
//! from an embedded `BotAgent` (no channel involved at all) or by awaiting
//! a `ClientMessage` on that side's channel, applies it, and pushes a fresh
//! masked `ClientView` to every channel-backed side. Direct generalization
//! of `netrunner_cli::App::drive_bots` (Phase 1) — same shape, decoupled
//! from any UI/terminal specifics and speaking `ClientView` instead of raw
//! state.

use tokio::sync::mpsc;

use netrunner_bots::BotAgent;
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{apply_action, current_actor, GameEvent, GamePhase, GameState, Side};
use netrunner_core::view::build_client_view;

use crate::protocol::{ClientMessage, GameEndReason, ServerMessage};

/// Guard against a stalled/looping game running forever — same budget as
/// `netrunner_cli::headless::MAX_TICKS`/`App::MAX_BOT_STEPS`.
const MAX_STEPS: u32 = 10_000;

pub enum PlayerSlot {
    Bot(Box<dyn BotAgent>),
    Channel { tx: mpsc::UnboundedSender<ServerMessage>, rx: mpsc::UnboundedReceiver<ClientMessage> },
}

pub struct MatchSession {
    state: GameState,
    registry: CardRegistry,
    corp: PlayerSlot,
    runner: PlayerSlot,
}

enum Step {
    Act(netrunner_core::rules::PlayerAction),
    Surrender,
    Disconnected,
    LoopAgain,
}

impl MatchSession {
    pub fn new(state: GameState, registry: CardRegistry, corp: PlayerSlot, runner: PlayerSlot) -> Self {
        MatchSession { state, registry, corp, runner }
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

        for _ in 0..MAX_STEPS {
            if matches!(self.state.phase, GamePhase::GameOver(_)) {
                break;
            }
            let Some(side) = current_actor(&self.state) else { break };

            let step = {
                let MatchSession { state, registry, corp, runner } = &mut self;
                let slot = match side {
                    Side::Corp => &mut *corp,
                    Side::Runner => &mut *runner,
                };
                match slot {
                    PlayerSlot::Bot(agent) => {
                        let view = build_client_view(state, registry, side);
                        Step::Act(agent.select_action(&view, registry))
                    }
                    PlayerSlot::Channel { rx, .. } => match rx.recv().await {
                        Some(ClientMessage::SubmitAction(action)) => Step::Act(action),
                        Some(ClientMessage::Surrender) => Step::Surrender,
                        Some(ClientMessage::Connect { .. }) => Step::LoopAgain,
                        None => Step::Disconnected,
                    },
                }
            };

            match step {
                Step::LoopAgain => continue,
                Step::Disconnected => break,
                Step::Surrender => {
                    self.send_game_ended(side.other(), GameEndReason::Surrender);
                    break;
                }
                Step::Act(action) => match apply_action(&self.state, &self.registry, action) {
                    Ok((next, events)) => {
                        self.state = next;
                        self.broadcast_state_updates();
                        if let GamePhase::GameOver(winner) = self.state.phase {
                            let reason = classify_end_reason(&events, winner, &self.state);
                            self.send_game_ended(winner, reason);
                            break;
                        }
                    }
                    Err(error) => {
                        // A bot slot only ever picks from `view.
                        // legal_actions`, so this should never actually
                        // happen there — only a misbehaving channel client
                        // can trigger it.
                        if let PlayerSlot::Channel { tx, .. } = self.slot(side) {
                            let _ = tx.send(ServerMessage::ActionRejected { reason: format!("{error:?}") });
                        }
                    }
                },
            }
        }
        self.state
    }

    fn slot(&self, side: Side) -> &PlayerSlot {
        match side {
            Side::Corp => &self.corp,
            Side::Runner => &self.runner,
        }
    }

    fn broadcast_state_updates(&self) {
        for side in [Side::Corp, Side::Runner] {
            if let PlayerSlot::Channel { tx, .. } = self.slot(side) {
                let view = build_client_view(&self.state, &self.registry, side);
                let _ = tx.send(ServerMessage::StateUpdate(Box::new(view)));
            }
        }
    }

    fn send_game_ended(&self, winner: Side, reason: GameEndReason) {
        for side in [Side::Corp, Side::Runner] {
            if let PlayerSlot::Channel { tx, .. } = self.slot(side) {
                let _ = tx.send(ServerMessage::GameEnded { winner, reason });
            }
        }
    }
}

/// Best-effort classification — `netrunner_core` doesn't track *why*
/// `GameOver` happened, only that it did (see `GameEndReason`'s doc
/// comment). A `RunnerFlatlined` event in this action's trailing events
/// means Flatline; an empty Corp R&D at a Runner win means the Corp decked
/// out attempting their mandatory draw (the one other unprompted win path —
/// see `turn::enter_start_of_turn`'s doc comment); anything else defaults
/// to the ordinary agenda-point threshold.
fn classify_end_reason(events: &[GameEvent], winner: Side, state: &GameState) -> GameEndReason {
    if events.iter().any(|event| matches!(event, GameEvent::RunnerFlatlined)) {
        return GameEndReason::Flatline;
    }
    if winner == Side::Runner && state.corp.r_and_d.is_empty() {
        return GameEndReason::Deckout;
    }
    GameEndReason::AgendaThreshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::RandomAgent;
    use netrunner_core::rules::{GameState as CoreGameState, PlayerAction};

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
        assert!(matches!(final_state.phase, GamePhase::GameOver(_)), "expected GameOver within {MAX_STEPS} steps");
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
