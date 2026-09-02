//! An authoritative single-match host: owns a `netrunner_session::Session`,
//! pumps it inside `tokio`, and pushes a fresh masked `ClientView` to every
//! channel-backed side after each applied action.
//!
//! **The decision loop itself no longer lives here.** `current_actor` →
//! action → `apply_action` → `GameOver`, and the `MAX_STEPS` budget, are all
//! `netrunner_session::Session`'s; this module is purely the *async pump*
//! around it — the part that knows about channels, rejection messages,
//! surrenders and dropped connections. That split is the point of the
//! shared driver: a bot seat and a channel seat differ in who supplies the
//! action, never in how the rules advance.
//!
//! **A channel seat outlives the channel that first backed it.** The pair
//! of `mpsc` halves behind a `PlayerSlot::Channel` is one WebSocket's
//! worth of connection; when it closes the seat is *detached*, not gone,
//! and a `ReattachHandle` can hand the same seat a fresh pair — after which
//! the session resends that seat its view, the full snapshot it would have
//! got anyway. The alternative, a relay task between socket and session
//! that swaps sockets underneath a fixed channel, was rejected because the
//! relay could not produce the resync: only the session holds the state.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;

use netrunner_bots::BotAgent;
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{GameState, Side};
use netrunner_core::view::build_client_view;
use netrunner_session::{Seat, Session, SessionStep};

use crate::protocol::{ClientMessage, GameEndReason, ServerMessage};

/// How long the session waits on a detached seat whose action it needs
/// before ruling the match a forfeit. Measured from the moment the seat is
/// *needed*, not from the disconnect: a Corp that dropped during a slow
/// Runner turn has cost nobody anything yet, and refusing it back because
/// the *other* player took ten minutes would be a strange rule.
pub const DEFAULT_RECONNECT_GRACE: Duration = Duration::from_secs(30);

/// A seat's cost of not answering: every awaited decision gets this long
/// before the match is awarded to the other side (`GameEndReason::
/// TimedOut`). `None` — the default — means no clock at all.
///
/// **Per decision, not per turn.** `await_seat` is entered once per
/// `SessionStep::Awaiting`, which is exactly "this player has been asked
/// something and has not answered"; a per-turn chess clock would need
/// accounting across steps and a rule for the paid-ability windows the
/// *opponent* opens during your turn. **A rejected action does not restart
/// it** — the deadline is kept on the session and cleared only when an
/// action is applied, or a client could submit garbage to buy time. **A
/// reattach does not restart it either** (only the reconnect grace is
/// reset then), or dropping the socket would. And when it runs out the
/// pump forfeits rather than playing something on the seat's behalf: there
/// is no universally legal action to play — `EndTurn` is illegal under a
/// pending decision — and choosing one would make the host a player.
pub type TurnTimeout = Option<Duration>;

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
    /// Set the moment either half is seen closed — `rx` yielding `None`,
    /// or a send failing — and cleared only by a reattach. A closed `rx`
    /// returns `None` on every poll, so an attached-but-dead seat would
    /// spin; this flag is what parks it instead.
    detached: bool,
}

impl ChannelSeat {
    fn send(&mut self, message: ServerMessage) {
        if self.tx.send(message).is_err() {
            self.detached = true;
        }
    }
}

impl PlayerSlot {
    fn split(self) -> (Seat, Option<ChannelSeat>) {
        match self {
            PlayerSlot::Bot(agent) => (Seat::Agent(agent), None),
            PlayerSlot::Channel { tx, rx } => (Seat::External, Some(ChannelSeat { tx, rx, detached: false })),
        }
    }
}

/// A fresh pair of channel halves for one seat, queued by a
/// `ReattachHandle` and applied by the running session.
struct Reattach {
    side: Side,
    tx: mpsc::UnboundedSender<ServerMessage>,
    rx: mpsc::UnboundedReceiver<ClientMessage>,
}

/// Lets whoever holds the sockets — `serve` — give a seat a new channel
/// pair while the match runs. Cloneable, so one handle per seat can sit in
/// a token registry; addressed by `Side` because that is all the session
/// knows a seat by. Tokens, match ids and who is allowed to present them
/// are the transport's business, not this pump's.
#[derive(Clone)]
pub struct ReattachHandle(mpsc::UnboundedSender<Reattach>);

