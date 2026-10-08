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
//! **A player's connection is attached and stays** (Phase 4 §7 stage 4b).
//! The machine attaches with the player's name and nothing else; what the
//! player then does on it — list, make, join or leave a lobby, look for a
//! game in a chair with that chair's deck or decks, stop looking — is
//! `submit`ted as the protocol's own messages and answered as `Event`s. A
//! game is a stretch of the same connection: `MatchJoined` opens it
//! (`Event::Joined`), its messages pass through (`Event::Message`), and
//! `BackInLobby` closes it, after which the player looks for the next game
//! without reconnecting. Until this stage the machine was given one lobby
//! and one chair and ended with the game (`Goal::Play`), so a second game
//! was a second connection — and a lobby could not be browsed at all.
//!
//! **Who is asking.** A player with `credentials` proves their key before
//! anything else on every transport they open, a reconnect included:
//! `Identify`, then the server's `Challenge` is signed, then the hello.
//! A server that says it keeps its key (`lasting`) is held to the one
//! remembered for its address (`with_pinned`), and one met for the first
//! time is reported (`Event::ServerKey`) for the driver to remember. A
//! key that differs ends the connection before this player's key is
//! proved to it. A player with no credentials skips all of it and plays
//! unrated; a spectator never identifies.
//!
//! **What reconnects.** A seat at a live match is taken back with its
//! token (`ClientMessage::Resume`); the server puts a resumed seat back in
//! its lobby itself when the game ends. Everything else is put back by the
//! machine, because the server forgets it with the socket: a dropped
//! connection attaches again, rejoins the lobby it was in with the
//! password it was let in by, and — if it was looking for a game — looks
//! again with the same chair. A spectator watches again (`Spectate`),
//! which is the whole of reconnecting for a place with nothing to hold. A
//! first connection that never got an answer is not retried: there is
//! nothing to take back, and the player is looking at the address they
//! typed. **A refused resume ends the game, not the connection:** the
//! match is told (`ResumeRejected` reaches the game's messages, so its
//! screen says the game stopped), and the player is attached again, back
//! in their lobby, for the next one.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use netrunner_core::rules::{Side, Viewer};
use netrunner_identity::PublicKey;
use netrunner_protocol::statements::{deck_hash, SeatStatement, SEAT_TAG};
use netrunner_protocol::{Chair, ClientMessage, LobbyInfo, ServerMessage};
use uuid::Uuid;

/// What the connection is for.
#[derive(Debug, Clone)]
pub enum Goal {
    /// Attached as this player, for as long as the player stays: lobbies
    /// and games come and go on it.
    Attach(Who),
    /// A running match to watch.
    Watch { match_id: Uuid },
}

/// Who is attaching: the name shown to others, and the key proved to the
/// server — `None` plays unrated.
#[derive(Debug, Clone)]
pub struct Who {
    pub player_name: String,
    /// Boxed: a signing key is a few hundred bytes, and a player without
    /// one should not carry the room for it.
    pub credentials: Option<Box<crate::identity::Credentials>>,
}

