//! A client's connection to a server, as a state machine that does no I/O
//! (Phase 4 §6 item 2).
//!
//! **Sans-IO.** `Connection` is told what happened — a transport opened
//! (`on_open`), a message arrived (`on_message`), the transport went away
//! (`on_closed`), the clock passed the deadline it asked for
//! (`on_timeout`), the player sent something (`submit`) or left (`close`).
//! In return it says what to do: open a transport (`poll_dial`), send these
//! messages (`poll_transmit`), tell the screen this (`poll_event`), wake me
//! at this instant (`poll_timeout`). Time is an argument, never read. The
//! shape is Firezone's
//! ([sans-IO](https://www.firezone.dev/blog/sans-io)), and it buys three
//! things here:
//!
//! - **The rules of a connection are tested without a socket or a clock.**
//!   The lobby, a resume within the server's grace, a refusal, giving up
//!   after `MAX_WAIT`: each is a few calls with made-up instants, where the
//!   blocking `Reconnector` this replaces could only be tested against a
//!   real server at the speed of real time.
//! - **Both clients drive the same machine.** `remote` is the one tokio
//!   driver, and it never blocks the thread that reads its channels — which
//!   Bevy's main thread requires (AGENTS.md §5) and the old reconnect loop,
//!   `block_in_place` on the render tick, could not give it.
//! - **A second transport is a transport change.** The machine speaks
//!   typed `ClientMessage`/`ServerMessage`; a WebSocket or a peer-to-peer
//!   stream (Phase 4 §6 item 3) only frames them.
//!
//! **Messages, not bytes.** The machine takes and gives typed messages
//! rather than frames: every transport this project has or plans carries
//! whole messages, and JSON framing is the driver's one line. Bytes would
//! put `serde_json` into every test for nothing.
//!
//! **What reconnects.** A seat is taken back with its token
//! (`ClientMessage::Resume`) — including a place in the lobby, whose token
//! `Queued` already carries and which nothing used to resume. A spectator
//! watches again (`Spectate`), which is the whole of reconnecting for a
//! place with nothing to hold. A first connection that never got an answer
//! is not retried: there is nothing to take back, and the player is
//! looking at the address they typed. A match that has ended is not
//! resumed either — the server has let the seat go.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use netrunner_core::rules::Viewer;
use netrunner_protocol::{ClientMessage, ServerMessage};
use uuid::Uuid;

/// What the connection is for.
#[derive(Debug, Clone)]
pub enum Goal {
    /// A seat, asked for with this `ClientMessage::Connect`.
    Play(ClientMessage),
    /// A running match to watch.
    Watch { match_id: Uuid },
}

/// How long a dropped seat keeps trying. Longer than the server's default
/// grace (30 s) on purpose: if the server needed the seat while it was
/// gone, it forfeits at its own deadline and the next attempt is refused
/// with a reason that says so — better than giving up first and never
/// learning the outcome.
pub const MAX_WAIT: Duration = Duration::from_secs(60);
/// Spacing between attempts. A refused TCP connection fails at once, so
/// without it a server that is down would use every attempt in a moment.
pub const RETRY_EVERY: Duration = Duration::from_secs(1);
/// Bound on one reconnect attempt, dial and answer together, so a
/// half-open socket does not hold up the next one.
pub const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(3);
/// Bound on the first dial. The old client had none, and an address that
/// drops packets hung the menu's wait until the player gave up.
pub const FIRST_DIAL_TIMEOUT: Duration = Duration::from_secs(10);

/// Why a transport went away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Closed {
    /// The dial itself failed, with the transport's words.
    DialFailed(String),
    /// An open transport closed or errored.
    Dropped,
}

/// Why a connection ended for good.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionError {
    /// The first dial failed or timed out.
    Transport(String),
    /// The server closed the first connection before giving a place.
    ClosedBeforeSeat,
    /// The server refused: at its match limit, a resume it no longer holds
    /// (the match ended, perhaps forfeited by this seat's absence), a match
    /// no longer running. No retry changes the answer.
    Rejected(String),
    /// Reconnecting took longer than `MAX_WAIT`.
    GaveUp(Duration),
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionError::Transport(error) => write!(f, "connecting to the server: {error}"),
            ConnectionError::ClosedBeforeSeat => write!(f, "server closed the connection before assigning a seat"),
            ConnectionError::Rejected(reason) => write!(f, "the server refused: {reason}"),
            ConnectionError::GaveUp(wait) => write!(f, "could not reconnect within {}s", wait.as_secs()),
        }
    }
}