/// The session behind a `ReattachHandle` has finished — by result,
/// surrender, stall, or the grace period on this very seat running out —
/// and dropped its end of the control channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchOver;

impl std::fmt::Display for MatchOver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the match is over")
    }
}

impl std::error::Error for MatchOver {}

impl ReattachHandle {
    /// Replaces `side`'s channel halves. **The newest connection wins**: if
    /// the seat is still attached (a second client presenting the same
    /// token, or a socket the host has not yet noticed is dead), the old
    /// halves are dropped, which closes the old bridge's channel ends and
    /// so its socket. The session answers on the new `tx` with a fresh
    /// `StateUpdate`; anything the caller wants delivered *before* that
    /// (the `MatchJoined` a resuming client is waiting for) it sends on
    /// `tx` before calling this — the channel preserves the order.
    pub fn reattach(
        &self,
        side: Side,
        tx: mpsc::UnboundedSender<ServerMessage>,
        rx: mpsc::UnboundedReceiver<ClientMessage>,
    ) -> Result<(), MatchOver> {
        self.0.send(Reattach { side, tx, rx }).map_err(|_| MatchOver)
    }

    /// Whether the session is still running. A `false` is final; a `true`
    /// can be stale by the time `reattach` is called, which is why that
    /// returns `MatchOver` itself.
    pub fn is_live(&self) -> bool {
        !self.0.is_closed()
    }
}

pub struct MatchSession {
    session: Session,
    corp: Option<ChannelSeat>,
    runner: Option<ChannelSeat>,
    reattach_tx: mpsc::UnboundedSender<Reattach>,
    reattach_rx: mpsc::UnboundedReceiver<Reattach>,
    reconnect_grace: Duration,
    turn_timeout: TurnTimeout,
    /// When the decision currently awaited runs out, if a clock is on.
    /// Lives here rather than in `await_seat` so that a rejected action —
    /// which re-enters `await_seat` for the same decision — finds the
    /// clock already running. Cleared by `run` when an action applies.
    decision_deadline: Option<Instant>,
}

/// What `await_seat` came back with.
enum SeatEvent {
    Message(ClientMessage),
    /// Detached for the whole grace period while the session needed it.
    Forfeited,
    /// Attached, or not, but silent for the whole `TurnTimeout`.
    TimedOut,
}

impl MatchSession {
    pub fn new(state: GameState, registry: CardRegistry, corp: PlayerSlot, runner: PlayerSlot) -> Self {
        let (corp_seat, corp_channel) = corp.split();
        let (runner_seat, runner_channel) = runner.split();
        let (reattach_tx, reattach_rx) = mpsc::unbounded_channel();
        MatchSession {
            session: Session::new(state, registry, corp_seat, runner_seat),
            corp: corp_channel,
            runner: runner_channel,
            reattach_tx,
            reattach_rx,
            reconnect_grace: DEFAULT_RECONNECT_GRACE,
            turn_timeout: None,
            decision_deadline: None,
        }
    }

    /// See `TurnTimeout`. `None` (the default) runs without a clock.
    pub fn with_turn_timeout(mut self, timeout: TurnTimeout) -> Self {
        self.turn_timeout = timeout;
        self
    }

    /// Overrides `DEFAULT_RECONNECT_GRACE`. Zero means a dropped seat
    /// forfeits the moment its action is needed.
    pub fn with_reconnect_grace(mut self, grace: Duration) -> Self {
        self.reconnect_grace = grace;
        self
    }

    /// A handle for reattaching seats while `run` is in progress. Take it
    /// before calling `run`, which consumes the session.
    pub fn reattach_handle(&self) -> ReattachHandle {
        ReattachHandle(self.reattach_tx.clone())
    }