/// How long a dropped connection keeps trying. Longer than the server's
/// default seat grace (30 s) on purpose: if the server needed the seat
/// while it was gone, it forfeits at its own deadline and the next attempt
/// is refused with a reason that says so — better than giving up first and
/// never learning the outcome.
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
    /// The server closed the first connection before answering it.
    ClosedBeforeSeat,
    /// The server refused: a spectator's match no longer running, a key it
    /// would not take. No retry changes the answer. A refused lobby or
    /// seek is not one of these — the connection stays, and the refusal
    /// is an `Event`.
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
            ConnectionError::ClosedBeforeSeat => write!(f, "server closed the connection before answering"),
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
    /// Attached, and the open lobbies: the first answer on a player's
    /// connection, and again each time a dropped connection has attached
    /// afresh (a resumed seat is a `Link` change instead).
    Attached(Vec<LobbyInfo>),
    /// The answer to `ListLobbies`.
    Lobbies(Vec<LobbyInfo>),
    /// In this lobby now, after `JoinLobby` or `CreateLobby` — or after a
    /// reattach put the connection back in the lobby it was in.
    LobbyJoined(LobbyInfo),
    /// In no lobby, after `LeaveLobby`.
    LobbyLeft,
    /// `JoinLobby` or `CreateLobby` refused, with the server's reason.
    LobbyRefused(String),
    /// Looking for a game, at this position in the queue.
    Queued(usize),
    /// `Seek` refused, with the server's reason.
    SeekRefused(String),
    /// No longer looking, after `CancelSeek`.
    SeekCancelled,
    /// The answer to `MyStanding`: whether this connection proved a key,
    /// and its standing on the server's book if it has one there.
    Standing { key: Option<PublicKey>, standing: Option<netrunner_protocol::Standing> },
    /// A server that keeps its key, met for the first time: remember it
    /// for this address.
    ServerKey(PublicKey),
    /// A place at a match. Sent once per game; a resume is a `Link`
    /// change.
    Joined { viewer: Viewer, session_token: Option<Uuid>, decks: (String, String) },
    /// A message about the match itself — a view, a log entry, a clock, a
    /// rejection, the end, the receipt.
    Message(ServerMessage),
    /// The game is over and the connection is back in its lobby, free to
    /// look for the next. `None` when the server no longer has the lobby,
    /// or when the game's end was learnt from a refused resume.
    BackInLobby(Option<LobbyInfo>),
    Link(Link),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Wants a transport, asked for at `since`.
    Dialing { since: Instant },
    /// The hello is sent; waiting for the first answer.
    Greeting { since: Instant },
    /// Attached — in a lobby or not — and not looking for a game.
    Attached,
    /// Looking for a game.
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
    /// Answered at least once — attached, seated or spectating — so a
    /// drop is reconnected rather than reported as a dead address.
    answered: bool,
    /// Seated at a match that has not ended: a drop is resumed.
    in_match: bool,
    /// `GameEnded` has arrived: the seat is let go, so a drop attaches
    /// afresh rather than resuming.
    ended: bool,
    /// The lobby this connection is in, by id, with the password it was
    /// let in by — what a reattach rejoins.
    lobby: Option<(String, Option<String>)>,
    /// A lobby asked for and not yet answered, with the password given:
    /// `LobbyJoined` names the lobby, and this is where its password is.
    asked: Option<Option<String>>,
    /// The chair a game is being looked for in, until it is found,
    /// refused or cancelled — what a reattach seeks again with.
    seeking: Option<Chair>,
    /// The chair the current game was found in: the deck the seat
    /// statement must name.
    chair: Option<Chair>,
    /// The key the server at this address must prove, once one is
    /// remembered or met.
    pinned: Option<PublicKey>,
    /// The key the server challenged this connection with, and the match
    /// and side it seated this player in: what a seat statement must name
    /// before it is signed.
    server_key: Option<PublicKey>,
    seated_at: Option<(Uuid, Side)>,
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
            answered: false,
            in_match: false,
            ended: false,
            lobby: None,
            asked: None,
            seeking: None,
            chair: None,
            pinned: None,
            server_key: None,
            seated_at: None,
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

    /// A lobby to be in, and a chair to look for a game in, from the first
    /// attach: put in place as a reattach puts back the lobby and the seek
    /// the socket lost, by the same rule — so one game looked for on a
    /// connection of its own (`remote::seek`) is the machine's ordinary
    /// behaviour and not a script a screen has to keep polling. A refusal
    /// of either comes back as the event it is.
    pub fn with_lobby_and_seek(mut self, lobby: Option<(String, Option<String>)>, chair: Option<Chair>) -> Self {
        self.lobby = lobby;
        self.seeking = chair;
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

    /// What a player proves themselves with, if they prove anything.
    fn credentials(&self) -> Option<&crate::identity::Credentials> {
        match &self.goal {
            Goal::Attach(who) => who.credentials.as_deref(),
            Goal::Watch { .. } => None,
        }
    }

    /// The message that asks for a place: after the key is proved, or
    /// first when there is none to prove. A seat at a live match is taken
    /// back; everything else attaches.
    fn hello(&self) -> ClientMessage {
        match (&self.goal, self.token) {
            (Goal::Watch { match_id }, _) => ClientMessage::Spectate { match_id: *match_id },
            (Goal::Attach(_), Some(session_token)) if self.in_match && !self.ended => ClientMessage::Resume { session_token },
            (Goal::Attach(who), _) => ClientMessage::Attach { player_name: who.player_name.clone() },
        }
    }

    /// A message from the server. Takes the time like every input; only
    /// a refused resume, which waits for the next dial, reads it.
    pub fn on_message(&mut self, message: ServerMessage, now: Instant) {
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
            // Attached: say so, and put back what the socket lost — the
            // lobby, then (once in it) the seek.
            (Phase::Greeting { .. }, ServerMessage::Attached { lobbies }) => {
                self.phase = Phase::Attached;
                self.answered = true;
                self.back_up();
                self.events.push_back(Event::Attached(lobbies));
                if let Some((lobby, password)) = self.lobby.clone() {
                    self.outbox.push_back(ClientMessage::JoinLobby { lobby, password });
                }
            }
            (Phase::Attached | Phase::Queued | Phase::Joined, ServerMessage::Lobbies { lobbies }) => {
                self.events.push_back(Event::Lobbies(lobbies));
            }
            (Phase::Attached | Phase::Queued | Phase::Joined, ServerMessage::Standing { key, standing }) => {
                self.events.push_back(Event::Standing { key, standing });
            }
            (Phase::Attached, ServerMessage::LobbyJoined { lobby }) => {
                // Asked for by the player, with a password; or put back by
                // a reattach, with the one it was let in by before.
                let password = match self.asked.take() {
                    Some(password) => password,
                    None => self.lobby.take().and_then(|(_, password)| password),
                };
                self.lobby = Some((lobby.id.clone(), password));
                self.events.push_back(Event::LobbyJoined(lobby));
                if let Some(chair) = self.seeking.clone() {
                    self.outbox.push_back(ClientMessage::Seek { chair });
                }
            }
            (Phase::Attached, ServerMessage::LobbyRefused { reason }) => {
                // A refusal nobody asked for is the reattach's: the lobby
                // has gone, and so has any seek that needed it.
                if self.asked.take().is_none() {
                    self.lobby = None;
                    self.seeking = None;
                }
                self.events.push_back(Event::LobbyRefused(reason));
            }
            (Phase::Attached, ServerMessage::LobbyLeft) => {
                self.lobby = None;
                self.events.push_back(Event::LobbyLeft);
            }
            (Phase::Attached | Phase::Queued, ServerMessage::Queued { session_token, position }) => {
                self.token = Some(session_token);
                self.phase = Phase::Queued;
                self.events.push_back(Event::Queued(position));
            }
            (Phase::Attached | Phase::Queued, ServerMessage::SeekRefused { reason }) => {
                self.seeking = None;
                self.phase = Phase::Attached;
                self.events.push_back(Event::SeekRefused(reason));
            }
            (Phase::Queued, ServerMessage::SeekCancelled) => {
                self.seeking = None;
                self.phase = Phase::Attached;
                self.events.push_back(Event::SeekCancelled);
            }
            (Phase::Greeting { .. } | Phase::Attached | Phase::Queued, message @ ServerMessage::MatchJoined { .. }) => {
                let ServerMessage::MatchJoined { match_id, assigned_side, session_token, ref corp_deck, ref runner_deck } = message else { unreachable!("matched above") };
                self.token = Some(session_token);
                self.seated_at = Some((match_id, assigned_side));
                if let Some(chair) = self.seeking.take() {
                    self.chair = Some(chair);
                }
                let decks = (corp_deck.clone(), runner_deck.clone());
                self.seated(Viewer::Player(assigned_side), Some(session_token), decks, message);
            }
            (Phase::Greeting { .. }, message @ ServerMessage::Spectating { .. }) => {
                self.seated(Viewer::Spectator, None, (String::new(), String::new()), message);
            }
            // The seat is gone — the match ended while this connection was
            // down, forfeited by its absence or finished without it. The
            // game is told, and the player stays: the server closes this
            // socket, and the next dial attaches afresh.
            (Phase::Greeting { .. }, ServerMessage::ResumeRejected { reason }) => {
                self.events.push_back(Event::Message(ServerMessage::ResumeRejected { reason }));
                self.leave_match();
                self.events.push_back(Event::BackInLobby(None));
                self.wait_to_retry(now);
            }
            (Phase::Greeting { .. }, ServerMessage::ConnectRejected { reason } | ServerMessage::IdentifyRefused { reason }) => {
                self.fail(ConnectionError::Rejected(reason));
            }
            // Signed here, never shown: the player has nothing to decide.
            (Phase::Joined, ServerMessage::SignSeat { statement, salt }) => {
                if let Some(signature) = self.sign_seat(&statement, &salt) {
                    self.outbox.push_back(ClientMessage::SeatSigned { signature });
                }
            }
            (Phase::Joined, ServerMessage::BackInLobby { lobby }) => {
                self.leave_match();
                self.lobby = match (lobby.as_ref(), self.lobby.take()) {
                    (Some(info), Some((_, password))) => Some((info.id.clone(), password)),
                    (Some(info), None) => Some((info.id.clone(), None)),
                    (None, _) => None,
                };
                self.events.push_back(Event::BackInLobby(lobby));
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
            Phase::Dialing { .. } | Phase::Greeting { .. } if self.retry.is_none() && !self.answered => {
                self.fail(match closed {
                    Closed::DialFailed(error) => ConnectionError::Transport(error),
                    Closed::Dropped => ConnectionError::ClosedBeforeSeat,
                });
            }
            // A spectator's match is over: nothing to watch again. A
            // player's seat is let go at the end, so the player attaches
            // afresh — the lobby would have been reached by `BackInLobby`
            // had the socket held.
            Phase::Joined if self.ended => match self.goal {
                Goal::Watch { .. } => self.phase = Phase::Done,
                Goal::Attach(_) => {
                    self.leave_match();
                    self.events.push_back(Event::BackInLobby(None));
                    self.begin_retry(now);
                }
            },
            Phase::Dialing { .. } | Phase::Greeting { .. } => self.wait_to_retry(now),
            Phase::Attached | Phase::Queued | Phase::Joined => self.begin_retry(now),
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

    /// Something the player sends. Refused (`false`) where it makes no
    /// sense, and the server's own refusals cover the rest: a game's
    /// message needs a seat — an action chosen on a view from before a
    /// drop may be stale by the time the seat is back, and the fresh view
    /// arrives first anyway; a lobby is joined, made or left only while
    /// attached and not looking, since the server would say "cancel
    /// first"; one seek at a time; a cancel only while looking. The
    /// handshake's messages are the machine's own.
    pub fn submit(&mut self, message: ClientMessage) -> bool {
        let allowed = match &message {
            ClientMessage::SubmitAction(_) | ClientMessage::Surrender | ClientMessage::TakeBack | ClientMessage::SeatSigned { .. } => self.phase == Phase::Joined,
            ClientMessage::JoinLobby { .. } | ClientMessage::CreateLobby { .. } | ClientMessage::LeaveLobby | ClientMessage::Seek { .. } => self.phase == Phase::Attached,
            ClientMessage::CancelSeek => self.phase == Phase::Queued,
            ClientMessage::ListLobbies | ClientMessage::ListMatches | ClientMessage::MyStanding => matches!(self.phase, Phase::Attached | Phase::Queued | Phase::Joined),
            ClientMessage::Attach { .. } | ClientMessage::Resume { .. } | ClientMessage::Spectate { .. } | ClientMessage::Identify { .. } | ClientMessage::Prove { .. } => false,
        };
        if !allowed {
            return false;
        }
        match &message {
            ClientMessage::JoinLobby { password, .. } | ClientMessage::CreateLobby { password, .. } => self.asked = Some(password.clone()),
            ClientMessage::Seek { chair } => self.seeking = Some(chair.clone()),
            _ => {}
        }
        self.outbox.push_back(message);
        true
    }

    /// The player has left. The driver closes the transport, which is how a
    /// player waiting in a lobby leaves it rather than being paired after
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
        let credentials = self.credentials()?;
        let said: SeatStatement = serde_json::from_str(statement).ok()?;
        let (match_id, side) = self.seated_at?;
        let deck = match (self.chair.as_ref()?, side) {
            (Chair::Corp(deck), Side::Corp) | (Chair::Runner(deck), Side::Runner) => deck,
            (Chair::Random { corp, .. }, Side::Corp) => corp,
            (Chair::Random { runner, .. }, Side::Runner) => runner,
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
        self.answered = true;
        self.back_up();
        if self.in_match {
            self.events.push_back(Event::Message(message));
        } else {
            self.in_match = true;
            self.ended = false;
            self.events.push_back(Event::Joined { viewer, session_token, decks });
        }
    }

    /// The game is over and the seat let go, however that was learnt.
    fn leave_match(&mut self) {
        self.phase = Phase::Attached;
        self.in_match = false;
        self.ended = false;
        self.token = None;
        self.seated_at = None;
        self.chair = None;
    }

    /// A reconnect has an answer: the link is up again.
    fn back_up(&mut self) {
        if self.retry.take().is_some() {
            self.events.push_back(Event::Link(Link::Up));
        }
    }

    /// The transport went while there was something to come back to.
    fn begin_retry(&mut self, now: Instant) {
        self.retry = Some(Retry { lost_at: now, attempts: 0 });
        self.events.push_back(Event::Link(Link::Reconnecting { attempts: 0, since: now }));
        self.phase = Phase::Waiting { next: now };
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
    use netrunner_core::format::NsgFormat;
    use netrunner_protocol::GameEndReason;

    fn who() -> Who {
        Who { player_name: "tester".into(), credentials: None }
    }

    fn corp_chair() -> Chair {
        Chair::Corp(Box::new(netrunner_core::decks::by_id("brick_stack").expect("a built-in deck")))
    }

    fn lobby() -> LobbyInfo {
        LobbyInfo { id: "startup".into(), name: "Startup".into(), format: NsgFormat::Startup, permanent: true, closed: false, password: false, rated: false, players: 1, seeking: 0 }
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

    /// Opened and attached, the lobbies reported.
    fn attach(conn: &mut Connection, t0: Instant) {
        conn.on_open(t0);
        assert!(matches!(sent(conn)[..], [ClientMessage::Attach { .. }]));
        conn.on_message(ServerMessage::Attached { lobbies: vec![lobby()] }, t0);
    }

    /// Attached, in the Startup lobby and looking for a game as the Corp
    /// — the player's three requests, each answered.
    fn seeking(conn: &mut Connection, t0: Instant) {
        attach(conn, t0);
        assert!(matches!(events(conn)[..], [Event::Attached(_)]));
        assert!(conn.submit(ClientMessage::JoinLobby { lobby: "startup".into(), password: None }));
        assert!(matches!(&sent(conn)[..], [ClientMessage::JoinLobby { lobby, password: None }] if lobby == "startup"));
        conn.on_message(ServerMessage::LobbyJoined { lobby: lobby() }, t0);
        assert!(matches!(events(conn)[..], [Event::LobbyJoined(_)]));
        assert!(sent(conn).is_empty(), "joining a lobby seeks nothing by itself");
        assert!(conn.submit(ClientMessage::Seek { chair: corp_chair() }));
        assert!(matches!(sent(conn)[..], [ClientMessage::Seek { chair: Chair::Corp(_) }]));
    }

    /// Seated from a fresh connection: the returned connection is at a
    /// match with `token`.
    fn seated(t0: Instant, token: Uuid) -> Connection {
        let mut conn = Connection::new(Goal::Attach(who()), t0);
        assert!(conn.poll_dial());
        seeking(&mut conn, t0);
        conn.on_message(joined(token), t0);
        assert!(matches!(events(&mut conn)[..], [Event::Joined { .. }]));
        conn
    }

    #[test]
    fn a_first_connection_attaches_joins_seeks_and_is_seated() {
        let t0 = Instant::now();
        let token = Uuid::new_v4();
        let mut conn = Connection::new(Goal::Attach(who()), t0);
        assert!(conn.poll_dial(), "dials at once");
        assert!(!conn.poll_dial(), "once");
        assert!(!conn.submit(ClientMessage::Surrender), "nothing to send from before a seat");
        assert!(!conn.submit(ClientMessage::JoinLobby { lobby: "startup".into(), password: None }), "nor a lobby before attaching");
        seeking(&mut conn, t0);
        conn.on_message(ServerMessage::Queued { session_token: token, position: 1 }, t0);
        assert!(matches!(events(&mut conn)[..], [Event::Queued(1)]));
        assert!(!conn.submit(ClientMessage::JoinLobby { lobby: "standard".into(), password: None }), "cancel the seek first");
        conn.on_message(joined(token), t0);
        let [Event::Joined { viewer, session_token, decks }] = &events(&mut conn)[..] else { panic!() };
        assert_eq!((*viewer, *session_token), (Viewer::Player(Side::Corp), Some(token)));
        assert_eq!(decks.0, "brick_stack");
        assert!(conn.submit(ClientMessage::Surrender));
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Surrender]));
        assert_eq!(conn.poll_timeout(), None, "a seated connection has nothing to wait for");
    }

    /// The game ends, the server says the connection is back in its lobby,
    /// and the next game is looked for on the same connection — in another
    /// chair, without a dial.
    #[test]
    fn the_next_game_is_looked_for_on_the_same_connection() {
        let t0 = Instant::now();
        let mut conn = seated(t0, Uuid::new_v4());
        conn.on_message(ServerMessage::GameEnded { winner: Side::Corp, reason: GameEndReason::AgendaThreshold }, t0);
        conn.on_message(ServerMessage::BackInLobby { lobby: Some(lobby()) }, t0);
        assert!(matches!(&events(&mut conn)[..], [Event::Message(ServerMessage::GameEnded { .. }), Event::BackInLobby(Some(_))]));
        assert!(!conn.poll_dial(), "no new connection");
        assert!(!conn.submit(ClientMessage::Surrender), "the seat is gone");
        let runner = Chair::Runner(Box::new(netrunner_core::decks::by_id("stolen_goods").unwrap()));
        assert!(conn.submit(ClientMessage::Seek { chair: runner }));
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Seek { chair: Chair::Runner(_) }]));
        let again = Uuid::new_v4();
        conn.on_message(ServerMessage::MatchJoined { match_id: Uuid::nil(), assigned_side: Side::Runner, session_token: again, corp_deck: String::new(), runner_deck: "stolen_goods".into() }, t0);
        assert!(matches!(&events(&mut conn)[..], [Event::Joined { viewer: Viewer::Player(Side::Runner), session_token: Some(token), .. }] if *token == again), "a second game is a second Joined");
    }

    /// The one-shot shape: a lobby and a chair given up front are asked
    /// for as the answers come, with nothing submitted, and a refusal of
    /// either is the event it would be if the player had asked.
    #[test]
    fn a_lobby_and_a_chair_given_up_front_are_asked_for_as_the_answers_come() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Attach(who()), t0).with_lobby_and_seek(Some(("startup".into(), None)), Some(corp_chair()));
        conn.poll_dial();
        attach(&mut conn, t0);
        assert!(matches!(&sent(&mut conn)[..], [ClientMessage::JoinLobby { lobby, password: None }] if lobby == "startup"));
        conn.on_message(ServerMessage::LobbyJoined { lobby: lobby() }, t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Seek { chair: Chair::Corp(_) }]));
        conn.on_message(ServerMessage::SeekRefused { reason: "not legal".into() }, t0);
        assert!(matches!(events(&mut conn)[..], [Event::Attached(_), Event::LobbyJoined(_), Event::SeekRefused(_)]));

        let mut conn = Connection::new(Goal::Attach(who()), t0).with_lobby_and_seek(Some(("nowhere".into(), None)), Some(corp_chair()));
        conn.poll_dial();
        attach(&mut conn, t0);
        sent(&mut conn);
        conn.on_message(ServerMessage::LobbyRefused { reason: "no lobby NOWHERE".into() }, t0);
        assert!(matches!(events(&mut conn).last(), Some(Event::LobbyRefused(_))));
        assert!(conn.lobby.is_none() && conn.seeking.is_none(), "nothing is tried again");
    }

    /// Lobbies are listed, made, joined and left as the player asks, and
    /// the answers are the events.
    #[test]
    fn lobbies_are_browsed_on_the_connection() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Attach(who()), t0);
        conn.poll_dial();
        attach(&mut conn, t0);
        events(&mut conn);
        assert!(conn.submit(ClientMessage::ListLobbies));
        conn.on_message(ServerMessage::Lobbies { lobbies: vec![lobby()] }, t0);
        assert!(conn.submit(ClientMessage::CreateLobby { name: "Friday".into(), format: NsgFormat::Startup, closed: true, password: Some("swordfish".into()), casual: false }));
        let made = LobbyInfo { id: "K7M2QX".into(), name: "Friday".into(), permanent: false, closed: true, password: true, ..lobby() };
        conn.on_message(ServerMessage::LobbyJoined { lobby: made.clone() }, t0);
        assert_eq!(conn.lobby, Some(("K7M2QX".to_string(), Some("swordfish".to_string()))), "the lobby and the password it was made with");
        assert!(conn.submit(ClientMessage::LeaveLobby));
        conn.on_message(ServerMessage::LobbyLeft, t0);
        assert_eq!(conn.lobby, None);
        assert!(conn.submit(ClientMessage::JoinLobby { lobby: "nowhere".into(), password: None }));
        conn.on_message(ServerMessage::LobbyRefused { reason: "no lobby NOWHERE".into() }, t0);
        // The standing is asked from the lobby too, and answered where
        // the connection stands (Phase 4 §7 stage 5).
        assert!(conn.submit(ClientMessage::MyStanding));
        conn.on_message(ServerMessage::Standing { key: None, standing: None }, t0);
        let events = events(&mut conn);
        assert!(
            matches!(&events[..], [Event::Lobbies(_), Event::LobbyJoined(info), Event::LobbyLeft, Event::LobbyRefused(reason), Event::Standing { key: None, standing: None }] if *info == made && reason == "no lobby NOWHERE"),
            "{events:?}"
        );
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::ListLobbies, ClientMessage::CreateLobby { .. }, ClientMessage::LeaveLobby, ClientMessage::JoinLobby { .. }, ClientMessage::MyStanding]));
    }

    /// Stopping looking is answered by the server, and until it is the
    /// connection is still looking: a `MatchJoined` that beat the cancel
    /// is a game.
    #[test]
    fn a_seek_is_cancelled_when_the_server_says_so() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Attach(who()), t0);
        conn.poll_dial();
        seeking(&mut conn, t0);
        assert!(!conn.submit(ClientMessage::CancelSeek), "nothing queued yet");
        conn.on_message(ServerMessage::Queued { session_token: Uuid::new_v4(), position: 1 }, t0);
        assert!(conn.submit(ClientMessage::CancelSeek));
        assert!(!conn.submit(ClientMessage::Seek { chair: corp_chair() }), "one seek at a time, until the cancel is answered");
        conn.on_message(ServerMessage::SeekCancelled, t0);
        assert!(matches!(events(&mut conn).last(), Some(Event::SeekCancelled)));
        assert!(conn.seeking.is_none() && conn.phase == Phase::Attached);
        assert!(conn.submit(ClientMessage::Seek { chair: corp_chair() }));
        conn.on_message(ServerMessage::SeekRefused { reason: "not legal".into() }, t0);
        assert!(matches!(events(&mut conn).last(), Some(Event::SeekRefused(reason)) if reason == "not legal"));
        assert!(conn.submit(ClientMessage::JoinLobby { lobby: "standard".into(), password: None }), "a refused seek leaves the player free");
    }

    #[test]
    fn a_first_connection_that_fails_is_not_retried() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Attach(who()), t0);
        conn.poll_dial();
        conn.on_closed(Closed::DialFailed("refused".into()), t0);
        assert!(conn.is_done());
        assert!(matches!(&events(&mut conn)[..], [Event::Link(Link::Down(ConnectionError::Transport(e)))] if e == "refused"));

        let mut conn = Connection::new(Goal::Attach(who()), t0);
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

    /// The seat is gone — forfeited by this connection's absence, or the
    /// match finished without it. The game is told it stopped, the player
    /// is back in no lobby, and the next dial attaches afresh and rejoins
    /// the lobby the game was found in.
    #[test]
    fn a_refused_resume_ends_the_game_not_the_connection() {
        let t0 = Instant::now();
        let mut conn = seated(t0, Uuid::new_v4());
        conn.on_closed(Closed::Dropped, t0);
        conn.on_timeout(t0);
        conn.poll_dial();
        conn.on_open(t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Resume { .. }]));
        events(&mut conn);
        conn.on_message(ServerMessage::ResumeRejected { reason: "the match ended".into() }, t0);
        assert!(!conn.is_done());
        assert!(matches!(&events(&mut conn)[..], [Event::Message(ServerMessage::ResumeRejected { .. }), Event::BackInLobby(None)]));
        assert!(!conn.wants_transport(), "the server closes the socket after a refusal; the next attempt is a fresh dial");
        conn.on_timeout(t0 + RETRY_EVERY);
        assert!(conn.poll_dial());
        attach(&mut conn, t0 + RETRY_EVERY);
        assert!(matches!(&sent(&mut conn)[..], [ClientMessage::JoinLobby { lobby, .. }] if lobby == "startup"), "back in the lobby the game was found in");
        assert!(matches!(&events(&mut conn)[..], [Event::Link(Link::Reconnecting { .. }), Event::Link(Link::Up), Event::Attached(_)]));
    }

    /// A queue place is not a seat: the server withdraws it with the
    /// socket, so a drop while waiting attaches, rejoins and looks again.
    #[test]
    fn a_drop_while_waiting_looks_again_from_the_start() {
        let t0 = Instant::now();
        let token = Uuid::new_v4();
        let mut conn = Connection::new(Goal::Attach(who()), t0);
        conn.poll_dial();
        seeking(&mut conn, t0);
        conn.on_message(ServerMessage::Queued { session_token: token, position: 1 }, t0);
        assert!(matches!(events(&mut conn)[..], [Event::Queued(1)]));
        conn.on_closed(Closed::Dropped, t0);
        conn.on_timeout(t0);
        conn.poll_dial();
        attach(&mut conn, t0);
        assert!(matches!(&sent(&mut conn)[..], [ClientMessage::JoinLobby { lobby, .. }] if lobby == "startup"));
        conn.on_message(ServerMessage::LobbyJoined { lobby: lobby() }, t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Seek { chair: Chair::Corp(_) }]), "the same chair, looked for again");
        let fresh = Uuid::new_v4();
        conn.on_message(ServerMessage::Queued { session_token: fresh, position: 1 }, t0);
        conn.on_message(joined(fresh), t0);
        let events = events(&mut conn);
        assert!(
            matches!(events[..], [Event::Link(Link::Reconnecting { .. }), Event::Link(Link::Reconnecting { .. }), Event::Link(Link::Up), Event::Attached(_), Event::LobbyJoined(_), Event::Queued(1), Event::Joined { .. }]),
            "{events:?}"
        );
    }

    /// Attached in a lobby and not looking: a drop rejoins the lobby — with
    /// the password it was let in by — and seeks nothing.
    #[test]
    fn a_drop_while_idle_rejoins_the_lobby_and_seeks_nothing() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Attach(who()), t0);
        conn.poll_dial();
        attach(&mut conn, t0);
        conn.submit(ClientMessage::JoinLobby { lobby: "k7m2qx".into(), password: Some("swordfish".into()) });
        let closed = LobbyInfo { id: "K7M2QX".into(), password: true, ..lobby() };
        conn.on_message(ServerMessage::LobbyJoined { lobby: closed.clone() }, t0);
        sent(&mut conn);
        events(&mut conn);
        conn.on_closed(Closed::Dropped, t0);
        conn.on_timeout(t0);
        conn.poll_dial();
        attach(&mut conn, t0);
        assert!(matches!(&sent(&mut conn)[..], [ClientMessage::JoinLobby { lobby, password: Some(password) }] if lobby == "K7M2QX" && password == "swordfish"), "the id the server gave, and the password");
        conn.on_message(ServerMessage::LobbyJoined { lobby: closed }, t0);
        assert!(sent(&mut conn).is_empty(), "nothing was being looked for");
        assert_eq!(conn.lobby, Some(("K7M2QX".to_string(), Some("swordfish".to_string()))));
    }

    /// The lobby a reattach tries to rejoin has gone — a player's lobby
    /// that emptied with this very drop: the player is told and is in no
    /// lobby, attached still.
    #[test]
    fn a_lobby_gone_while_the_connection_was_down_is_reported_and_let_go() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Attach(who()), t0);
        conn.poll_dial();
        seeking(&mut conn, t0);
        conn.on_message(ServerMessage::Queued { session_token: Uuid::new_v4(), position: 1 }, t0);
        events(&mut conn);
        conn.on_closed(Closed::Dropped, t0);
        conn.on_timeout(t0);
        conn.poll_dial();
        attach(&mut conn, t0);
        sent(&mut conn);
        conn.on_message(ServerMessage::LobbyRefused { reason: "there is no lobby STARTUP".into() }, t0);
        assert!(matches!(events(&mut conn).last(), Some(Event::LobbyRefused(_))));
        assert!(conn.lobby.is_none() && conn.seeking.is_none(), "nothing left to put back");
        assert!(conn.submit(ClientMessage::JoinLobby { lobby: "standard".into(), password: None }), "still attached");
    }

    /// A refusal of the player's own request leaves the lobby they were in.
    #[test]
    fn a_refused_join_keeps_the_lobby_the_player_was_in() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Attach(who()), t0);
        conn.poll_dial();
        seeking(&mut conn, t0);
        conn.on_message(ServerMessage::SeekRefused { reason: "no".into() }, t0);
        conn.submit(ClientMessage::JoinLobby { lobby: "nowhere".into(), password: None });
        conn.on_message(ServerMessage::LobbyRefused { reason: "no lobby NOWHERE".into() }, t0);
        assert_eq!(conn.lobby, Some(("startup".to_string(), None)));
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
    fn a_match_that_is_not_running_refuses_a_spectator_for_good() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Watch { match_id: Uuid::nil() }, t0);
        conn.poll_dial();
        conn.on_open(t0);
        conn.on_message(ServerMessage::ConnectRejected { reason: "no live match has that id".into() }, t0);
        assert!(conn.is_done());
        assert!(matches!(events(&mut conn).last(), Some(Event::Link(Link::Down(ConnectionError::Rejected(_))))));
    }

    /// A spectator's match is over: nothing to watch again. A player's
    /// seat is let go at the end, so a drop after it attaches afresh and
    /// the lobby is rejoined — the player stays, as they would have had
    /// the socket held.
    #[test]
    fn a_drop_after_the_game_ended_attaches_afresh() {
        let t0 = Instant::now();
        let mut watcher = Connection::new(Goal::Watch { match_id: Uuid::nil() }, t0);
        watcher.poll_dial();
        watcher.on_open(t0);
        watcher.on_message(ServerMessage::Spectating { match_id: Uuid::nil() }, t0);
        watcher.on_message(ServerMessage::GameEnded { winner: Side::Corp, reason: GameEndReason::AgendaThreshold }, t0);
        watcher.on_closed(Closed::Dropped, t0);
        assert!(watcher.is_done() && !watcher.poll_dial());

        let mut conn = seated(t0, Uuid::new_v4());
        conn.on_message(ServerMessage::GameEnded { winner: Side::Corp, reason: GameEndReason::AgendaThreshold }, t0);
        conn.on_closed(Closed::Dropped, t0);
        assert!(!conn.is_done());
        assert!(matches!(&events(&mut conn)[..], [Event::Message(ServerMessage::GameEnded { .. }), Event::BackInLobby(None), Event::Link(Link::Reconnecting { .. })]));
        conn.on_timeout(t0);
        conn.poll_dial();
        conn.on_open(t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Attach { .. }]), "not a resume: the seat is gone");
        conn.on_message(ServerMessage::Attached { lobbies: vec![lobby()] }, t0);
        assert!(matches!(&sent(&mut conn)[..], [ClientMessage::JoinLobby { lobby, .. }] if lobby == "startup"));
    }

    #[test]
    fn leaving_stops_everything() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Attach(who()), t0);
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

    fn signed_in() -> Who {
        let credentials = crate::identity::Credentials { identity: Identity::from_secret([1; 32]), dir: None };
        Who { credentials: Some(Box::new(credentials)), ..who() }
    }

    fn server() -> Identity {
        Identity::from_secret([2; 32])
    }

    fn challenge(lasting: bool) -> ServerMessage {
        ServerMessage::Challenge { nonce: Nonce([9; 32]), server_key: server().public_key(), lasting }
    }

    /// Identify, sign the challenge for this server, then the hello.
    #[test]
    fn a_player_with_a_key_proves_it_before_attaching_and_learns_a_lasting_server() {
        let t0 = Instant::now();
        let mut conn = Connection::new(Goal::Attach(signed_in()), t0);
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
        let mut conn = Connection::new(Goal::Attach(signed_in()), t0).with_pinned(Some(other));
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
        let mut conn = Connection::new(Goal::Attach(signed_in()), t0).with_pinned(Some(remembered));
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
        let mut conn = Connection::new(Goal::Attach(signed_in()), t0);
        conn.poll_dial();
        conn.on_open(t0);
        conn.on_message(challenge(true), t0);
        conn.on_message(ServerMessage::IdentifyRefused { reason: "no".into() }, t0);
        assert!(conn.is_done());
    }

    /// Seated through the key handshake, as the Corp in match `nil`.
    fn seated_signed_in(t0: Instant, lasting: bool) -> Connection {
        let me = Identity::from_secret([1; 32]).public_key();
        let mut conn = Connection::new(Goal::Attach(signed_in()), t0);
        conn.poll_dial();
        conn.on_open(t0);
        conn.on_message(challenge(lasting), t0);
        conn.on_message(ServerMessage::Identified { key: me }, t0);
        conn.on_message(ServerMessage::Attached { lobbies: vec![lobby()] }, t0);
        conn.submit(ClientMessage::JoinLobby { lobby: "startup".into(), password: None });
        conn.on_message(ServerMessage::LobbyJoined { lobby: lobby() }, t0);
        conn.submit(ClientMessage::Seek { chair: corp_chair() });
        conn.on_message(joined(Uuid::new_v4()), t0);
        sent(&mut conn);
        events(&mut conn);
        conn
    }

    /// A reconnect proves the key again — it is a new socket — and then
    /// resumes, held to the key the first connection met.
    #[test]
    fn a_reconnect_proves_the_key_again_then_resumes() {
        let t0 = Instant::now();
        let me = Identity::from_secret([1; 32]).public_key();
        let mut conn = seated_signed_in(t0, true);
        let token = conn.token.unwrap();

        conn.on_closed(Closed::Dropped, t0);
        conn.on_timeout(t0);
        conn.poll_dial();
        conn.on_open(t0);
        assert!(matches!(sent(&mut conn)[..], [ClientMessage::Identify { .. }]));
        let impostor = ServerMessage::Challenge { nonce: Nonce([1; 32]), server_key: Identity::from_secret([4; 32]).public_key(), lasting: true };
        let mut again = Connection::new(Goal::Attach(signed_in()), t0).with_pinned(conn.pinned);
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

    /// A statement true to this seat is signed, unasked and unshown — the
    /// deck checked is the one the chair the game was found in brought.
    #[test]
    fn a_seat_statement_true_to_this_seat_is_signed() {
        let t0 = Instant::now();
        let mut conn = seated_signed_in(t0, false);
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
        let mut conn = seated_signed_in(t0, false);
        let brick = netrunner_core::decks::by_id("brick_stack").unwrap().to_deck();
        let other = netrunner_core::decks::by_id("stolen_goods").unwrap().to_deck();
        for (said, salt) in [(statement(&other, Side::Corp), "salt"), (statement(&brick, Side::Runner), "salt"), (statement(&brick, Side::Corp), "pepper")] {
            conn.on_message(ServerMessage::SignSeat { statement: said, salt: salt.into() }, t0);
            assert!(sent(&mut conn).is_empty());
        }
    }
}
