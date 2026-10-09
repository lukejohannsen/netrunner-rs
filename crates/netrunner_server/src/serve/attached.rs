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
    /// This connection in `Registry::attached`, for what is pushed to it
    /// unasked and for leaving its own answer out of a push.
    link: Uuid,
    player_name: String,
    /// The key this socket proved before attaching, if it did.
    key: Option<PublicKey>,
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
    key: Option<PublicKey>,
    tx: mpsc::UnboundedSender<ServerMessage>,
    mut rx: mpsc::UnboundedReceiver<ClientMessage>,
    resumed: Option<Playing>,
) {
    let link = Uuid::new_v4();
    shared.lock().attached.insert(link, AttachedLink { key, tx: tx.clone() });
    let mut conn = Attached { shared, link, player_name, key, tx, lobby: None, in_match: resumed.is_some() };
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
    conn.shared.lock().attached.remove(&conn.link);
}

impl Attached {
    fn send(&self, message: ServerMessage) {
        let _ = self.tx.send(message);
    }

    fn on_client(&mut self, message: ClientMessage, playing: &mut Option<Playing>) {
        match message {
            // In a game, the game's messages go to the game and nothing
            // else is answered: the lobby is where the player returns.
            ClientMessage::SubmitAction(_) | ClientMessage::Surrender | ClientMessage::TakeBack => {
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
            ClientMessage::CreateLobby { name, format, closed, password, casual } => {
                if playing.is_some() {
                    return self.send(ServerMessage::LobbyRefused { reason: BUSY.into() });
                }
                match self.create(name, format, closed, password, casual) {
                    Ok(id) => self.move_to(id),
                    Err(reason) => self.send(ServerMessage::LobbyRefused { reason }),
                }
            }
            ClientMessage::JoinLobby { lobby, password } => {
                let lobby = lobby_id_as_typed(&lobby);
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
            // Asked when the client attaches and after every game, for
            // the Server page's line (Phase 4 §7 stage 5).
            ClientMessage::MyStanding => {
                let standing = self.shared.standing_of(self.key);
                self.send(standing);
            }
            ClientMessage::SeatSigned { signature } => {
                if let Some(seat) = playing.as_ref().filter(|_| self.in_match) {
                    self.shared.accept_commitment(seat.token, signature);
                }
            }
            // Tournaments (Phase 4 §7 stage 6a): held by a daemon that
            // keeps things, made and entered by proved keys.
            ClientMessage::CreateTournament { name, format } => match self.create_tournament(name, format) {
                Ok(tournament) => self.send(ServerMessage::Tournament { tournament }),
                Err(reason) => self.send(ServerMessage::TournamentRefused { reason }),
            },
            ClientMessage::ListTournaments => {
                let tournaments = self.shared.lock().tournaments.list();
                self.send(ServerMessage::Tournaments { tournaments });
            }
            ClientMessage::Register { tournament, corp, runner, salt, statement } => match self.register(&tournament, *corp, *runner, salt, statement) {
                Ok(tournament) => self.send(ServerMessage::Tournament { tournament }),
                Err(reason) => self.send(ServerMessage::TournamentRefused { reason }),
            },
            ClientMessage::Unregister { tournament } => match self.unregister(&tournament) {
                Ok(tournament) => self.send(ServerMessage::Tournament { tournament }),
                Err(reason) => self.send(ServerMessage::TournamentRefused { reason }),
            },
            // The rounds (Phase 4 §7 stage 6b): the organizer's to begin,
            // record and end; a table's seat anyone paired may take.
            ClientMessage::BeginRound { tournament } => match self.begin_round(&tournament) {
                Ok(tournament) => self.send(ServerMessage::Tournament { tournament }),
                Err(reason) => self.send(ServerMessage::TournamentRefused { reason }),
            },
            ClientMessage::FinishTournament { tournament } => match self.finish_tournament(&tournament) {
                Ok(tournament) => self.send(ServerMessage::Tournament { tournament }),
                Err(reason) => self.send(ServerMessage::TournamentRefused { reason }),
            },
            ClientMessage::OfferDraw { tournament } => match self.offer_draw(&tournament) {
                Ok(tournament) => self.send(ServerMessage::Tournament { tournament }),
                Err(reason) => self.send(ServerMessage::TournamentRefused { reason }),
            },
            ClientMessage::RecordResult { tournament, table, outcome } => match self.record_result(&tournament, table, outcome) {
                Ok(tournament) => self.send(ServerMessage::Tournament { tournament }),
                Err(reason) => self.send(ServerMessage::TournamentRefused { reason }),
            },
            ClientMessage::Sit { tournament } => {
                if playing.is_some() {
                    return self.send(ServerMessage::SeekRefused { reason: "already looking for a game, or playing one: cancel first".into() });
                }
                match self.sit(&tournament) {
                    Ok(seat) => *playing = Some(seat),
                    Err(reason) => self.send(ServerMessage::SeekRefused { reason }),
                }
            }
            // A key is proved before attaching, never after.
            ClientMessage::Attach { .. } | ClientMessage::Resume { .. } | ClientMessage::Spectate { .. } | ClientMessage::Identify { .. } | ClientMessage::Prove { .. } => {}
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
                let newcomer = PendingHuman { token, player_name: self.player_name.clone(), key: self.key, lobby: lobby.clone(), format, deck, random, tx: out_tx, slot };
                enqueue_or_pair(&self.shared, newcomer);
            }
            // A bot sits opposite whatever chair was chosen; a random one
            // is a coin here, since there is no second player to pair.
            kind => {
                let deck = match (deck, random) {
                    (Some(deck), _) => deck,
                    (None, Some((corp, runner))) => {
                        if rand::random::<bool>() {
                            corp
                        } else {
                            runner
                        }
                    }
                    (None, None) => unreachable!("every chair brings a deck"),
                };
                seat_vs_bot(&self.shared, kind, self.player_name.clone(), format, deck, lobby.clone(), out_tx, slot);
            }
        }
        Ok(Playing { token, out, into, lobby })
    }

    /// Takes this connection's seek off its lobby's queue — or its seat
    /// off the tournament table it was waiting at.
    fn withdraw(&self, token: Uuid) {
        let mut registry = self.shared.lock();
        registry.lobby.retain(|waiter| waiter.token != token);
        registry.tables.retain(|_, state| !matches!(state, TableState::Waiting(seated) if seated.token == token));
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
    fn create(&self, name: String, format: NsgFormat, closed: bool, password: Option<String>, casual: bool) -> Result<String, String> {
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
        registry.player_lobbies.insert(id.clone(), PlayerLobby { name, format, closed, password, casual });
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

/// A lobby id as a person typed it: a format's lobby in any case
/// (`Startup`, `startup`), a player's code in any case (`k7m2qx`), with
/// the spaces around it gone.
impl Attached {
    /// A tournament of this key's, on a daemon that keeps things.
    fn create_tournament(&self, name: String, format: NsgFormat) -> Result<TournamentInfo, String> {
        let organizer = self.key.ok_or("a tournament is run by a key: prove one before attaching")?;
        if self.shared.options.data_dir.is_none() {
            return Err("this server keeps nothing between runs, so it cannot hold a tournament".into());
        }
        let name: String = name.trim().chars().take(MAX_LOBBY_NAME).collect();
        if name.is_empty() {
            return Err("a tournament needs a name".into());
        }
        if !self.shared.options.formats.contains(&format) {
            return Err(format!("this server does not play {format:?}"));
        }
        let mut registry = self.shared.lock();
        let id = loop {
            let id = lobby_code();
            if !registry.tournaments.0.contains_key(&id) {
                break id;
            }
        };
        let tournament = Tournament { name, format, organizer, created_at: unix_now(), state: TournamentState::Registering, entrants: Vec::new(), seeding: Vec::new(), rounds: Vec::new(), dropped: Vec::new(), draw_offers: Vec::new() };
        let info = tournament.info(&id);
        registry.tournaments.0.insert(id, tournament);
        self.shared.save_tournaments(&registry);
        Ok(info)
    }

    /// This key's entry in a tournament: the statement checked against
    /// the key, the server, the tournament and the decks sent, and the
    /// decks against the tournament's format, before any of it is kept.
    fn register(&self, id: &str, corp: DeckFile, runner: DeckFile, salt: String, statement: Signed) -> Result<TournamentInfo, String> {
        let key = self.key.ok_or("registering needs a key: prove one before attaching")?;
        if statement.key != key {
            return Err("the statement is signed by another key".into());
        }
        let payload = statement.verify(REGISTRATION_TAG).map_err(|error| format!("the statement does not hold: {error}"))?;
        let said: RegistrationStatement = serde_json::from_str(payload).map_err(|error| format!("not a registration statement: {error}"))?;
        // A code is read out and typed back in any case: the id and the
        // one the statement names are both taken as the code they spell.
        let id = id.trim().to_uppercase();
        if said.tournament.trim().to_uppercase() != id || said.key != key || said.server_key != self.shared.identity.public_key() {
            return Err("the statement names another tournament, key or server".into());
        }
        if salt.trim().is_empty() || salt.len() > 64 {
            return Err("a salt is 1 to 64 characters".into());
        }
        let (format, open) = {
            let registry = self.shared.lock();
            let tournament = registry.tournaments.0.get(&id).ok_or("no such tournament")?;
            (tournament.format, tournament.state == TournamentState::Registering)
        };
        if !open {
            return Err("registration has closed".into());
        }
        for (deck, side) in [(&corp, Side::Corp), (&runner, Side::Runner)] {
            if deck.side != side {
                return Err(format!("{:?} is a {:?} deck, not a {side:?} one", deck.name, deck.side));
            }
            deck.validate(&self.shared.cards, format).map_err(|error| format!("{:?} is not legal in {format:?}: {error}", deck.name))?;
        }
        if said.corp_hash != statements::deck_hash(&salt, &corp.to_deck()) || said.runner_hash != statements::deck_hash(&salt, &runner.to_deck()) {
            return Err("the commitment does not name the decks sent".into());
        }
        let mut registry = self.shared.lock();
        let tournament = registry.tournaments.0.get_mut(&id).ok_or("no such tournament")?;
        let entry = Entry { name: self.player_name.clone(), key, corp, runner, salt, commitment: statement };
        match tournament.entrants.iter_mut().find(|entry| entry.key == key) {
            Some(existing) => *existing = entry,
            None => tournament.entrants.push(entry),
        }
        let info = tournament.info(&id);
        self.shared.save_tournaments(&registry);
        self.shared.announce(&registry, &id, Some(self.link));
        Ok(info)
    }

    /// Withdraw while registration is open, or drop mid-event. A drop is
    /// the policies' player who leaves, not an erasure: the entry and
    /// every result stay in the standings — their opponents' Strength of
    /// Schedule still counts the games against them — and `swiss::pair`
    /// leaves them out of every later table and bye. A table of the
    /// current round they have not played is forfeit: the opponent takes
    /// the win, and whoever is waiting at it is stood up and told which.
    /// A drop with a game under way is refused rather than ended here —
    /// the board's Surrender is the way out of a game, and it writes the
    /// loss through the table — so no game outlives its player's entry.
    fn unregister(&self, id: &str) -> Result<TournamentInfo, String> {
        let key = self.key.ok_or("withdrawing needs a key: prove one before attaching")?;
        let id = id.trim().to_uppercase();
        let mut registry = self.shared.lock();
        let tournament = registry.tournaments.0.get(&id).ok_or("no such tournament")?;
        if tournament.entry(&key).is_none() {
            return Err("you are not registered there".into());
        }
        let registering = tournament.state == TournamentState::Registering;
        let forfeit = match tournament.state {
            TournamentState::Registering => None,
            TournamentState::Finished => return Err("the tournament is over".into()),
            TournamentState::Playing { .. } => {
                if tournament.dropped.contains(&key) {
                    return Err("you have dropped already".into());
                }
                let (round, current) = tournament.current().ok_or("the round being played is missing")?;
                current.table_of(&key).filter(|(_, slot)| slot.result.is_none()).map(|(table, slot)| {
                    let outcome = match slot.role_of(&key) {
                        Some(TableRole::Corp) => TableOutcome::RunnerWon,
                        _ => TableOutcome::CorpWon,
                    };
                    (TableRef { tournament: id.clone(), round, table }, outcome)
                })
            }
        };
        if let Some((table_ref, _)) = &forfeit {
            match registry.tables.get(table_ref) {
                Some(TableState::Playing(_)) => return Err("your game is under way: concede it on the board, which records the loss, then drop".into()),
                Some(TableState::Waiting(seated)) => {
                    refuse(&seated.tx, if seated.key == key { "you dropped from the tournament" } else { "your opponent dropped: the table is yours" });
                    registry.tables.remove(table_ref);
                }
                None => {}
            }
        }
        let tournament = registry.tournaments.0.get_mut(&id).ok_or("no such tournament")?;
        if registering {
            tournament.entrants.retain(|entry| entry.key != key);
        } else {
            tournament.dropped.push(key);
            if let Some((table_ref, outcome)) = forfeit {
                tournament.rounds[table_ref.round].tables[table_ref.table].result = Some(outcome);
                tournament.draw_offers.retain(|offer| offer.table != table_ref.table);
            }
        }
        let info = tournament.info(&id);
        self.shared.save_tournaments(&registry);
        self.shared.announce(&registry, &id, Some(self.link));
        Ok(info)
    }

    /// The tournament `id`, for this key as its organizer, under the
    /// registry lock the caller holds.
    fn held<'a>(&self, registry: &'a mut Registry, id: &str) -> Result<&'a mut Tournament, String> {
        let key = self.key.ok_or("a tournament is run by a key: prove one before attaching")?;
        let tournament = registry.tournaments.0.get_mut(id).ok_or("no such tournament")?;
        if tournament.organizer != key {
            return Err("only the organizer may do that".into());
        }
        Ok(tournament)
    }

    /// The organizer begins the next round. The first closes
    /// registration — two entrants at least, or there is nobody to pair
    /// — and fixes the seeding, a shuffle of the entrants off the
    /// daemon's seed and the tournament's age, so a `--seed` daemon seeds
    /// the same way every run and any other at random; each round after
    /// needs the one before complete, since a table with no result has
    /// no points to pair by, and two players who have not dropped.
    fn begin_round(&self, id: &str) -> Result<TournamentInfo, String> {
        let id = id.trim().to_uppercase();
        let mut registry = self.shared.lock();
        let seed = self.shared.base_seed;
        let tournament = self.held(&mut registry, &id)?;
        match tournament.state {
            TournamentState::Registering => {
                if tournament.entrants.len() < 2 {
                    return Err("a round needs two entrants at least".into());
                }
                use rand::seq::SliceRandom;
                use rand::SeedableRng;
                let mut seeding: Vec<PublicKey> = tournament.entrants.iter().map(|entry| entry.key).collect();
                seeding.shuffle(&mut rand::rngs::StdRng::seed_from_u64(seed.wrapping_add(tournament.created_at)));
                tournament.seeding = seeding;
            }
            TournamentState::Playing { .. } => {
                let (_, current) = tournament.current().ok_or("the round being played is missing")?;
                if !current.complete() {
                    return Err("a table of this round has no result yet".into());
                }
                if tournament.entrants.len() - tournament.dropped.len() < 2 {
                    return Err("fewer than two players remain: end the tournament".into());
                }
            }
            TournamentState::Finished => return Err("the tournament is over".into()),
        }
        let next = swiss::pair(&tournament.seeding, &tournament.rounds, &tournament.dropped);
        tournament.rounds.push(next);
        tournament.draw_offers.clear();
        tournament.state = TournamentState::Playing { round: tournament.rounds.len() as u32 };
        let info = tournament.info(&id);
        self.shared.save_tournaments(&registry);
        self.shared.announce(&registry, &id, Some(self.link));
        Ok(info)
    }

    /// The organizer ends the tournament, once the round being played is
    /// complete: the standings are final.
    fn finish_tournament(&self, id: &str) -> Result<TournamentInfo, String> {
        let id = id.trim().to_uppercase();
        let mut registry = self.shared.lock();
        let tournament = self.held(&mut registry, &id)?;
        match tournament.state {
            TournamentState::Registering => return Err("nothing has been played yet: begin a round, or leave it".into()),
            TournamentState::Finished => return Err("the tournament is over".into()),
            TournamentState::Playing { .. } => {
                let (_, current) = tournament.current().ok_or("the round being played is missing")?;
                if !current.complete() {
                    return Err("a table of this round has no result yet".into());
                }
            }
        }
        tournament.state = TournamentState::Finished;
        let info = tournament.info(&id);
        self.shared.save_tournaments(&registry);
        self.shared.announce(&registry, &id, Some(self.link));
        Ok(info)
    }

    /// The organizer records a result for a table of the round being
    /// played that has none — a no-show, or what the players agreed —
    /// and not for one whose game is under way, which decides itself. A
    /// player waiting at it is stood up, told why.
    fn record_result(&self, id: &str, table: usize, outcome: TableOutcome) -> Result<TournamentInfo, String> {
        let id = id.trim().to_uppercase();
        let mut registry = self.shared.lock();
        let round = {
            let tournament = self.held(&mut registry, &id)?;
            let (round, current) = tournament.current().ok_or("no round is being played")?;
            let slot = current.tables.get(table).ok_or("no such table this round")?;
            if slot.result.is_some() {
                return Err("that table has its result".into());
            }
            round
        };
        let table_ref = TableRef { tournament: id.clone(), round, table };
        match registry.tables.remove(&table_ref) {
            Some(TableState::Playing(match_id)) => {
                registry.tables.insert(table_ref, TableState::Playing(match_id));
                return Err("that table's game is being played: it decides itself".into());
            }
            Some(TableState::Waiting(seated)) => refuse(&seated.tx, "the organizer recorded this table's result"),
            None => {}
        }
        let tournament = registry.tournaments.0.get_mut(&id).ok_or("no such tournament")?;
        tournament.rounds[round].tables[table].result = Some(outcome);
        tournament.draw_offers.retain(|offer| offer.table != table);
        let info = tournament.info(&id);
        self.shared.save_tournaments(&registry);
        self.shared.announce(&registry, &id, Some(self.link));
        Ok(info)
    }

    /// This key offers the opponent at its table an intentional draw
    /// (2.5.8). The first offer is published to the table and waits; the
    /// second — the opponent's — is the agreement, and the table's result
    /// is a tie, with whoever was seated and waiting stood up and told.
    /// Not while the game is under way: a game that has started decides
    /// itself, and an offer made before it lapsed when it started. The
    /// policies' five-minute window is the round clock's to keep, which
    /// is not built yet.
    fn offer_draw(&self, id: &str) -> Result<TournamentInfo, String> {
        let key = self.key.ok_or("a draw is offered by a key: prove one before attaching")?;
        let id = id.trim().to_uppercase();
        let mut registry = self.shared.lock();
        let (round, table, opponent) = {
            let tournament = registry.tournaments.0.get(&id).ok_or("no such tournament")?;
            let (round, current) = tournament.current().ok_or("no round is being played")?;
            let (table, slot) = current.table_of(&key).ok_or("you have no table this round")?;
            if slot.result.is_some() {
                return Err("that table has its result".into());
            }
            if tournament.draw_offers.iter().any(|offer| offer.table == table && offer.by == key) {
                return Err("you have offered a draw already; it stands until your opponent answers or the game starts".into());
            }
            let opponent = *slot.opponent_of(&key).expect("a table has two seats");
            (round, table, opponent)
        };
        let table_ref = TableRef { tournament: id.clone(), round, table };
        if matches!(registry.tables.get(&table_ref), Some(TableState::Playing(_))) {
            return Err("your game is under way: it decides itself".into());
        }
        let agreed = registry.tournaments.0.get(&id).is_some_and(|tournament| tournament.draw_offers.iter().any(|offer| offer.table == table && offer.by == opponent));
        if let Some(TableState::Waiting(seated)) = agreed.then(|| registry.tables.remove(&table_ref)).flatten() {
            refuse(&seated.tx, "you agreed a draw: the table's result is a tie");
        }
        let tournament = registry.tournaments.0.get_mut(&id).ok_or("no such tournament")?;
        if agreed {
            tournament.rounds[round].tables[table].result = Some(TableOutcome::Tie);
            tournament.draw_offers.retain(|offer| offer.table != table);
        } else {
            tournament.draw_offers.push(DrawOffer { table, by: key });
        }
        let info = tournament.info(&id);
        self.shared.save_tournaments(&registry);
        self.shared.announce(&registry, &id, Some(self.link));
        Ok(info)
    }

    /// This key takes its seat at this round's table. The first to sit
    /// waits, as a seek does; the second starts the game, dealt from the
    /// two registered lists on the sides the pairing gave — nothing is
    /// brought, because everything was locked at registration.
    fn sit(&mut self, id: &str) -> Result<Playing, String> {
        let key = self.key.ok_or("a seat is a key's: prove one before attaching")?;
        let id = id.trim().to_uppercase();
        let mut registry = self.shared.lock();
        let (round, table, format, mine, theirs) = {
            let tournament = registry.tournaments.0.get(&id).ok_or("no such tournament")?;
            let (round, current) = tournament.current().ok_or("no round is being played")?;
            let (table, slot) = current.table_of(&key).ok_or("you have no table this round")?;
            if slot.result.is_some() {
                return Err("that table has its result".into());
            }
            let role = slot.role_of(&key).expect("table_of found this key");
            let opponent = *slot.opponent_of(&key).expect("a table has two seats");
            let deck_for = |entry: &Entry, role: TableRole| match role {
                TableRole::Corp => Box::new(entry.corp.clone()),
                TableRole::Runner => Box::new(entry.runner.clone()),
            };
            let mine = deck_for(tournament.entry(&key).ok_or("you are not entered")?, role);
            let other_role = match role {
                TableRole::Corp => TableRole::Runner,
                TableRole::Runner => TableRole::Corp,
            };
            let theirs = deck_for(tournament.entry(&opponent).ok_or("your opponent is not entered")?, other_role);
            (round, table, tournament.format, (role, mine), theirs)
        };
        let table_ref = TableRef { tournament: id.clone(), round, table };
        let (out_tx, out) = mpsc::unbounded_channel::<ServerMessage>();
        let (into, into_rx) = mpsc::unbounded_channel::<ClientMessage>();
        let slot = PlayerSlot::Channel { tx: out_tx.clone(), rx: into_rx };
        let token = Uuid::new_v4();
        let lobby = self.lobby.clone().unwrap_or_default();
        let (role, deck) = mine;
        match registry.tables.remove(&table_ref) {
            Some(TableState::Playing(match_id)) => {
                registry.tables.insert(table_ref, TableState::Playing(match_id));
                Err("that table's game is being played".into())
            }
            Some(TableState::Waiting(seated)) if seated.key == key => {
                // The same key twice — a second client, or a reconnect that
                // sat again: the newer seat replaces the older, which is
                // told so.
                refuse(&seated.tx, "you sat at this table again from another connection");
                registry.tables.insert(table_ref, TableState::Waiting(Seated { token, key, player_name: self.player_name.clone(), lobby: lobby.clone(), tx: out_tx.clone(), slot }));
                let _ = out_tx.send(ServerMessage::Queued { session_token: token, position: 1 });
                Ok(Playing { token, out, into, lobby })
            }
            Some(TableState::Waiting(seated)) => {
                if registry.at_cap(&self.shared.options) {
                    registry.tables.insert(table_ref, TableState::Waiting(seated));
                    return Err(AT_CAP.into());
                }
                let (match_id, seed) = registry.allocate(self.shared.base_seed);
                let me = SeatedPlayer { name: self.player_name.clone(), key: Some(key), token, slot, deck: Some(deck), lobby: Some(lobby.clone()), bot: None };
                let them = SeatedPlayer { name: seated.player_name, key: Some(seated.key), token: seated.token, slot: seated.slot, deck: Some(theirs), lobby: Some(seated.lobby), bot: None };
                let (corp, runner) = match role {
                    TableRole::Corp => (me, them),
                    TableRole::Runner => (them, me),
                };
                // A game under way is the table's answer: an offer made
                // before it lapses, as it would at a table in person.
                if let Some(tournament) = registry.tournaments.0.get_mut(&id) {
                    tournament.draw_offers.retain(|offer| offer.table != table);
                }
                start_match(&self.shared, &mut registry, match_id, seed, format, corp, runner, false, Some(table_ref));
                Ok(Playing { token, out, into, lobby })
            }
            None => {
                registry.tables.insert(table_ref, TableState::Waiting(Seated { token, key, player_name: self.player_name.clone(), lobby: lobby.clone(), tx: out_tx.clone(), slot }));
                let _ = out_tx.send(ServerMessage::Queued { session_token: token, position: 1 });
                Ok(Playing { token, out, into, lobby })
            }
        }
    }
}

fn lobby_id_as_typed(typed: &str) -> String {
    let typed = typed.trim();
    let lower = typed.to_lowercase();
    if NsgFormat::ALL.iter().any(|&format| format_lobby_id(format) == lower) { lower } else { typed.to_uppercase() }
}

/// A player lobby's id — and a tournament's: six characters from an
/// alphabet with no look-alikes (no 0/O, 1/I/L), short enough to read out
/// and never a format's name.
fn lobby_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    (0..6).map(|_| ALPHABET[rand::random_range(0..ALPHABET.len())] as char).collect()
}