    /// Runs the match to completion (or until a needed seat stays
    /// disconnected past the grace period / the step budget is exhausted)
    /// and returns the final `GameState` — callers check
    /// `matches!(final.phase, GamePhase::GameOver(_))` to tell a real
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
                SessionStep::Applied { .. } => {
                    self.decision_deadline = None;
                    self.broadcast_applied();
                }
                SessionStep::Awaiting { side, .. } => {
                    let message = match self.await_seat(side).await {
                        SeatEvent::Message(message) => message,
                        SeatEvent::Forfeited => {
                            // Not a rules outcome — the engine never
                            // reaches `GameOver` here, so only this pump
                            // can report it. The seat that vanished gets
                            // the message too, uselessly; its `send` is
                            // already a no-op.
                            self.send_game_ended(side.other(), GameEndReason::Disconnected);
                            break;
                        }
                        SeatEvent::TimedOut => {
                            self.send_game_ended(side.other(), GameEndReason::TimedOut);
                            break;
                        }
                    };
                    match message {
                        ClientMessage::SubmitAction(action) => match self.session.submit(action) {
                            Ok(()) => {
                                self.decision_deadline = None;
                                self.broadcast_applied();
                            }
                            // A bot slot only ever picks from
                            // `view.legal_actions`, so this is only
                            // reachable for a misbehaving channel client.
                            // The session leaves the state untouched and
                            // this same side still awaiting, so the next
                            // `step` re-offers the decision.
                            Err(error) => {
                                self.send_to(side, ServerMessage::ActionRejected { reason: format!("{error:?}") });
                            }
                        },
                        ClientMessage::Surrender => {
                            self.send_game_ended(side.other(), GameEndReason::Surrender);
                            break;
                        }
                        // Handshake messages belong to the transport; one
                        // that reaches the session is a client repeating
                        // itself and is ignored.
                        ClientMessage::Connect { .. } | ClientMessage::Resume { .. } | ClientMessage::ListMatches => continue,
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

    /// The awaiting seat's next message, servicing reattachments for
    /// *either* seat meanwhile — the non-acting side's client can come
    /// back at any time, and must get its view then, not when its turn
    /// comes round. The grace clock starts when this seat is observed
    /// detached (on entry, or when its `rx` closes under us) and is reset
    /// by a reattach, so a client that keeps dropping and returning keeps
    /// the match alive — it is, after all, present.
    async fn await_seat(&mut self, side: Side) -> SeatEvent {
        let mut deadline: Option<Instant> = None;
        if self.decision_deadline.is_none()
            && let Some(timeout) = self.turn_timeout
        {
            self.decision_deadline = Some(Instant::now() + timeout);
            self.broadcast(ServerMessage::DecisionClock { side, remaining: timeout });
        }
        let decision_deadline = self.decision_deadline;
        loop {
            let MatchSession { session, corp, runner, reattach_rx, reconnect_grace, .. } = self;
            let seat = match side {
                Side::Corp => corp.as_mut(),
                Side::Runner => runner.as_mut(),
            }
            // `Awaiting` only ever names a `Seat::External`, and every
            // External seat here came from a `PlayerSlot::Channel`.
            .expect("a bot seat never yields Awaiting");
            if seat.detached && deadline.is_none() {
                deadline = Some(Instant::now() + *reconnect_grace);
            }

            tokio::select! {
                // Reattachments first: a queued one must beat a deadline
                // that expired while the pump was busy elsewhere.
                biased;
                reattach = reattach_rx.recv() => {
                    let Reattach { side: reattached, tx, rx } = reattach.expect("the session holds a sender");
                    let slot = match reattached {
                        Side::Corp => corp.as_mut(),
                        Side::Runner => runner.as_mut(),
                    };
                    // A bot seat has no channel to replace; the halves
                    // drop here and the presenting client's socket closes.
                    if let Some(slot) = slot {
                        *slot = ChannelSeat { tx, rx, detached: false };
                        let view = build_client_view(session.state(), session.registry(), reattached);
                        slot.send(ServerMessage::StateUpdate(Box::new(view)));
                        if let Some(decision_deadline) = decision_deadline {
                            let remaining = decision_deadline.saturating_duration_since(Instant::now());
                            slot.send(ServerMessage::DecisionClock { side, remaining });
                        }
                        if reattached == side {
                            deadline = None;
                        }
                    }
                }
                message = Self::recv_attached(seat) => match message {
                    Some(message) => return SeatEvent::Message(message),
                    None => {
                        seat.detached = true;
                        deadline = Some(Instant::now() + *reconnect_grace);
                    }
                },
                _ = tokio::time::sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => {
                    return SeatEvent::Forfeited;
                }
                // Both clocks can run at once for a detached seat;
                // whichever fires first names the reason.
                _ = tokio::time::sleep_until(decision_deadline.unwrap_or_else(Instant::now)), if decision_deadline.is_some() => {
                    return SeatEvent::TimedOut;
                }
            }
        }
    }

    /// `rx.recv()` for an attached seat; never resolves for a detached one,
    /// so `await_seat`'s select waits on the reattach channel and the
    /// deadline alone.
    async fn recv_attached(seat: &mut ChannelSeat) -> Option<ClientMessage> {
        if seat.detached {
            std::future::pending().await
        } else {
            seat.rx.recv().await
        }
    }

    /// Everything a channel seat needs after an action resolves: the new
    /// board, then the log line describing how it got there — each seat's
    /// own copy of both. The log used to be one `broadcast` of the raw
    /// `HistoryEntry`, which handed the Runner the Corp's facedown install
    /// by name; it is masked here, right after the action, because
    /// `last_entry_for` reads concealment off the state this action left.
    fn broadcast_applied(&mut self) {
        self.broadcast_state_updates();
        for side in [Side::Corp, Side::Runner] {
            if let Some(entry) = self.session.last_entry_for(side) {
                self.send_to(side, ServerMessage::ActionLog(Box::new(entry)));
            }
        }
    }

    fn send_to(&mut self, side: Side, message: ServerMessage) {
        let seat = match side {
            Side::Corp => self.corp.as_mut(),
            Side::Runner => self.runner.as_mut(),
        };
        if let Some(seat) = seat {
            seat.send(message);
        }
    }

    fn broadcast(&mut self, message: ServerMessage) {
        for side in [Side::Corp, Side::Runner] {
            self.send_to(side, message.clone());
        }
    }

    fn broadcast_state_updates(&mut self) {
        for side in [Side::Corp, Side::Runner] {
            let view = build_client_view(self.session.state(), self.session.registry(), side);
            self.send_to(side, ServerMessage::StateUpdate(Box::new(view)));
        }
    }

    fn send_game_ended(&mut self, winner: Side, reason: GameEndReason) {
        self.broadcast(ServerMessage::GameEnded { winner, reason });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::RandomAgent;
    use netrunner_core::rules::{Clicks, ConcealedAction, GameEvent, GamePhase, GameState as CoreGameState, PlayerAction, PublicAction};

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
                assert_eq!(entry.action, PublicAction::Visible(PlayerAction::KeepHand), "a seat sees its own action whole");
                assert_eq!(entry.turn_number, 0, "mulligan actions are turn 0");
            }
            other => panic!("expected an ActionLog after the StateUpdate, got {other:?}"),
        }

        drop(corp_client_tx);
        handle.abort();
    }

