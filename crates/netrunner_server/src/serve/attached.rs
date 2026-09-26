//! An attached connection (`ClientMessage::Attach`): one task per socket
//! that stays for as long as the socket does, carrying the player between
//! lobbies, into a game and back (Phase 4 §7, decided 26 September 2026).
//!
//! **No deck until a game is looked for.** Attaching, listing, making and
//! joining lobbies take a name and nothing else; a deck — one for a chair
//! chosen, one for each side for a random chair — rides on `Seek`, and a
//! player chooses afresh for every game from whatever decks their client
//! holds, without reconnecting.
//!
//! **A match never holds the socket.** Seeking hands the lobby a
//! `PlayerSlot` of this task's own channels, so the match talks to the
//! task and the task to the socket. That is what lets the connection
//! outlive the game: when the match lets go of the seat its channel
//! closes, and the task — still holding the socket — sends `BackInLobby`.
//! A `Connect` seat, by contrast, is the socket itself, and closes with
//! the game, as it always did.
//!
//! **What a dropped socket costs.** A seek is withdrawn with the socket,
//! and the lobby's count forgets it; a seat in a game is kept for the
//! reconnect grace, as any seat is, and `Resume` with its token attaches
//! a new socket to it (`serve::handle_connection`), which goes back to
//! the lobby when the game ends.

use super::*;

/// A seek or a seat: the channels the lobby's slot, and then the match,
/// talks through.
pub(super) struct Playing {
    /// The seat's token, off `Queued` or `MatchJoined` as they pass;
    /// what a cancel takes off the queue.
    pub(super) token: Uuid,
    /// What the lobby and the match send this player.
    pub(super) out: mpsc::UnboundedReceiver<ServerMessage>,
    /// What this player sends the match.
    pub(super) into: mpsc::UnboundedSender<ClientMessage>,
    /// The lobby the game was found in, returned to after it.
    pub(super) lobby: String,
}

/// Where a connection is.
struct Attached {
    shared: Shared,
    player_name: String,
    tx: mpsc::UnboundedSender<ServerMessage>,
    /// The lobby this connection is in, by id.
    lobby: Option<String>,
    /// Seated in a match: set when its `MatchJoined` passes.
    in_match: bool,
}

/// What woke the task.
enum Woke {
    Client(Option<ClientMessage>),
    Match(Option<ServerMessage>),
}

/// The next message from the lobby or the match, or never when there is
/// neither.
async fn from_play(playing: &mut Option<Playing>) -> Option<ServerMessage> {
    match playing {
        Some(playing) => playing.out.recv().await,
        None => std::future::pending().await,
    }
}

/// Runs an attached connection until its socket goes. `resumed` is a seat
/// taken back by `Resume`, already reattached to its match.
pub(super) async fn run(
    shared: Shared,
    player_name: String,
    tx: mpsc::UnboundedSender<ServerMessage>,
    mut rx: mpsc::UnboundedReceiver<ClientMessage>,
    resumed: Option<Playing>,
) {
    let mut conn = Attached { shared, player_name, tx, lobby: None, in_match: resumed.is_some() };
    let mut playing = resumed;
    match &playing {
        Some(seat) => {
            let lobby = seat.lobby.clone();
            conn.enter(lobby);
        }
        None => {
            let lobbies = conn.shared.lock().open_lobbies(&conn.shared.options);
            conn.send(ServerMessage::Attached { lobbies });
        }
    }
    loop {
        let woke = tokio::select! {
            message = rx.recv() => Woke::Client(message),
            message = from_play(&mut playing) => Woke::Match(message),
        };
        match woke {
            Woke::Client(None) => break,
            Woke::Client(Some(message)) => conn.on_client(message, &mut playing),
            Woke::Match(Some(message)) => conn.on_match(message, &mut playing),
            Woke::Match(None) => {
                if !conn.play_ended(&mut playing) {
                    break;
                }
            }
        }
    }
    // The socket is gone. A seek goes with it; a seat stays with its
    // match for the grace, which starts when `playing` drops here.
    if let Some(seek) = playing.take().filter(|_| !conn.in_match) {
        conn.withdraw(seek.token);
    }
    conn.leave_lobby();
}

impl Attached {
    fn send(&self, message: ServerMessage) {
        let _ = self.tx.send(message);
    }