impl std::error::Error for ConnectionError {}

/// The link as a screen shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    Up,
    /// Down since `since`, on reconnect attempt `attempts`.
    Reconnecting { attempts: u32, since: Instant },
    /// Down for good.
    Down(ConnectionError),
}

impl Link {
    /// A one-line status for a header, `None` while the link is up.
    pub fn status_line(&self, now: Instant) -> Option<String> {
        match self {
            Link::Up => None,
            Link::Reconnecting { attempts, since } => Some(format!(
                "Connection lost — reconnecting (attempt {attempts}, {}s of {}s)",
                now.saturating_duration_since(*since).as_secs(),
                MAX_WAIT.as_secs()
            )),
            Link::Down(error) => Some(format!("Connection lost: {error}. Press q to quit.")),
        }
    }
}

/// What the connection has to tell the screen.
#[derive(Debug, Clone)]
pub enum Event {
    /// Waiting in the lobby at this position.
    Queued(usize),
    /// A place at a match. Sent once; a resume is a `Link` change.
    Joined { viewer: Viewer, session_token: Option<Uuid>, decks: (String, String) },
    /// A message about the match itself — a view, a log entry, a clock, a
    /// rejection, the end.
    Message(ServerMessage),
    Link(Link),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Wants a transport, asked for at `since`.
    Dialing { since: Instant },
    /// The hello is sent; waiting for the first answer.
    Greeting { since: Instant },
    /// In the lobby.
    Queued,
    /// At a match.
    Joined,
    /// Between reconnect attempts, until `next`.
    Waiting { next: Instant },
    Done,
}

/// The reconnect under way, if one is.
#[derive(Debug, Clone, Copy)]
struct Retry {
    lost_at: Instant,
    attempts: u32,
}

pub struct Connection {
    goal: Goal,
    phase: Phase,
    /// The seat's credential, from `Queued` or `MatchJoined`.
    token: Option<Uuid>,
    /// Given a place once, so the next is a resume.
    joined: bool,
    /// `GameEnded` has arrived: a drop after it is not resumed.
    ended: bool,
    retry: Option<Retry>,
    dial_due: bool,
    outbox: VecDeque<ClientMessage>,
    events: VecDeque<Event>,
}

impl Connection {
    /// A connection that will dial as soon as it is driven.
    pub fn new(goal: Goal, now: Instant) -> Self {
        Connection {
            goal,
            phase: Phase::Dialing { since: now },
            token: None,
            joined: false,
            ended: false,
            retry: None,
            dial_due: true,
            outbox: VecDeque::new(),
            events: VecDeque::new(),
        }
    }

    // --- inputs ---------------------------------------------------------

    /// The transport `poll_dial` asked for is open.
    pub fn on_open(&mut self, now: Instant) {
        if !matches!(self.phase, Phase::Dialing { .. }) {
            return;
        }
        let hello = match (&self.goal, self.token) {
            (Goal::Watch { match_id }, _) => ClientMessage::Spectate { match_id: *match_id },
            (Goal::Play(_), Some(session_token)) => ClientMessage::Resume { session_token },
            (Goal::Play(connect), None) => connect.clone(),
        };
        self.outbox.push_back(hello);
        self.phase = Phase::Greeting { since: now };
    }