    /// The log is masked per seat at the engine boundary, like the view.
    /// Before this the same `HistoryEntry` went to both seats, so the
    /// Runner's log read "Install Palisade into Remote(0)" for a card its
    /// own `StateUpdate` had just rendered as `card: None`.
    #[tokio::test]
    async fn the_action_log_never_names_the_other_sides_hidden_cards() {
        let registry = fixtures::kate_vs_hb_registry();
        let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
        let (mut state, _events) = CoreGameState::setup(&corp_deck, &runner_deck, &registry, 5).expect("legal decks set up cleanly");
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.resources.clicks = Clicks(3);

        let (corp_tx, mut corp_rx) = mpsc::unbounded_channel();
        let (corp_client_tx, corp_client_rx) = mpsc::unbounded_channel();
        let (runner_tx, mut runner_rx) = mpsc::unbounded_channel();
        let (_runner_client_tx, runner_client_rx) = mpsc::unbounded_channel();
        let session = MatchSession::new(
            state,
            registry,
            PlayerSlot::Channel { tx: corp_tx, rx: corp_client_rx },
            PlayerSlot::Channel { tx: runner_tx, rx: runner_client_rx },
        );
        let handle = tokio::spawn(session.run());

        let install = match corp_rx.recv().await.unwrap() {
            ServerMessage::StateUpdate(view) => view
                .legal_actions
                .iter()
                .find(|action| matches!(action, PlayerAction::InstallCard { .. }))
                .cloned()
                .expect("a Corp with three clicks and an opening hand can install something"),
            other => panic!("expected the initial StateUpdate, got {other:?}"),
        };
        assert!(matches!(runner_rx.recv().await.unwrap(), ServerMessage::StateUpdate(_)));
        corp_client_tx.send(ClientMessage::SubmitAction(install.clone())).unwrap();

        assert!(matches!(corp_rx.recv().await.unwrap(), ServerMessage::StateUpdate(_)));
        match corp_rx.recv().await.unwrap() {
            ServerMessage::ActionLog(entry) => assert_eq!(entry.action, PublicAction::Visible(install.clone())),
            other => panic!("expected the Corp's ActionLog, got {other:?}"),
        }

        assert!(matches!(runner_rx.recv().await.unwrap(), ServerMessage::StateUpdate(_)));
        match runner_rx.recv().await.unwrap() {
            ServerMessage::ActionLog(entry) => {
                assert!(
                    matches!(entry.action, PublicAction::Concealed(ConcealedAction::InstallCard { .. })),
                    "the Runner's copy names the install's shape, not its card: {:?}",
                    entry.action
                );
                assert!(!entry.events.iter().any(|event| matches!(event, GameEvent::CardInstalled { .. })));
                let PlayerAction::InstallCard { card_id, .. } = &install else { unreachable!() };
                let rendered = format!("{entry:?}");
                assert!(!rendered.contains(&format!("\"{}\"", card_id.0)), "{rendered}");
            }
            other => panic!("expected the Runner's ActionLog, got {other:?}"),
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

/// The reconnection contract at the channel level, without sockets: a seat
/// survives its channel pair, gets its view back on reattach, and forfeits
/// only after the grace period runs out while the session needs it.
#[cfg(test)]
mod reattach_tests {
    use std::time::Duration;

    use super::*;
    use netrunner_bots::RandomAgent;
    use netrunner_core::rules::{GamePhase, GameState as CoreGameState, PlayerAction};

    use crate::fixtures;

    type Halves = (
        mpsc::UnboundedSender<ClientMessage>,
        mpsc::UnboundedReceiver<ServerMessage>,
        PlayerSlot,
    );

    fn channel_slot() -> Halves {
        let (server_tx, client_rx) = mpsc::unbounded_channel();
        let (client_tx, server_rx) = mpsc::unbounded_channel();
        (client_tx, client_rx, PlayerSlot::Channel { tx: server_tx, rx: server_rx })
    }

    fn state(seed: u64) -> (CoreGameState, CardRegistry) {
        let registry = fixtures::kate_vs_hb_registry();
        let (corp_deck, runner_deck) = fixtures::kate_vs_hb_decks();
        let (state, _events) = CoreGameState::setup(&corp_deck, &runner_deck, &registry, seed).expect("legal decks set up cleanly");
        (state, registry)
    }

    async fn expect_state_update(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) {
        match rx.recv().await {
            Some(ServerMessage::StateUpdate(_)) => {}
            other => panic!("expected a StateUpdate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_detached_seat_gets_a_fresh_view_on_reattach_and_play_continues() {
        let (state, registry) = state(11);
        let (corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let session = MatchSession::new(state, registry, corp_slot, PlayerSlot::Bot(Box::new(RandomAgent::new(12))))
            .with_reconnect_grace(Duration::from_secs(60));
        let handle = session.reattach_handle();
        let run = tokio::spawn(session.run());

        expect_state_update(&mut corp_rx).await;
        // The socket "drops": both client halves go away.
        drop(corp_tx);
        drop(corp_rx);

        let (corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let PlayerSlot::Channel { tx, rx } = corp_slot else { unreachable!() };
        handle.reattach(Side::Corp, tx, rx).expect("the match is still running");

        // The resync: a full snapshot, unprompted, on the new channel.
        expect_state_update(&mut corp_rx).await;
        corp_tx.send(ClientMessage::SubmitAction(PlayerAction::KeepHand)).unwrap();
        expect_state_update(&mut corp_rx).await;
        assert!(matches!(corp_rx.recv().await, Some(ServerMessage::ActionLog(_))));

        run.abort();
    }

    #[tokio::test]
    async fn the_non_acting_seat_can_reattach_while_the_other_side_is_awaited() {
        let (state, registry) = state(13);
        let (_corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let (runner_tx, mut runner_rx, runner_slot) = channel_slot();
        let session = MatchSession::new(state, registry, corp_slot, runner_slot).with_reconnect_grace(Duration::from_secs(60));
        let handle = session.reattach_handle();
        let run = tokio::spawn(session.run());

        expect_state_update(&mut corp_rx).await;
        expect_state_update(&mut runner_rx).await;
        // The Corp's mulligan is awaited; the Runner drops meanwhile.
        drop(runner_tx);
        drop(runner_rx);

        let (_runner_tx, mut runner_rx, runner_slot) = channel_slot();
        let PlayerSlot::Channel { tx, rx } = runner_slot else { unreachable!() };
        handle.reattach(Side::Runner, tx, rx).expect("the match is still running");
        // Without the Corp acting at all, the Runner has its board back.
        expect_state_update(&mut runner_rx).await;

        run.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn a_needed_seat_detached_past_the_grace_period_forfeits() {
        let (state, registry) = state(17);
        let (corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let (_runner_tx, mut runner_rx, runner_slot) = channel_slot();
        let session = MatchSession::new(state, registry, corp_slot, runner_slot).with_reconnect_grace(Duration::from_secs(30));
        let handle = session.reattach_handle();
        let run = tokio::spawn(session.run());

        expect_state_update(&mut corp_rx).await;
        expect_state_update(&mut runner_rx).await;
        drop(corp_tx);
        drop(corp_rx);

        // Paused time auto-advances to the deadline once every task is
        // idle, so this waits exactly the grace period in zero wall time.
        match runner_rx.recv().await {
            Some(ServerMessage::GameEnded { winner, reason }) => {
                assert_eq!(winner, Side::Runner);
                assert_eq!(reason, GameEndReason::Disconnected);
            }
            other => panic!("expected the Runner to be awarded the game, got {other:?}"),
        }
        let final_state = run.await.unwrap();
        assert!(!matches!(final_state.phase, GamePhase::GameOver(_)), "a forfeit is not a rules outcome");
        let (tx, rx) = (mpsc::unbounded_channel().0, mpsc::unbounded_channel().1);
        assert_eq!(handle.reattach(Side::Corp, tx, rx), Err(MatchOver));
        assert!(!handle.is_live());
    }

    /// The clock is reset by a reattach, not merely paused: a client that
    /// keeps coming back is present, and a seat that reattached 29 seconds
    /// in and dropped again is not forfeited one second later.
    #[tokio::test(start_paused = true)]
    async fn a_reattach_restarts_the_grace_clock() {
        let (state, registry) = state(19);
        let (corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let (_runner_tx, mut runner_rx, runner_slot) = channel_slot();
        let session = MatchSession::new(state, registry, corp_slot, runner_slot).with_reconnect_grace(Duration::from_secs(30));
        let handle = session.reattach_handle();
        let run = tokio::spawn(session.run());

        expect_state_update(&mut corp_rx).await;
        expect_state_update(&mut runner_rx).await;
        drop(corp_tx);
        drop(corp_rx);
        tokio::time::sleep(Duration::from_secs(29)).await;

        let (corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let PlayerSlot::Channel { tx, rx } = corp_slot else { unreachable!() };
        handle.reattach(Side::Corp, tx, rx).unwrap();
        expect_state_update(&mut corp_rx).await;
        drop(corp_tx);
        drop(corp_rx);
        tokio::time::sleep(Duration::from_secs(20)).await;

        // 49 seconds after the first drop, 20 after the second: alive.
        let (corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let PlayerSlot::Channel { tx, rx } = corp_slot else { unreachable!() };
        handle.reattach(Side::Corp, tx, rx).expect("the second drop's clock had 10 seconds left");
        expect_state_update(&mut corp_rx).await;
        assert!(runner_rx.try_recv().is_err(), "no GameEnded was sent");
        corp_tx.send(ClientMessage::SubmitAction(PlayerAction::KeepHand)).unwrap();
        expect_state_update(&mut corp_rx).await;

        run.abort();
    }

    // ----- the per-decision clock (`TurnTimeout`) -----

    fn expect_clock(message: Option<ServerMessage>) -> (Side, Duration) {
        match message {
            Some(ServerMessage::DecisionClock { side, remaining }) => (side, remaining),
            other => panic!("expected a DecisionClock, got {other:?}"),
        }
    }

    fn expect_game_ended(message: Option<ServerMessage>) -> (Side, GameEndReason) {
        match message {
            Some(ServerMessage::GameEnded { winner, reason }) => (winner, reason),
            other => panic!("expected GameEnded, got {other:?}"),
        }
    }

    /// Both seats attached and both told the Corp is on the clock; the
    /// Corp says nothing, and the Runner is awarded the game.
    #[tokio::test(start_paused = true)]
    async fn a_connected_seat_that_never_moves_forfeits_at_the_turn_timeout() {
        let (state, registry) = state(23);
        let (_corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let (_runner_tx, mut runner_rx, runner_slot) = channel_slot();
        let session = MatchSession::new(state, registry, corp_slot, runner_slot).with_turn_timeout(Some(Duration::from_secs(20)));
        let handle = session.reattach_handle();
        let run = tokio::spawn(session.run());

        expect_state_update(&mut corp_rx).await;
        expect_state_update(&mut runner_rx).await;
        assert_eq!(expect_clock(corp_rx.recv().await), (Side::Corp, Duration::from_secs(20)));
        assert_eq!(expect_clock(runner_rx.recv().await), (Side::Corp, Duration::from_secs(20)));

        assert_eq!(expect_game_ended(runner_rx.recv().await), (Side::Runner, GameEndReason::TimedOut));
        let final_state = run.await.unwrap();
        assert!(!matches!(final_state.phase, GamePhase::GameOver(_)), "a timeout is not a rules outcome");
        assert!(!handle.is_live());
    }

    /// Dropping the socket buys no time: the seat comes back ten seconds
    /// before the clock runs out, is told exactly that, and still forfeits
    /// — by the clock, not by the (longer) reconnect grace.
    #[tokio::test(start_paused = true)]
    async fn the_turn_clock_is_not_reset_by_a_reattach() {
        let (state, registry) = state(29);
        let (corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let (_runner_tx, mut runner_rx, runner_slot) = channel_slot();
        let session = MatchSession::new(state, registry, corp_slot, runner_slot)
            .with_reconnect_grace(Duration::from_secs(60))
            .with_turn_timeout(Some(Duration::from_secs(30)));
        let handle = session.reattach_handle();
        let run = tokio::spawn(session.run());

        expect_state_update(&mut corp_rx).await;
        expect_state_update(&mut runner_rx).await;
        expect_clock(corp_rx.recv().await);
        expect_clock(runner_rx.recv().await);

        tokio::time::sleep(Duration::from_secs(10)).await;
        drop(corp_tx);
        drop(corp_rx);
        tokio::time::sleep(Duration::from_secs(10)).await;

        let (_corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let PlayerSlot::Channel { tx, rx } = corp_slot else { unreachable!() };
        handle.reattach(Side::Corp, tx, rx).unwrap();
        expect_state_update(&mut corp_rx).await;
        assert_eq!(expect_clock(corp_rx.recv().await), (Side::Corp, Duration::from_secs(10)), "what is left, not a fresh clock");

        assert_eq!(expect_game_ended(runner_rx.recv().await), (Side::Runner, GameEndReason::TimedOut));
        run.await.unwrap();
    }

    /// The clock is per decision: an applied action ends it, and the next
    /// decision — the Runner's mulligan — starts its own.
    #[tokio::test(start_paused = true)]
    async fn a_submitted_action_starts_a_fresh_clock() {
        let (state, registry) = state(31);
        let (corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let (_runner_tx, mut runner_rx, runner_slot) = channel_slot();
        let session = MatchSession::new(state, registry, corp_slot, runner_slot).with_turn_timeout(Some(Duration::from_secs(30)));
        let run = tokio::spawn(session.run());

        expect_state_update(&mut corp_rx).await;
        expect_state_update(&mut runner_rx).await;
        expect_clock(corp_rx.recv().await);
        expect_clock(runner_rx.recv().await);

        tokio::time::sleep(Duration::from_secs(20)).await;
        corp_tx.send(ClientMessage::SubmitAction(PlayerAction::KeepHand)).unwrap();
        expect_state_update(&mut corp_rx).await;
        assert!(matches!(corp_rx.recv().await, Some(ServerMessage::ActionLog(_))));
        expect_state_update(&mut runner_rx).await;
        assert!(matches!(runner_rx.recv().await, Some(ServerMessage::ActionLog(_))));
        assert_eq!(expect_clock(corp_rx.recv().await), (Side::Runner, Duration::from_secs(30)));
        assert_eq!(expect_clock(runner_rx.recv().await), (Side::Runner, Duration::from_secs(30)));

        // 40 seconds after the Corp's clock started, 20 into the Runner's.
        tokio::time::sleep(Duration::from_secs(20)).await;
        assert!(runner_rx.try_recv().is_err(), "no GameEnded: the Runner has ten seconds left");
        assert!(corp_rx.try_recv().is_err());

        run.abort();
    }

    /// The design decision on `TurnTimeout`, pinned: an illegal submission
    /// is answered with `ActionRejected` and nothing else — no new clock,
    /// and the old one still fires on time.
    #[tokio::test(start_paused = true)]
    async fn a_rejected_action_does_not_restart_the_clock() {
        let started = Instant::now();
        let (state, registry) = state(37);
        let (corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let (_runner_tx, mut runner_rx, runner_slot) = channel_slot();
        let session = MatchSession::new(state, registry, corp_slot, runner_slot).with_turn_timeout(Some(Duration::from_secs(30)));
        let run = tokio::spawn(session.run());

        expect_state_update(&mut corp_rx).await;
        expect_state_update(&mut runner_rx).await;
        expect_clock(corp_rx.recv().await);
        expect_clock(runner_rx.recv().await);

        tokio::time::sleep(Duration::from_secs(20)).await;
        corp_tx.send(ClientMessage::SubmitAction(PlayerAction::EndTurn)).unwrap();
        assert!(matches!(corp_rx.recv().await, Some(ServerMessage::ActionRejected { .. })));
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(corp_rx.try_recv().is_err(), "no second DecisionClock");

        assert_eq!(expect_game_ended(runner_rx.recv().await), (Side::Runner, GameEndReason::TimedOut));
        assert!(started.elapsed() >= Duration::from_secs(30), "the original clock ran its full length");
        run.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn the_reconnect_grace_still_wins_when_it_is_shorter() {
        let (state, registry) = state(41);
        let (corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let (_runner_tx, mut runner_rx, runner_slot) = channel_slot();
        let session = MatchSession::new(state, registry, corp_slot, runner_slot)
            .with_reconnect_grace(Duration::from_secs(5))
            .with_turn_timeout(Some(Duration::from_secs(30)));
        let run = tokio::spawn(session.run());

        expect_state_update(&mut corp_rx).await;
        expect_state_update(&mut runner_rx).await;
        expect_clock(runner_rx.recv().await);
        drop(corp_tx);
        drop(corp_rx);

        assert_eq!(expect_game_ended(runner_rx.recv().await), (Side::Runner, GameEndReason::Disconnected));
        run.await.unwrap();
    }

    /// Off by default, and silent when off: a clock-less match's message
    /// sequence is exactly what it was before the clock existed.
    #[tokio::test]
    async fn no_clock_message_is_sent_when_the_timeout_is_off() {
        let (state, registry) = state(43);
        let (_corp_tx, mut corp_rx, corp_slot) = channel_slot();
        let (_runner_tx, mut runner_rx, runner_slot) = channel_slot();
        let session = MatchSession::new(state, registry, corp_slot, runner_slot);
        let run = tokio::spawn(session.run());

        expect_state_update(&mut corp_rx).await;
        expect_state_update(&mut runner_rx).await;
        tokio::task::yield_now().await;
        assert!(corp_rx.try_recv().is_err());
        assert!(runner_rx.try_recv().is_err());

        run.abort();
    }
}
