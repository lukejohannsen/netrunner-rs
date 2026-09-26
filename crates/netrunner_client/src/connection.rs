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
//! **How a game is asked for.** Every connection is attached (Phase 4 §7):
//! the machine attaches with the player's name, joins the lobby it was
//! given, and looks for a game in the chair it was given with that chair's
//! deck or decks — three messages, each sent when the one before it is
//! answered (`Seat`).
//!
//! **Who is asking.** A seat with `credentials` proves its key before
//! anything else on every transport it opens, a reconnect included:
//! `Identify`, then the server's `Challenge` is signed, then the hello.
//! A server that says it keeps its key (`lasting`) is held to the one
//! remembered for its address (`with_pinned`), and one met for the first
//! time is reported (`Event::ServerKey`) for the driver to remember. A
//! key that differs ends the connection before this player's key is
//! proved to it. A seat with no credentials skips all of it and plays
//! unrated; a spectator never identifies.
//!
//! **What reconnects.** A seat is taken back with its token
//! (`ClientMessage::Resume`). A place in a lobby's queue is not a seat —
//! the server withdraws it with its socket — so a connection dropped while
//! waiting attaches and looks again from the start. A spectator watches
//! again (`Spectate`), which is the whole of reconnecting for a place with
//! nothing to hold. A first connection that never got an answer is not
//! retried: there is nothing to take back, and the player is looking at
//! the address they typed. A match that has ended is not resumed either —
//! the server has let the seat go.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use netrunner_core::rules::Viewer;
use netrunner_identity::PublicKey;
use netrunner_protocol::statements::{deck_hash, SeatStatement, SEAT_TAG};
use netrunner_protocol::{Chair, ClientMessage, ServerMessage};
use uuid::Uuid;

/// What the connection is for.
#[derive(Debug, Clone)]
pub enum Goal {
    /// A game, looked for as this seat asks.
    Play(Seat),
    /// A running match to watch.
    Watch { match_id: Uuid },
}