    fn on_client(&mut self, message: ClientMessage, playing: &mut Option<Playing>) {
        match message {
            // In a game, the game's messages go to the game and nothing
            // else is answered: the lobby is where the player returns.
            ClientMessage::SubmitAction(_) | ClientMessage::Surrender => {
                if let Some(seat) = playing.as_ref().filter(|_| self.in_match) {
                    let _ = seat.into.send(message);
                }
            }
            ClientMessage::ListLobbies => {
                let lobbies = self.shared.lock().open_lobbies(&self.shared.options);
                self.send(ServerMessage::Lobbies { lobbies });
            }
            ClientMessage::ListMatches => {
                let list = self.shared.match_list();
                self.send(list);
            }
            ClientMessage::CreateLobby { name, format, closed, password } => {
                if playing.is_some() {
                    return self.send(ServerMessage::LobbyRefused { reason: BUSY.into() });
                }
                match self.create(name, format, closed, password) {
                    Ok(id) => self.move_to(id),
                    Err(reason) => self.send(ServerMessage::LobbyRefused { reason }),
                }
            }
            ClientMessage::JoinLobby { lobby, password } => {
                if playing.is_some() {
                    return self.send(ServerMessage::LobbyRefused { reason: BUSY.into() });
                }
                match self.admits(&lobby, password.as_deref()) {
                    Ok(()) => self.move_to(lobby),
                    Err(reason) => self.send(ServerMessage::LobbyRefused { reason }),
                }
            }
            ClientMessage::LeaveLobby => {
                if playing.is_some() {
                    return self.send(ServerMessage::LobbyRefused { reason: BUSY.into() });
                }
                self.leave_lobby();
                self.send(ServerMessage::LobbyLeft);
            }
            ClientMessage::Seek { chair } => {
                if playing.is_some() {
                    return self.send(ServerMessage::SeekRefused { reason: "already looking for a game, or playing one: cancel first".into() });
                }
                match self.seek(chair) {
                    Ok(seek) => *playing = Some(seek),
                    Err(reason) => self.send(ServerMessage::SeekRefused { reason }),
                }
            }
            ClientMessage::CancelSeek => {
                if let Some(seek) = playing.take_if(|_| !self.in_match) {
                    self.withdraw(seek.token);
                    self.send(ServerMessage::SeekCancelled);
                }
            }
            // Watching from an attached connection is a later stage; a
            // second hello is the client repeating itself.
            ClientMessage::Attach { .. } | ClientMessage::Connect { .. } | ClientMessage::Resume { .. } | ClientMessage::Spectate { .. } => {}
        }
    }

    /// A message from the lobby or the match, on its way to the socket.
    fn on_match(&mut self, message: ServerMessage, playing: &mut Option<Playing>) {
        match &message {
            ServerMessage::Queued { session_token, .. } | ServerMessage::MatchJoined { session_token, .. } => {
                if let Some(seat) = playing.as_mut() {
                    seat.token = *session_token;
                }
                self.in_match |= matches!(message, ServerMessage::MatchJoined { .. });
            }
            // Refused before it was paired (the server at its match cap):
            // a refused seek, not a refused connection.
            ServerMessage::ConnectRejected { reason } if !self.in_match => {
                return self.send(ServerMessage::SeekRefused { reason: reason.clone() });
            }
            _ => {}
        }
        self.send(message);
    }

    /// The lobby or the match let go. Returns whether the connection goes
    /// on: not when the seat was taken over by a newer socket (`Resume`),
    /// which is the one that plays on.
    fn play_ended(&mut self, playing: &mut Option<Playing>) -> bool {
        let Some(seat) = playing.take() else { return true };
        if !self.in_match {
            // Withdrawn by the server — refused, or swept — rather than by
            // this connection, which clears `playing` before it withdraws.
            return true;
        }
        let taken_over = self.shared.lock().seats.get(&seat.token).is_some_and(|ticket| ticket.handle.is_live());
        if taken_over {
            return false;
        }
        self.in_match = false;
        let lobby = self.lobby.as_ref().and_then(|id| self.shared.lock().lobby_info(id, &self.shared.options));
        self.send(ServerMessage::BackInLobby { lobby });
        true
    }