    /// A message from the server. Takes the time like every input, though
    /// no answer today depends on it.
    pub fn on_message(&mut self, message: ServerMessage, _now: Instant) {
        match (self.phase, message) {
            (Phase::Done, _) => {}
            (Phase::Greeting { .. } | Phase::Queued, ServerMessage::Queued { session_token, position }) => {
                self.token = Some(session_token);
                self.phase = Phase::Queued;
                self.back_up();
                self.events.push_back(Event::Queued(position));
            }
            (Phase::Greeting { .. } | Phase::Queued, message @ ServerMessage::MatchJoined { .. }) => {
                let ServerMessage::MatchJoined { assigned_side, session_token, ref corp_deck, ref runner_deck, .. } = message else { unreachable!("matched above") };
                self.token = Some(session_token);
                let decks = (corp_deck.clone(), runner_deck.clone());
                self.seated(Viewer::Player(assigned_side), Some(session_token), decks, message);
            }
            (Phase::Greeting { .. }, message @ ServerMessage::Spectating { .. }) => {
                self.seated(Viewer::Spectator, None, (String::new(), String::new()), message);
            }
            (Phase::Greeting { .. } | Phase::Queued, ServerMessage::ConnectRejected { reason } | ServerMessage::ResumeRejected { reason }) => {
                self.fail(ConnectionError::Rejected(reason));
            }
            (Phase::Joined, message) => {
                if matches!(message, ServerMessage::GameEnded { .. }) {
                    self.ended = true;
                }
                self.events.push_back(Event::Message(message));
            }
            // A `MatchList` answering nothing, or a message ahead of the
            // place it belongs to: nothing to do with it.
            _ => {}
        }
    }

    /// The transport went away, or the dial `poll_dial` asked for failed.
    pub fn on_closed(&mut self, closed: Closed, now: Instant) {
        match self.phase {
            Phase::Done | Phase::Waiting { .. } => {}
            Phase::Dialing { .. } | Phase::Greeting { .. } if self.retry.is_none() && self.token.is_none() && !self.joined => {
                self.fail(match closed {
                    Closed::DialFailed(error) => ConnectionError::Transport(error),
                    Closed::Dropped => ConnectionError::ClosedBeforeSeat,
                });
            }
            Phase::Joined if self.ended => self.phase = Phase::Done,
            Phase::Dialing { .. } | Phase::Greeting { .. } => self.wait_to_retry(now),
            Phase::Queued | Phase::Joined => {
                let retry = Retry { lost_at: now, attempts: 0 };
                self.retry = Some(retry);
                self.events.push_back(Event::Link(Link::Reconnecting { attempts: 0, since: now }));
                self.phase = Phase::Waiting { next: now };
            }
        }
    }

    /// The clock has reached (or passed) `poll_timeout`'s instant.
    pub fn on_timeout(&mut self, now: Instant) {
        if let Some(retry) = self.retry
            && now.saturating_duration_since(retry.lost_at) > MAX_WAIT
            && self.phase != Phase::Done
        {
            self.fail(ConnectionError::GaveUp(MAX_WAIT));
            return;
        }
        match self.phase {
            Phase::Waiting { next } if now >= next => {
                let retry = self.retry.get_or_insert(Retry { lost_at: now, attempts: 0 });
                retry.attempts += 1;
                let (attempts, since) = (retry.attempts, retry.lost_at);
                self.events.push_back(Event::Link(Link::Reconnecting { attempts, since }));
                self.phase = Phase::Dialing { since: now };
                self.dial_due = true;
            }
            Phase::Dialing { since } | Phase::Greeting { since } if self.retry.is_some() && now >= since + ATTEMPT_TIMEOUT => {
                self.wait_to_retry(now);
            }
            Phase::Dialing { since } if self.retry.is_none() && now >= since + FIRST_DIAL_TIMEOUT => {
                self.fail(ConnectionError::Transport(format!("no answer within {}s", FIRST_DIAL_TIMEOUT.as_secs())));
            }
            _ => {}
        }
    }

    /// Something the player sends — an action, a surrender. Refused
    /// (`false`) while there is no place to send it from: an action chosen
    /// on a view from before a drop may be stale by the time the seat is
    /// back, and the fresh view arrives first anyway.
    pub fn submit(&mut self, message: ClientMessage) -> bool {
        if self.phase != Phase::Joined {
            return false;
        }
        self.outbox.push_back(message);
        true
    }

    /// The player has left. The driver closes the transport, which is how a
    /// player waiting in the lobby leaves it rather than being paired after
    /// they have gone.
    pub fn close(&mut self) {
        self.phase = Phase::Done;
        self.outbox.clear();
    }