/// A game to look for: who is asking, in which lobby — a format's
/// (`netrunner_protocol::format_lobby_id`) or a player's, with its
/// password if it has one — and in which chair, with that chair's deck or
/// decks. `credentials` is the key the player proves, and where servers'
/// keys are remembered; `None` plays unrated.
#[derive(Debug, Clone)]
pub struct Seat {
    pub player_name: String,
    pub lobby: String,
    pub password: Option<String>,
    pub chair: Chair,
    /// Boxed: a signing key is a few hundred bytes, and a seat without
    /// one should not carry the room for it.
    pub credentials: Option<Box<crate::identity::Credentials>>,
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
    /// The server at this address proved a key other than the one
    /// remembered for it. It may be a different server answering at the
    /// address, so this player's key was not proved to it.
    ServerKeyChanged { remembered: PublicKey, found: PublicKey },
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionError::Transport(error) => write!(f, "connecting to the server: {error}"),
            ConnectionError::ClosedBeforeSeat => write!(f, "server closed the connection before assigning a seat"),
            ConnectionError::Rejected(reason) => write!(f, "the server refused: {reason}"),
            ConnectionError::GaveUp(wait) => write!(f, "could not reconnect within {}s", wait.as_secs()),
            ConnectionError::ServerKeyChanged { remembered, found } => write!(
                f,
                "the server at this address has a new key ({}, not the {} remembered), so it may not be the same server. \
                 If its operator says the key changed, remove the address from {} and connect again",
                found.fingerprint(),
                remembered.fingerprint(),
                crate::identity::KNOWN_SERVERS_FILE
            ),
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
    /// A server that keeps its key, met for the first time: remember it
    /// for this address.
    ServerKey(PublicKey),
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
    /// The seat's credential, from `Queued` or `MatchJoined`; presented
    /// with `Resume` only once seated.
    token: Option<Uuid>,
    /// Given a place once, so the next is a resume.
    joined: bool,
    /// The key the server at this address must prove, once one is
    /// remembered or met.
    pinned: Option<PublicKey>,
    /// The key the server challenged this connection with, and the match
    /// and side it seated this player in: what a seat statement must name
    /// before it is signed.
    server_key: Option<PublicKey>,
    seated_at: Option<(Uuid, netrunner_core::rules::Side)>,
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
            pinned: None,
            server_key: None,
            seated_at: None,
            ended: false,
            retry: None,
            dial_due: true,
            outbox: VecDeque::new(),
            events: VecDeque::new(),
        }
    }

    /// The key the server at this address was last seen to have, which it
    /// must prove again if it says it keeps its key.
    pub fn with_pinned(mut self, key: Option<PublicKey>) -> Self {
        self.pinned = key;
        self
    }

    // --- inputs ---------------------------------------------------------

    /// The transport `poll_dial` asked for is open.
    pub fn on_open(&mut self, now: Instant) {
        if !matches!(self.phase, Phase::Dialing { .. }) {
            return;
        }
        let first = match self.credentials() {
            Some(credentials) => ClientMessage::Identify { key: credentials.identity.public_key() },
            None => self.hello(),
        };
        self.outbox.push_back(first);
        self.phase = Phase::Greeting { since: now };
    }

    /// What a seat proves itself with, if it proves anything.
    fn credentials(&self) -> Option<&crate::identity::Credentials> {
        match &self.goal {
            Goal::Play(seat) => seat.credentials.as_deref(),
            Goal::Watch { .. } => None,
        }
    }

    /// The message that asks for a place: after the key is proved, or
    /// first when there is none to prove.
    fn hello(&self) -> ClientMessage {
        match (&self.goal, self.token) {
            (Goal::Watch { match_id }, _) => ClientMessage::Spectate { match_id: *match_id },
            (Goal::Play(_), Some(session_token)) if self.joined => ClientMessage::Resume { session_token },
            (Goal::Play(seat), _) => ClientMessage::Attach { player_name: seat.player_name.clone() },
        }
    }

    /// A message from the server. Takes the time like every input, though
    /// no answer today depends on it.
    pub fn on_message(&mut self, message: ServerMessage, _now: Instant) {
        match (self.phase, message) {
            (Phase::Done, _) => {}
            // Sign the server's nonce — unless it is not the server this
            // address was remembered with, which is told before anything
            // is proved to it.
            (Phase::Greeting { .. }, ServerMessage::Challenge { nonce, server_key, lasting }) => {
                let Some(identity) = self.credentials().map(|credentials| credentials.identity.clone()) else { return };
                if lasting {
                    match self.pinned {
                        Some(remembered) if remembered != server_key => {
                            return self.fail(ConnectionError::ServerKeyChanged { remembered, found: server_key });
                        }
                        Some(_) => {}
                        None => {
                            self.pinned = Some(server_key);
                            self.events.push_back(Event::ServerKey(server_key));
                        }
                    }
                }
                self.server_key = Some(server_key);
                self.outbox.push_back(ClientMessage::Prove { signature: identity.prove(&server_key, &nonce) });
            }
            (Phase::Greeting { .. }, ServerMessage::Identified { .. }) => {
                let hello = self.hello();
                self.outbox.push_back(hello);
            }
            // Attached: into the lobby. In it: look for the game.
            (Phase::Greeting { .. }, ServerMessage::Attached { .. }) => {
                if let Goal::Play(seat) = &self.goal {
                    self.outbox.push_back(ClientMessage::JoinLobby { lobby: seat.lobby.clone(), password: seat.password.clone() });
                }
            }
            (Phase::Greeting { .. }, ServerMessage::LobbyJoined { .. }) => {
                if let Goal::Play(seat) = &self.goal {
                    self.outbox.push_back(ClientMessage::Seek { chair: seat.chair.clone() });
                }
            }
            (Phase::Greeting { .. } | Phase::Queued, ServerMessage::Queued { session_token, position }) => {
                self.token = Some(session_token);
                self.phase = Phase::Queued;
                self.back_up();
                self.events.push_back(Event::Queued(position));
            }
            (Phase::Greeting { .. } | Phase::Queued, message @ ServerMessage::MatchJoined { .. }) => {
                let ServerMessage::MatchJoined { match_id, assigned_side, session_token, ref corp_deck, ref runner_deck } = message else { unreachable!("matched above") };
                self.token = Some(session_token);
                self.seated_at = Some((match_id, assigned_side));
                let decks = (corp_deck.clone(), runner_deck.clone());
                self.seated(Viewer::Player(assigned_side), Some(session_token), decks, message);
            }
            (Phase::Greeting { .. }, message @ ServerMessage::Spectating { .. }) => {
                self.seated(Viewer::Spectator, None, (String::new(), String::new()), message);
            }
            (
                Phase::Greeting { .. } | Phase::Queued,
                ServerMessage::ConnectRejected { reason }
                | ServerMessage::ResumeRejected { reason }
                | ServerMessage::LobbyRefused { reason }
                | ServerMessage::SeekRefused { reason }
                | ServerMessage::IdentifyRefused { reason },
            ) => {
                self.fail(ConnectionError::Rejected(reason));
            }
            // Signed here, never shown: the player has nothing to decide.
            (Phase::Joined, ServerMessage::SignSeat { statement, salt }) => {
                if let Some(signature) = self.sign_seat(&statement, &salt) {
                    self.outbox.push_back(ClientMessage::SeatSigned { signature });
                }
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

    /// This seat's signature over `statement`, if the statement names what
    /// this connection knows to be true: this player's key, the server
    /// that challenged it, the match and side it was seated in, and the
    /// deck it brought for that side, hashed with `salt`. A statement
    /// that names anything else is not signed — the game goes on, and the
    /// receipt simply lacks this seat's word.
    fn sign_seat(&self, statement: &str, salt: &str) -> Option<netrunner_identity::Signature> {
        let Goal::Play(seat) = &self.goal else { return None };
        let credentials = seat.credentials.as_deref()?;
        let said: SeatStatement = serde_json::from_str(statement).ok()?;
        let (match_id, side) = self.seated_at?;
        let deck = match (&seat.chair, side) {
            (Chair::Corp(deck), netrunner_core::rules::Side::Corp) | (Chair::Runner(deck), netrunner_core::rules::Side::Runner) => deck,
            (Chair::Random { corp, .. }, netrunner_core::rules::Side::Corp) => corp,
            (Chair::Random { runner, .. }, netrunner_core::rules::Side::Runner) => runner,
            _ => return None,
        };
        let true_to_this_seat = said.key == credentials.identity.public_key()
            && Some(said.server_key) == self.server_key
            && said.match_id == match_id
            && said.side == side
            && said.deck_hash == deck_hash(salt, &deck.to_deck());
        true_to_this_seat.then(|| credentials.identity.sign(SEAT_TAG, statement.to_string()).signature)
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

    fn hello() -> Seat {
        let deck = Box::new(netrunner_core::decks::by_id("brick_stack").expect("a built-in deck"));
        Seat { player_name: "tester".into(), lobby: "startup".into(), password: None, chair: Chair::Corp(deck), credentials: None }
    }

    fn lobby() -> netrunner_protocol::LobbyInfo {
        netrunner_protocol::LobbyInfo { id: "startup".into(), name: "Startup".into(), format: netrunner_core::format::NsgFormat::Startup, permanent: true, closed: false, password: false, players: 1, seeking: 0 }
    }

    /// Opened and greeted: attached, in the lobby, and the seek sent.
    fn greet(conn: &mut Connection, t0: Instant) {
        conn.on_open(t0);
        assert!(matches!(sent(conn)[..], [ClientMessage::Attach { .. }]));
        conn.on_message(ServerMessage::Attached { lobbies: vec![lobby()] }, t0);
        assert!(matches!(&sent(conn)[..], [ClientMessage::JoinLobby { lobby, password: None }] if lobby == "startup"));
        conn.on_message(ServerMessage::LobbyJoined { lobby: lobby() }, t0);
        assert!(matches!(sent(conn)[..], [ClientMessage::Seek { chair: Chair::Corp(_) }]));
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
        greet(&mut conn, t0);
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
        greet(&mut conn, t0);
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

    /// A queue place is not a seat: the server withdraws it with the
    /// socket, so a drop while waiting attaches and looks again.
    #[test]
    fn a_drop_while_waiting_looks_again_from_the_start() {
        let t0 = Instant::now();
        let token = Uuid::new_v4();
        let mut conn = Connection::new(Goal::Play(hello()), t0);
        conn.poll_dial();
        greet(&mut conn, t0);
        conn.on_message(ServerMessage::Queued { session_token: token, position: 1 }, t0);
        assert!(matches!(events(&mut conn)[..], [Event::Queued(1)]));
        conn.on_closed(Closed::Dropped, t0);
        conn.on_timeout(t0);
        conn.poll_dial();
        greet(&mut conn, t0);
        let fresh = Uuid::new_v4();
        conn.on_message(ServerMessage::Queued { session_token: fresh, position: 1 }, t0);
        conn.on_message(joined(fresh), t0);
        let events = events(&mut conn);
        assert!(matches!(events[..], [Event::Link(Link::Reconnecting { .. }), Event::Link(Link::Reconnecting { .. }), Event::Link(Link::Up), Event::Queued(1), Event::Joined { .. }]), "{events:?}");
    }

    /// A lobby or a seek refused ends the first connection with the reason.
    #[test]
    fn a_refused_seek_is_final() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Play(hello()), t0);
        conn.poll_dial();
        greet(&mut conn, t0);
        conn.on_message(ServerMessage::SeekRefused { reason: "not legal".into() }, t0);
        assert!(conn.is_done());
        assert!(matches!(events(&mut conn).last(), Some(Event::Link(Link::Down(ConnectionError::Rejected(reason)))) if reason == "not legal"));
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

    // --- proving a key ---------------------------------------------------

    use netrunner_identity::{Identity, Nonce};

    fn signed_in() -> Seat {
        let credentials = crate::identity::Credentials { identity: Identity::from_secret([1; 32]), known_servers: None };
        Seat { credentials: Some(Box::new(credentials)), ..hello() }
    }

    fn server() -> Identity {
        Identity::from_secret([2; 32])
    }

    fn challenge(lasting: bool) -> ServerMessage {
        ServerMessage::Challenge { nonce: Nonce([9; 32]), server_key: server().public_key(), lasting }
    }

    /// Identify, sign the challenge for this server, then the hello.
    #[test]
    fn a_seat_with_a_key_proves_it_before_it_attaches_and_learns_a_lasting_server() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Play(signed_in()), t0);
        conn.poll_dial();
        conn.on_open(t0);
        let me = Identity::from_secret([1; 32]).public_key();
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Identify { key }] if key == me));
        conn.on_message(challenge(true), t0);
        let [ClientMessage::Prove { signature }] = sent(&mut conn)[..] else { panic!("expected Prove") };
        assert_eq!(me.verify_proof(&server().public_key(), &Nonce([9; 32]), &signature), Ok(()), "signed for this server and this nonce");
        assert!(matches!(&events(&mut conn)[..], [Event::ServerKey(key)] if *key == server().public_key()), "met for the first time: remember it");
        conn.on_message(ServerMessage::Identified { key: me }, t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Attach { .. }]));
    }

    /// A server that makes a new key each run is neither remembered nor
    /// held to a key remembered for its address.
    #[test]
    fn a_passing_server_key_is_not_remembered_or_checked() {
        let t0 = Instant::now();
        let other = Identity::from_secret([3; 32]).public_key();
        let mut conn = Connection::new(Goal::Play(signed_in()), t0).with_pinned(Some(other));
        conn.poll_dial();
        conn.on_open(t0);
        sent(&mut conn);
        conn.on_message(challenge(false), t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Prove { .. }]));
        assert!(events(&mut conn).is_empty());
    }

    /// A lasting key other than the one remembered ends the connection,
    /// and nothing is proved to whoever answered.
    #[test]
    fn a_changed_server_key_is_refused_before_anything_is_proved() {
        let t0 = Instant::now();
        let remembered = Identity::from_secret([3; 32]).public_key();
        let mut conn = Connection::new(Goal::Play(signed_in()), t0).with_pinned(Some(remembered));
        conn.poll_dial();
        conn.on_open(t0);
        sent(&mut conn);
        conn.on_message(challenge(true), t0);
        assert!(sent(&mut conn).is_empty(), "no proof for a server that is not the one remembered");
        assert!(conn.is_done());
        let [Event::Link(Link::Down(ConnectionError::ServerKeyChanged { remembered: was, found }))] = &events(&mut conn)[..] else { panic!() };
        assert_eq!((*was, *found), (remembered, server().public_key()));
        let words = ConnectionError::ServerKeyChanged { remembered, found: server().public_key() }.to_string();
        assert!(words.contains(&remembered.fingerprint()) && words.contains(crate::identity::KNOWN_SERVERS_FILE), "{words}");
    }

    #[test]
    fn a_refused_proof_ends_the_connection() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Play(signed_in()), t0);
        conn.poll_dial();
        conn.on_open(t0);
        conn.on_message(challenge(true), t0);
        conn.on_message(ServerMessage::IdentifyRefused { reason: "no".into() }, t0);
        assert!(conn.is_done());
    }

    /// A reconnect proves the key again — it is a new socket — and then
    /// resumes, held to the key the first connection met.
    #[test]
    fn a_reconnect_proves_the_key_again_then_resumes() {
        let t0 = Instant::now();
        let token = Uuid::new_v4();
        let me = Identity::from_secret([1; 32]).public_key();
        let mut conn = Connection::new(Goal::Play(signed_in()), t0);
        conn.poll_dial();
        conn.on_open(t0);
        conn.on_message(challenge(true), t0);
        conn.on_message(ServerMessage::Identified { key: me }, t0);
        conn.on_message(ServerMessage::Attached { lobbies: vec![lobby()] }, t0);
        conn.on_message(ServerMessage::LobbyJoined { lobby: lobby() }, t0);
        conn.on_message(joined(token), t0);
        sent(&mut conn);
        events(&mut conn);

        conn.on_closed(Closed::Dropped, t0);
        conn.on_timeout(t0);
        conn.poll_dial();
        conn.on_open(t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Identify { .. }]));
        let impostor = ServerMessage::Challenge { nonce: Nonce([1; 32]), server_key: Identity::from_secret([4; 32]).public_key(), lasting: true };
        let mut again = Connection::new(Goal::Play(signed_in()), t0).with_pinned(conn.pinned);
        again.poll_dial();
        again.on_open(t0);
        again.on_message(impostor, t0);
        assert!(again.is_done(), "the key met on the first connection is held for the next");

        conn.on_message(challenge(true), t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Prove { .. }]));
        conn.on_message(ServerMessage::Identified { key: me }, t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Resume { session_token }] if session_token == token));
    }

    /// A spectator never identifies.
    #[test]
    fn a_spectator_proves_nothing() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Watch { match_id: Uuid::nil() }, t0);
        conn.poll_dial();
        conn.on_open(t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Spectate { .. }]));
    }

    /// Seated through the key handshake, as the Corp in match `nil`.
    fn seated_signed_in(t0: Instant) -> Connection {
        let me = Identity::from_secret([1; 32]).public_key();
        let mut conn = Connection::new(Goal::Play(signed_in()), t0);
        conn.poll_dial();
        conn.on_open(t0);
        conn.on_message(challenge(false), t0);
        conn.on_message(ServerMessage::Identified { key: me }, t0);
        conn.on_message(ServerMessage::Attached { lobbies: vec![lobby()] }, t0);
        conn.on_message(ServerMessage::LobbyJoined { lobby: lobby() }, t0);
        conn.on_message(joined(Uuid::new_v4()), t0);
        sent(&mut conn);
        events(&mut conn);
        conn
    }

    fn statement(deck: &netrunner_core::rules::Deck, side: Side) -> String {
        serde_json::to_string(&SeatStatement {
            match_id: Uuid::nil(),
            server_key: server().public_key(),
            side,
            key: Identity::from_secret([1; 32]).public_key(),
            opponent_key: None,
            deck_hash: deck_hash("salt", deck),
            started_at: 0,
        })
        .unwrap()
    }

    /// A statement true to this seat is signed, unasked and unshown.
    #[test]
    fn a_seat_statement_true_to_this_seat_is_signed() {
        let t0 = Instant::now();
        let mut conn = seated_signed_in(t0);
        let brick = netrunner_core::decks::by_id("brick_stack").unwrap().to_deck();
        let said = statement(&brick, Side::Corp);
        conn.on_message(ServerMessage::SignSeat { statement: said.clone(), salt: "salt".into() }, t0);
        let [ClientMessage::SeatSigned { signature }] = sent(&mut conn)[..] else { panic!("expected SeatSigned") };
        assert_eq!(Identity::from_secret([1; 32]).public_key().verify(SEAT_TAG, said.as_bytes(), &signature), Ok(()));
        assert!(events(&mut conn).is_empty(), "nothing for the screen");
    }

    /// Another deck, another side or another salt is not signed.
    #[test]
    fn a_seat_statement_that_names_anything_else_is_not_signed() {
        let t0 = Instant::now();
        let mut conn = seated_signed_in(t0);
        let brick = netrunner_core::decks::by_id("brick_stack").unwrap().to_deck();
        let other = netrunner_core::decks::by_id("stolen_goods").unwrap().to_deck();
        for (said, salt) in [(statement(&other, Side::Corp), "salt"), (statement(&brick, Side::Runner), "salt"), (statement(&brick, Side::Corp), "pepper")] {
            conn.on_message(ServerMessage::SignSeat { statement: said, salt: salt.into() }, t0);
            assert!(sent(&mut conn).is_empty());
        }
    }
}