    /// Checks `chair`'s decks against the lobby's format and puts this
    /// player in its queue — or, on a bot daemon, straight into a game.
    fn seek(&mut self, chair: Chair) -> Result<Playing, String> {
        let Some(lobby) = self.lobby.clone() else { return Err("join a lobby first".into()) };
        let format = self.shared.lock().lobby_info(&lobby, &self.shared.options).ok_or("that lobby has gone")?.format;
        let check = |deck: &DeckFile, side: Side| -> Result<(), String> {
            if deck.side != side {
                return Err(format!("{:?} is a {:?} deck, not a {side:?} one", deck.name, deck.side));
            }
            deck.validate(&self.shared.cards, format).map(drop).map_err(|error| format!("your deck {:?} is not legal in {format:?}: {error}", deck.name))
        };
        let (deck, random) = match chair {
            Chair::Corp(deck) => {
                check(&deck, Side::Corp)?;
                (Some(deck), None)
            }
            Chair::Runner(deck) => {
                check(&deck, Side::Runner)?;
                (Some(deck), None)
            }
            Chair::Random { corp, runner } => {
                check(&corp, Side::Corp)?;
                check(&runner, Side::Runner)?;
                (None, Some((corp, runner)))
            }
        };
        let (out_tx, out) = mpsc::unbounded_channel::<ServerMessage>();
        let (into, into_rx) = mpsc::unbounded_channel::<ClientMessage>();
        let slot = PlayerSlot::Channel { tx: out_tx.clone(), rx: into_rx };
        let token = Uuid::new_v4();
        match self.shared.options.bot_runner {
            ServeBotKind::None => {
                let newcomer = PendingHuman {
                    token,
                    player_name: self.player_name.clone(),
                    preferred_side: None,
                    lobby: lobby.clone(),
                    format,
                    deck,
                    random,
                    attached: Some(lobby.clone()),
                    tx: out_tx,
                    slot,
                };
                enqueue_or_pair(&self.shared, newcomer);
            }
            // A bot sits opposite whatever chair was chosen; a random one
            // is a coin here, since there is no second player to pair.
            kind => {
                let (side, deck) = match (deck, random) {
                    (Some(deck), _) => (deck.side, deck),
                    (None, Some((corp, runner))) => {
                        if rand::random::<bool>() {
                            (Side::Corp, corp)
                        } else {
                            (Side::Runner, runner)
                        }
                    }
                    (None, None) => unreachable!("every chair brings a deck"),
                };
                seat_vs_bot(&self.shared, kind, self.player_name.clone(), Some(side), format, Some(deck), Some(lobby.clone()), out_tx, slot);
            }
        }
        Ok(Playing { token, out, into, lobby })
    }

    /// Takes this connection's seek off its lobby's queue.
    fn withdraw(&self, token: Uuid) {
        let mut registry = self.shared.lock();
        registry.lobby.retain(|waiter| waiter.token != token);
        if let Some(id) = &self.lobby {
            registry.forget_if_empty(id);
        }
    }

    /// Whether lobby `id` lets this connection in.
    fn admits(&self, id: &str, password: Option<&str>) -> Result<(), String> {
        let registry = self.shared.lock();
        if let Some(lobby) = registry.player_lobbies.get(id) {
            return match (&lobby.password, password) {
                (None, _) => Ok(()),
                (Some(wanted), Some(given)) if wanted == given => Ok(()),
                (Some(_), None) => Err(format!("lobby {id} asks for a password")),
                (Some(_), Some(_)) => Err(format!("that is not lobby {id}'s password")),
            };
        }
        match registry.lobby_info(id, &self.shared.options) {
            Some(_) => Ok(()),
            None => Err(format!("there is no lobby {id} on this server")),
        }
    }

    /// Makes a player's lobby, returning its id.
    fn create(&self, name: String, format: NsgFormat, closed: bool, password: Option<String>) -> Result<String, String> {
        let name: String = name.trim().chars().take(MAX_LOBBY_NAME).collect();
        if name.is_empty() {
            return Err("a lobby needs a name".into());
        }
        if !self.shared.options.formats.contains(&format) {
            return Err(format!("this server has no {format:?} lobby to offer"));
        }
        let password = password.map(|password| password.trim().to_string()).filter(|password| !password.is_empty());
        let mut registry = self.shared.lock();
        let id = loop {
            let id = lobby_code();
            if !registry.player_lobbies.contains_key(&id) {
                break id;
            }
        };
        registry.player_lobbies.insert(id.clone(), PlayerLobby { name, format, closed, password });
        Ok(id)
    }

    /// Leaves the lobby this connection is in for `id`, and says so.
    fn move_to(&mut self, id: String) {
        self.leave_lobby();
        self.enter(id.clone());
        let info = self.shared.lock().lobby_info(&id, &self.shared.options);
        if let Some(lobby) = info {
            self.send(ServerMessage::LobbyJoined { lobby });
        }
    }

    fn enter(&mut self, id: String) {
        *self.shared.lock().members.entry(id.clone()).or_insert(0) += 1;
        self.lobby = Some(id);
    }

    fn leave_lobby(&mut self) {
        let Some(id) = self.lobby.take() else { return };
        let mut registry = self.shared.lock();
        if let Some(count) = registry.members.get_mut(&id) {
            *count = count.saturating_sub(1);
        }
        registry.forget_if_empty(&id);
    }
}

/// Why a lobby cannot be changed right now.
const BUSY: &str = "cancel the game you are looking for, or finish the one you are playing, first";

/// The longest name a player's lobby may have.
const MAX_LOBBY_NAME: usize = 40;

/// A player lobby's id: six characters from an alphabet with no look-alikes
/// (no 0/O, 1/I/L), short enough to read out and never a format's name.
fn lobby_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    (0..6).map(|_| ALPHABET[rand::random_range(0..ALPHABET.len())] as char).collect()
}