    // --- outputs --------------------------------------------------------

    /// Whether to open a transport now. True once per attempt.
    pub fn poll_dial(&mut self) -> bool {
        std::mem::take(&mut self.dial_due)
    }

    pub fn poll_transmit(&mut self) -> Option<ClientMessage> {
        self.outbox.pop_front()
    }

    pub fn poll_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    /// When to call `on_timeout` next, if ever.
    pub fn poll_timeout(&self) -> Option<Instant> {
        let give_up = self.retry.map(|retry| retry.lost_at + MAX_WAIT + Duration::from_millis(1));
        let own = match self.phase {
            Phase::Waiting { next } => Some(next),
            Phase::Dialing { since } | Phase::Greeting { since } if self.retry.is_some() => Some(since + ATTEMPT_TIMEOUT),
            Phase::Dialing { since } => Some(since + FIRST_DIAL_TIMEOUT),
            _ => None,
        };
        [give_up, own].into_iter().flatten().min()
    }

    /// Whether a transport, open or opening, is still wanted. False
    /// between attempts and once done: the driver drops what it holds.
    pub fn wants_transport(&self) -> bool {
        !matches!(self.phase, Phase::Waiting { .. } | Phase::Done)
    }

    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    // --- transitions ----------------------------------------------------

    /// A place, first or taken back. **A place taken back is passed on
    /// as the message that gave it** (`MatchJoined`, `Spectating`), after
    /// `Link::Up`, so whoever reads the match's messages knows, in order,
    /// that the view which follows is a fresh one with no action behind
    /// it (`play::MatchHandle::start_remote`): the server does not replay
    /// what a seat missed (Phase 4 §2), and a view that arrives alone is
    /// otherwise indistinguishable from one whose log entry is on its way.
    /// The link's `watch` could not say it: it may have gone down and up
    /// again before a reader gets to a message that was sent before the
    /// drop.
    fn seated(&mut self, viewer: Viewer, session_token: Option<Uuid>, decks: (String, String), message: ServerMessage) {
        self.phase = Phase::Joined;
        if self.joined {
            self.back_up();
            self.events.push_back(Event::Message(message));
        } else {
            self.joined = true;
            self.retry = None;
            self.events.push_back(Event::Joined { viewer, session_token, decks });
        }
    }

    /// A reconnect has an answer: the link is up again.
    fn back_up(&mut self) {
        if self.retry.take().is_some() {
            self.events.push_back(Event::Link(Link::Up));
        }
    }

    fn wait_to_retry(&mut self, now: Instant) {
        self.phase = Phase::Waiting { next: now + RETRY_EVERY };
    }

    fn fail(&mut self, error: ConnectionError) {
        self.phase = Phase::Done;
        self.outbox.clear();
        self.events.push_back(Event::Link(Link::Down(error)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::Side;
    use netrunner_protocol::GameEndReason;

    fn hello() -> ClientMessage {
        ClientMessage::Connect { player_name: "tester".into(), preferred_side: Some(Side::Corp), room: None, deck: None, format: None }
    }

    fn joined(token: Uuid) -> ServerMessage {
        ServerMessage::MatchJoined {
            match_id: Uuid::nil(),
            assigned_side: Side::Corp,
            session_token: token,
            corp_deck: "brick_stack".into(),
            runner_deck: "stolen_goods".into(),
        }
    }

    fn events(conn: &mut Connection) -> Vec<Event> {
        std::iter::from_fn(|| conn.poll_event()).collect()
    }

    fn sent(conn: &mut Connection) -> Vec<ClientMessage> {
        std::iter::from_fn(|| conn.poll_transmit()).collect()
    }

    /// Seated from a fresh connection: the returned connection is at a
    /// match with `token`.
    fn seated(t0: Instant, token: Uuid) -> Connection {
        let mut conn = Connection::new(Goal::Play(hello()), t0);
        assert!(conn.poll_dial());
        conn.on_open(t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Connect { .. }]));
        conn.on_message(joined(token), t0);
        assert!(matches!(events(&mut conn)[..], [Event::Joined { .. }]));
        conn
    }

    #[test]
    fn a_first_connection_says_hello_and_is_seated() {
        let t0 = Instant::now();
        let token = Uuid::new_v4();
        let mut conn = Connection::new(Goal::Play(hello()), t0);
        assert!(conn.poll_dial(), "dials at once");
        assert!(!conn.poll_dial(), "once");
        assert!(!conn.submit(ClientMessage::Surrender), "nothing to send from before a seat");
        conn.on_open(t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Connect { .. }]));
        conn.on_message(joined(token), t0);
        let [Event::Joined { viewer, session_token, decks }] = &events(&mut conn)[..] else { panic!() };
        assert_eq!((*viewer, *session_token), (Viewer::Player(Side::Corp), Some(token)));
        assert_eq!(decks.0, "brick_stack");
        assert!(conn.submit(ClientMessage::Surrender));
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Surrender]));
        assert_eq!(conn.poll_timeout(), None, "a seated connection has nothing to wait for");
    }

    #[test]
    fn a_first_connection_that_fails_is_not_retried() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Play(hello()), t0);
        conn.poll_dial();
        conn.on_closed(Closed::DialFailed("refused".into()), t0);
        assert!(conn.is_done());
        assert!(matches!(&events(&mut conn)[..], [Event::Link(Link::Down(ConnectionError::Transport(e)))] if e == "refused"));

        let mut conn = Connection::new(Goal::Play(hello()), t0);
        conn.poll_dial();
        conn.on_timeout(t0 + FIRST_DIAL_TIMEOUT);
        assert!(conn.is_done(), "a dial into nothing is bounded");
    }

    #[test]
    fn a_dropped_seat_resumes_with_its_token_and_the_link_comes_back() {
        let t0 = Instant::now();
        let token = Uuid::new_v4();
        let mut conn = seated(t0, token);
        conn.on_closed(Closed::Dropped, t0);
        assert!(!conn.wants_transport());
        assert!(matches!(&events(&mut conn)[..], [Event::Link(Link::Reconnecting { attempts: 0, .. })]));
        assert_eq!(conn.poll_timeout(), Some(t0));
        conn.on_timeout(t0);
        assert!(conn.poll_dial());
        assert!(matches!(&events(&mut conn)[..], [Event::Link(Link::Reconnecting { attempts: 1, .. })]));
        conn.on_open(t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Resume { session_token }] if session_token == token));
        assert!(!conn.submit(ClientMessage::Surrender), "held until the seat is back");
        conn.on_message(joined(token), t0);
        assert!(
            matches!(&events(&mut conn)[..], [Event::Link(Link::Up), Event::Message(ServerMessage::MatchJoined { .. })]),
            "a resume is not a second Joined, and the place is passed on to mark the fresh view"
        );
        assert!(conn.submit(ClientMessage::Surrender));
    }

    #[test]
    fn a_server_that_is_down_is_tried_every_second_until_max_wait() {
        let t0 = Instant::now();
        let mut conn = seated(t0, Uuid::new_v4());
        conn.on_closed(Closed::Dropped, t0);
        let mut now = t0;
        let mut dials = 0;
        while !conn.is_done() {
            now = conn.poll_timeout().expect("always waiting on something");
            conn.on_timeout(now);
            if conn.poll_dial() {
                dials += 1;
                conn.on_closed(Closed::DialFailed("refused".into()), now);
            }
        }
        assert!(now > t0 + MAX_WAIT && now < t0 + MAX_WAIT + RETRY_EVERY * 2, "gave up at the deadline: {:?}", now - t0);
        assert!((59..=61).contains(&dials), "one attempt a second: {dials}");
        let last = events(&mut conn).pop();
        assert!(matches!(last, Some(Event::Link(Link::Down(ConnectionError::GaveUp(_))))));
    }

    #[test]
    fn a_half_open_attempt_is_abandoned_for_the_next() {
        let t0 = Instant::now();
        let mut conn = seated(t0, Uuid::new_v4());
        conn.on_closed(Closed::Dropped, t0);
        conn.on_timeout(t0);
        conn.poll_dial();
        conn.on_open(t0);
        sent(&mut conn);
        conn.on_timeout(t0 + ATTEMPT_TIMEOUT);
        assert!(!conn.wants_transport(), "the silent socket is let go");
        conn.on_timeout(t0 + ATTEMPT_TIMEOUT + RETRY_EVERY);
        assert!(conn.poll_dial(), "and a fresh one tried");
    }

    #[test]
    fn a_refused_resume_is_final() {
        let t0 = Instant::now();
        let mut conn = seated(t0, Uuid::new_v4());
        conn.on_closed(Closed::Dropped, t0);
        conn.on_timeout(t0);
        conn.poll_dial();
        conn.on_open(t0);
        conn.on_message(ServerMessage::ResumeRejected { reason: "the match is over".into() }, t0);
        assert!(conn.is_done());
        assert!(matches!(events(&mut conn).last(), Some(Event::Link(Link::Down(ConnectionError::Rejected(_))))));
    }

    #[test]
    fn a_place_in_the_lobby_is_resumed_too() {
        let t0 = Instant::now();
        let token = Uuid::new_v4();
        let mut conn = Connection::new(Goal::Play(hello()), t0);
        conn.poll_dial();
        conn.on_open(t0);
        sent(&mut conn);
        conn.on_message(ServerMessage::Queued { session_token: token, position: 1 }, t0);
        assert!(matches!(events(&mut conn)[..], [Event::Queued(1)]));
        conn.on_closed(Closed::Dropped, t0);
        conn.on_timeout(t0);
        conn.poll_dial();
        conn.on_open(t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Resume { session_token }] if session_token == token));
        conn.on_message(ServerMessage::Queued { session_token: token, position: 1 }, t0);
        conn.on_message(joined(token), t0);
        let events = events(&mut conn);
        assert!(matches!(events[..], [Event::Link(Link::Reconnecting { .. }), Event::Link(Link::Reconnecting { .. }), Event::Link(Link::Up), Event::Queued(1), Event::Joined { .. }]), "{events:?}");
    }

    #[test]
    fn a_spectator_watches_again() {
        let t0 = Instant::now();
        let match_id = Uuid::new_v4();
        let mut conn = Connection::new(Goal::Watch { match_id }, t0);
        conn.poll_dial();
        conn.on_open(t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Spectate { match_id: id }] if id == match_id));
        conn.on_message(ServerMessage::Spectating { match_id }, t0);
        assert!(matches!(events(&mut conn)[..], [Event::Joined { viewer: Viewer::Spectator, session_token: None, .. }]));
        conn.on_closed(Closed::Dropped, t0);
        conn.on_timeout(t0);
        conn.poll_dial();
        conn.on_open(t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Spectate { .. }]), "a spectator has no token: it asks again");
        conn.on_message(ServerMessage::Spectating { match_id }, t0);
        assert!(matches!(&events(&mut conn)[..], [.., Event::Link(Link::Up), Event::Message(ServerMessage::Spectating { .. })]));
    }

    #[test]
    fn a_drop_after_the_game_ended_is_not_resumed() {
        let t0 = Instant::now();
        let mut conn = seated(t0, Uuid::new_v4());
        conn.on_message(ServerMessage::GameEnded { winner: Side::Corp, reason: GameEndReason::AgendaThreshold }, t0);
        conn.on_closed(Closed::Dropped, t0);
        assert!(conn.is_done());
        assert!(!conn.poll_dial());
        assert!(matches!(events(&mut conn)[..], [Event::Message(ServerMessage::GameEnded { .. })]), "no reconnect notice over the result");
    }

    #[test]
    fn leaving_stops_everything() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Play(hello()), t0);
        conn.poll_dial();
        conn.on_open(t0);
        conn.close();
        assert!(conn.is_done() && !conn.wants_transport());
        assert!(sent(&mut conn).is_empty(), "nothing goes out after leaving");
    }

    #[test]
    fn the_status_line_counts_the_wait() {
        let t0 = Instant::now();
        assert_eq!(Link::Up.status_line(t0), None);
        let line = Link::Reconnecting { attempts: 3, since: t0 }.status_line(t0 + Duration::from_secs(4)).unwrap();
        assert_eq!(line, "Connection lost — reconnecting (attempt 3, 4s of 60s)");
    }
}
