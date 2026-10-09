//! The Play Online screen's state: which page is up, what the person has
//! typed and chosen, where the connection stands, and what a press asks
//! the screen to do.
//!
//! **Three ways in, as the terminal offers them** (`netrunner_cli::tui::
//! online`): host a game on this machine and give the opponent an address
//! or a ticket; connect to a server by address or ticket; or list a
//! server's matches and watch one. Hosting and connecting both end on the
//! **Server page**, attached (Phase 4 §7 stage 4c): the server's lobbies
//! listed, one joined, a lobby made or a closed one joined by its code,
//! and — once in a lobby — a game looked for in a chair with that chair's
//! deck or decks, chosen afresh for every game. The game is played on the
//! board through the same handle every game uses
//! (`MatchHandle::start_remote`), and the board leads back here, still
//! attached, for the next one. Until this stage a connection was one game:
//! the form named a lobby and a deck up front, and a second game was a
//! second connection.
//!
//! **A game online is played in a format, and the format is the lobby.**
//! A server pairs a player only with a player in the same lobby, and the
//! decks offered are those legal in the lobby's format. There is no list
//! of waiting players to pick an opponent from (declined, 26 September
//! 2026): a lobby pairs whoever is waiting, and a closed lobby — joined by
//! its code, and its password if it has one — is how two people who know
//! each other meet.
//!
//! **No I/O here.** A press that needs the network — start hosting, dial,
//! list a server's matches, join a lobby, look for a game — is an
//! [`Outcome`] the screen carries out, and what comes back is an
//! [`Intent`] (`Attached`, `LobbyJoined`, `Queued`, `Failed`…). So every
//! rule of the form — a port that is not a number is refused before
//! anything is bound, Escape walks back one page, a failed dial reopens
//! the form it came from with the reason, a chair with no legal deck is
//! not looked for — is tested without a socket.
//!
//! **Tournaments are pages under the Server page** (Phase 4 §7 stage 6b):
//! the server's tournaments listed, one opened to its page — its code, its
//! entrants, and for a key that has one the two decks to lock in and
//! Register, or Withdraw — and one held from a form of its own by the key
//! this connection proved. The registration's statement and salt are the
//! connection driver's (`remote::Attached::register`); this form chooses
//! the decks, from those legal in the tournament's format. **The rounds
//! are the same page**: once the organizer has begun one, the standings
//! replace the entrants, each table is a line with its result, a paired
//! player sits at their table from here (a seek, under the same `seeking`
//! as a lobby's), and the organizer's buttons — the next round, a result
//! for a table nobody played, the end — are drawn for the key that holds
//! it and nobody else, though the server is what refuses anyone else.

use netrunner_client::hosting::{normalize_address, Reach, DEFAULT_PORT};
use netrunner_client::identity::{PublicKey, StandingHere};
use netrunner_client::tournament;
use netrunner_core::decks::DeckFile;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;
use netrunner_server::protocol::swiss::Outcome as TableOutcome;
use netrunner_server::protocol::{Chair, LobbyInfo, TournamentInfo, TournamentState};
use netrunner_server::MatchSummary;

/// The page up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// The three ways in.
    Home,
    Host,
    Join,
    /// A server's matches, to watch one.
    Watch,
    /// Hosting or dialling, until attached (or, watching, until a place
    /// comes).
    Waiting,
    /// Attached: the lobbies, and the game to look for.
    Server,
    /// The form for a lobby of the person's own.
    MakeLobby,
    /// Attached: the server's tournaments.
    Tournaments,
    /// One tournament: its entrants, and this key's entry.
    Tournament,
    /// The form for a tournament of this key's own.
    MakeTournament,
}

/// A line the person types into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// Join's and Watch's: a server's address or a host's ticket.
    Address,
    /// The Server page's: a closed lobby's code, and its password — also
    /// the password a listed lobby asks for.
    Lobby,
    Password,
    Port,
    /// Make a lobby's.
    LobbyName,
    LobbyPassword,
    /// Hold a tournament's.
    TournamentName,
}

impl Field {
    /// The longest text the field takes. A ticket is a few hundred
    /// characters of base32 and is pasted, never typed.
    pub fn max_len(self) -> usize {
        match self {
            Field::Address => 1024,
            Field::Lobby => 16,
            Field::Password | Field::LobbyPassword => 64,
            Field::Port => 5,
            Field::LobbyName | Field::TournamentName => 40,
        }
    }
}

/// Which chair a game is looked for in. `Either` brings a deck of each
/// side and the server picks at pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChairChoice {
    Corp,
    Runner,
    Either,
}

impl ChairChoice {
    pub const ALL: [ChairChoice; 3] = [ChairChoice::Corp, ChairChoice::Runner, ChairChoice::Either];

    pub fn label(self) -> &'static str {
        match self {
            ChairChoice::Corp => "Corp",
            ChairChoice::Runner => "Runner",
            ChairChoice::Either => "Either",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Open(Page),
    /// Escape or Back: one page out, or off the screen from Home. From the
    /// Server page it disconnects.
    Back,
    /// A field was typed into and committed.
    Typed(Field, String),
    SetReach(Reach),
    /// The format a hosted game offers, or the one a made lobby is for.
    SetFormat(NsgFormat),
    /// Whether the lobby being made is listed or joined by its code.
    SetClosed(bool),
    /// Make a lobby's: whether its games count.
    SetCasual(bool),
    /// Which side of the table a spectator sits nearer.
    SetWatchFrom(Side),
    /// Host's, Join's and Make a lobby's primary button.
    Go,
    /// List the matches at Watch's address.
    List,
    Watch(usize),
    /// The Server page: a listed lobby by its row, a closed one by the
    /// code typed, out of the lobby, the list again.
    JoinLobby(usize),
    JoinById,
    LeaveLobby,
    Refresh,
    SetChair(ChairChoice),
    SetCorpDeck(usize),
    SetRunnerDeck(usize),
    /// Look for a game in the chair chosen, and stop looking.
    Seek,
    CancelSeek,
    /// The tournaments page: one opened by its row; its page: entered
    /// with the decks chosen, or left.
    OpenTournament(usize),
    Register,
    Unregister,
    /// A tournament's rounds: a paired player's seat at their table, and
    /// the organizer's next round, recorded result and end.
    Sit,
    BeginRound,
    FinishTournament,
    RecordResult(usize, TableOutcome),
    /// Offer the opponent at this round's table an intentional draw.
    OfferDraw,
    Disconnect,
    /// The screen has started what `Go` or `Watch` asked for; this is its
    /// first line.
    Waiting(String),
    /// The waiting line changed: a reconnect, while dialling or watching.
    Status(String),
    /// What `Go`, `Watch` or `List` asked for did not work, or the
    /// connection went down for good.
    Failed(String),
    Listed(Vec<MatchSummary>),
    /// Attached, with the open lobbies — first, and again after a reattach.
    /// `hosting` says the server is this person's own.
    Attached { lobbies: Vec<LobbyInfo>, hosting: bool },
    Lobbies(Vec<LobbyInfo>),
    LobbyJoined(LobbyInfo),
    LobbyLeft,
    /// A lobby or a seek refused: the server's reason, and nothing changed.
    Refused(String),
    Queued(usize),
    SeekCancelled,
    /// The game is over and the connection is back in its lobby.
    BackInLobby(Option<LobbyInfo>),
    /// The server's answer about this connection's standing there, and
    /// the key it proved — whose a tournament entry is.
    Standing { key: Option<PublicKey>, standing: StandingHere },
    /// The server's tournaments, and one as it now stands after making,
    /// entering or leaving it. A refusal is a `Refused`.
    Tournaments(Vec<TournamentInfo>),
    Tournament(TournamentInfo),
    /// The attached connection's link: its status line while it is down,
    /// `None` when it is up again.
    Link(Option<String>),
}

/// What a press asks of the screen.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Nothing,
    Redraw,
    /// Off the screen, to the main menu.
    Leave,
    /// Start a server, and attach to it.
    Host { port: u16, reach: Reach, format: NsgFormat },
    /// Attach to a server.
    Connect { url: String },
    /// The lobby's format: the screen reads the decks legal in it and
    /// hands them to [`OnlineForm::set_decks`].
    Decks(NsgFormat),
    List { url: String },
    Watch { url: String, match_id: uuid::Uuid },
    /// Stop waiting: whatever is hosting or dialling is dropped.
    Stop,
    /// Let the connection go, and the hosted server with it.
    Disconnect,
    ListLobbies,
    JoinLobby { id: String, password: Option<String> },
    LeaveLobby,
    CreateLobby { name: String, format: NsgFormat, closed: bool, password: Option<String>, casual: bool },
    Seek(Chair),
    CancelSeek,
    ListTournaments,
    CreateTournament { name: String, format: NsgFormat },
    /// Enter the tournament with these two decks; the driver signs the
    /// commitment and keeps the salt.
    Register { tournament: String, corp: Box<DeckFile>, runner: Box<DeckFile> },
    Unregister { tournament: String },
    /// The open tournament's format: the screen reads the decks legal in
    /// it and hands them to [`OnlineForm::set_tournament_decks`].
    TournamentDecks(NsgFormat),
    Sit { tournament: String },
    BeginRound { tournament: String },
    FinishTournament { tournament: String },
    RecordResult { tournament: String, table: usize, outcome: TableOutcome },
    OfferDraw { tournament: String },
}

/// The connection, as the page draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerState {
    /// Where the connection went, for the heading.
    pub address: String,
    /// This person's own server, whose addresses the page gives out.
    pub hosting: bool,
    pub lobbies: Vec<LobbyInfo>,
    /// The lobby this connection is in.
    pub lobby: Option<LobbyInfo>,
    /// Looking for a game, at this position in the queue.
    pub seeking: Option<usize>,
    /// The decks legal in the lobby's format, built-in and saved
    /// (`netrunner_client::online::deck_choices`), both sides.
    pub decks: Vec<DeckFile>,
    pub chair: ChairChoice,
    /// Which of each side's decks (`corp_decks`, `runner_decks`).
    pub corp_deck: usize,
    pub runner_deck: usize,
    /// The link's status line while it is down.
    pub link: Option<String>,
    /// This person's standing at the server, as it last answered: asked
    /// on attaching and after every game.
    pub standing: StandingHere,
    /// The key this connection proved, as that answer named it.
    pub key: Option<PublicKey>,
    /// The server's tournaments, as last listed, and whether the list is
    /// an answer yet.
    pub tournaments: Vec<TournamentInfo>,
    pub tournaments_listed: bool,
    /// The tournament whose page is open, by id.
    pub tournament: Option<String>,
    /// The decks legal in the open tournament's format, both sides, and
    /// which of each side's this player would lock in.
    pub tournament_decks: Vec<DeckFile>,
    pub tournament_corp: usize,
    pub tournament_runner: usize,
}

impl ServerState {
    fn new(address: String, hosting: bool, lobbies: Vec<LobbyInfo>) -> Self {
        ServerState {
            address,
            hosting,
            lobbies,
            lobby: None,
            seeking: None,
            decks: Vec::new(),
            chair: ChairChoice::Corp,
            corp_deck: 0,
            runner_deck: 0,
            link: None,
            standing: StandingHere::Unasked,
            key: None,
            tournaments: Vec::new(),
            tournaments_listed: false,
            tournament: None,
            tournament_decks: Vec::new(),
            tournament_corp: 0,
            tournament_runner: 0,
        }
    }

    /// The tournament whose page is open, as the list last had it.
    pub fn open_tournament(&self) -> Option<&TournamentInfo> {
        let id = self.tournament.as_deref()?;
        self.tournaments.iter().find(|info| info.id == id)
    }

    /// Whether this key may register in the open tournament: it has a key
    /// and the tournament takes registrations.
    pub fn may_register(&self) -> bool {
        self.key.is_some() && self.open_tournament().is_some_and(|info| info.state == TournamentState::Registering)
    }

    /// Whether this key is entered in the open tournament.
    pub fn is_entered(&self) -> bool {
        self.open_tournament().is_some_and(|info| tournament::entry_of(info, self.key.as_ref()).is_some())
    }

    /// Whether this key holds the open tournament.
    pub fn is_organizer(&self) -> bool {
        self.open_tournament().is_some_and(|info| tournament::is_organizer(info, self.key.as_ref()))
    }

    /// Whether this key may drop from the open tournament: entered, in
    /// the rounds, not dropped already (`tournament::may_drop`).
    pub fn may_drop(&self) -> bool {
        self.open_tournament().is_some_and(|info| tournament::may_drop(info, self.key.as_ref()))
    }

    /// Whether this key may offer a draw at its table this round
    /// (`tournament::may_offer_draw`).
    pub fn may_offer_draw(&self) -> bool {
        self.open_tournament().is_some_and(|info| tournament::may_offer_draw(info, self.key.as_ref()))
    }

    /// The round the organizer may begin now, as its button reads: the
    /// first once two have entered, the next once every table of the
    /// current one has a result. `None` for anyone else, and between.
    pub fn next_round(&self) -> Option<String> {
        if !self.is_organizer() {
            return None;
        }
        let info = self.open_tournament()?;
        match info.state {
            TournamentState::Registering if info.entrants.len() >= 2 => Some("Begin round 1".to_string()),
            TournamentState::Registering | TournamentState::Finished => None,
            TournamentState::Playing { round } => info.current_round().filter(|current| current.complete()).map(|_| format!("Begin round {}", round + 1)),
        }
    }

    /// Whether the organizer may end the tournament now: a round is
    /// being played and complete.
    pub fn may_finish(&self) -> bool {
        self.is_organizer() && self.open_tournament().and_then(TournamentInfo::current_round).is_some_and(|current| current.complete())
    }

    /// Whether this key has a table this round whose game is still to
    /// be played, so the seat is offered.
    pub fn may_sit(&self) -> bool {
        self.open_tournament().and_then(|info| tournament::my_table(info, self.key.as_ref())).is_some_and(|(_, _, table)| table.result.is_none())
    }

    /// The tables of the current round the organizer may record a result
    /// for: those with none.
    pub fn tables_to_record(&self) -> Vec<usize> {
        if !self.is_organizer() {
            return Vec::new();
        }
        self.open_tournament()
            .and_then(TournamentInfo::current_round)
            .map(|current| current.tables.iter().enumerate().filter(|(_, table)| table.result.is_none()).map(|(index, _)| index).collect())
            .unwrap_or_default()
    }

    pub fn tournament_side_decks(&self, side: Side) -> Vec<&DeckFile> {
        self.tournament_decks.iter().filter(|deck| deck.side == side).collect()
    }

    /// The deck this player would lock in for `side`, if any is legal.
    pub fn tournament_chosen(&self, side: Side) -> Option<&DeckFile> {
        match side {
            Side::Corp => self.tournament_side_decks(Side::Corp).get(self.tournament_corp).copied(),
            Side::Runner => self.tournament_side_decks(Side::Runner).get(self.tournament_runner).copied(),
        }
    }

    /// One tournament as the server now reports it, into the list — in
    /// its place, or at the end if it is new.
    fn put_tournament(&mut self, info: TournamentInfo) {
        match self.tournaments.iter_mut().find(|listed| listed.id == info.id) {
            Some(listed) => *listed = info,
            None => self.tournaments.push(info),
        }
    }

    /// The line over Find a game: what the lobby offers and what this
    /// person stands to gain there (`identity::lobby_rating_line`).
    pub fn rating_line(&self, lobby: &LobbyInfo) -> String {
        netrunner_client::identity::lobby_rating_line(lobby.rated, self.hosting, &self.standing)
    }

    pub fn corp_decks(&self) -> Vec<&DeckFile> {
        self.decks.iter().filter(|deck| deck.side == Side::Corp).collect()
    }

    pub fn runner_decks(&self) -> Vec<&DeckFile> {
        self.decks.iter().filter(|deck| deck.side == Side::Runner).collect()
    }

    /// The deck chosen for `side`, if any is legal.
    pub fn chosen(&self, side: Side) -> Option<&DeckFile> {
        match side {
            Side::Corp => self.corp_decks().get(self.corp_deck).copied(),
            Side::Runner => self.runner_decks().get(self.runner_deck).copied(),
        }
    }

    /// The chair to look for a game in, with its deck or decks — or which
    /// side has no legal deck to bring.
    pub fn chair(&self) -> Result<Chair, Side> {
        let deck = |side: Side| self.chosen(side).cloned().map(Box::new).ok_or(side);
        Ok(match self.chair {
            ChairChoice::Corp => Chair::Corp(deck(Side::Corp)?),
            ChairChoice::Runner => Chair::Runner(deck(Side::Runner)?),
            ChairChoice::Either => Chair::Random { corp: deck(Side::Corp)?, runner: deck(Side::Runner)? },
        })
    }
}

/// The lobby being made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MakeLobby {
    pub name: String,
    pub format: NsgFormat,
    pub closed: bool,
    pub password: String,
    /// Games here count for nothing, on a server that would rate them.
    pub casual: bool,
}

/// The tournament being held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MakeTournament {
    pub name: String,
    pub format: NsgFormat,
}

#[derive(Debug, Clone)]
pub struct OnlineForm {
    pub page: Page,
    /// The page Waiting or Server was entered from, which a Stop, a
    /// disconnect or a failure reopens.
    pub came_from: Page,
    pub address: String,
    pub lobby: String,
    pub password: String,
    pub port: String,
    pub reach: Reach,
    /// The format a hosted game offers.
    pub format: NsgFormat,
    pub make: MakeLobby,
    pub make_tournament: MakeTournament,
    pub watch_from: Side,
    pub matches: Vec<MatchSummary>,
    /// Whether Watch's list is an answer from the address shown, so an
    /// empty list can say "no matches" rather than nothing.
    pub listed: bool,
    /// The waiting line while dialling or watching.
    pub status: String,
    pub notice: Option<String>,
    /// The connection, once attached — and while attaching, `None`.
    pub server: Option<ServerState>,
}

impl OnlineForm {
    /// `address` is where Join and Watch start: the last one used, or the
    /// terminal's default server. `format` is the one Settings names,
    /// where Host and Make a lobby start.
    pub fn new(address: String, format: NsgFormat) -> Self {
        OnlineForm {
            page: Page::Home,
            came_from: Page::Home,
            address,
            lobby: String::new(),
            password: String::new(),
            port: DEFAULT_PORT.to_string(),
            reach: Reach::Network,
            format,
            make: MakeLobby { name: String::new(), format, closed: false, password: String::new(), casual: false },
            make_tournament: MakeTournament { name: String::new(), format },
            watch_from: Side::Corp,
            matches: Vec::new(),
            listed: false,
            status: String::new(),
            notice: None,
            server: None,
        }
    }

    /// Back on the screen after a game or a visit elsewhere: everything
    /// typed kept, and the Server page up if the connection is still
    /// attached (`attached`), Home otherwise.
    pub fn reopen(&mut self, attached: bool) {
        self.notice = None;
        if attached && self.server.is_some() {
            // Back from a tournament game, its page: the result is there
            // and the next round will be.
            let on_tournament = self.page == Page::Tournament && self.server.as_ref().is_some_and(|server| server.tournament.is_some());
            if !on_tournament {
                self.page = Page::Server;
            }
        } else {
            self.server = None;
            self.page = Page::Home;
        }
    }

    /// The decks legal in the lobby's format, read afresh: each side's
    /// chosen deck stays chosen if it is still among them, and the first
    /// otherwise.
    pub fn set_decks(&mut self, decks: Vec<DeckFile>) {
        let Some(server) = &mut self.server else { return };
        let keep = |chosen: Option<&DeckFile>, side: Side| {
            let id = chosen.map(|deck| deck.id.clone());
            let among: Vec<&DeckFile> = decks.iter().filter(|deck| deck.side == side).collect();
            id.and_then(|id| among.iter().position(|deck| deck.id == id)).unwrap_or(0)
        };
        let (corp, runner) = (keep(server.chosen(Side::Corp), Side::Corp), keep(server.chosen(Side::Runner), Side::Runner));
        server.decks = decks;
        server.corp_deck = corp;
        server.runner_deck = runner;
    }

    /// The decks legal in the open tournament's format, read afresh: each
    /// side's chosen deck stays chosen if it is still among them.
    pub fn set_tournament_decks(&mut self, decks: Vec<DeckFile>) {
        let Some(server) = &mut self.server else { return };
        let keep = |chosen: Option<&DeckFile>, side: Side| {
            let id = chosen.map(|deck| deck.id.clone());
            let among: Vec<&DeckFile> = decks.iter().filter(|deck| deck.side == side).collect();
            id.and_then(|id| among.iter().position(|deck| deck.id == id)).unwrap_or(0)
        };
        let (corp, runner) = (keep(server.tournament_chosen(Side::Corp), Side::Corp), keep(server.tournament_chosen(Side::Runner), Side::Runner));
        server.tournament_decks = decks;
        server.tournament_corp = corp;
        server.tournament_runner = runner;
    }

    pub fn apply(&mut self, intent: Intent) -> Outcome {
        match intent {
            Intent::Open(Page::MakeLobby) => {
                self.notice = None;
                self.page = Page::MakeLobby;
                // A lobby is made for the format of the one the person is
                // in, or of the server's first, since that is the game
                // they came to play.
                if let Some(server) = &self.server {
                    self.make.format = server.lobby.as_ref().or(server.lobbies.first()).map_or(self.make.format, |lobby| lobby.format);
                }
                Outcome::Redraw
            }
            // The list is asked for each time the page opens: a tournament
            // moves while nobody is looking.
            Intent::Open(Page::Tournaments) => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                self.notice = None;
                self.page = Page::Tournaments;
                server.tournaments_listed = false;
                Outcome::ListTournaments
            }
            Intent::Open(Page::MakeTournament) => {
                let Some(server) = &self.server else { return Outcome::Nothing };
                if server.key.is_none() {
                    self.notice = Some("A tournament is held by a key, and this client has none: connect with one first".to_string());
                    return Outcome::Redraw;
                }
                self.notice = None;
                self.page = Page::MakeTournament;
                self.make_tournament.format = server.lobby.as_ref().map_or(self.make_tournament.format, |lobby| lobby.format);
                Outcome::Redraw
            }
            Intent::Open(page) => {
                self.notice = None;
                self.page = page;
                Outcome::Redraw
            }
            Intent::Back => match self.page {
                Page::Home => Outcome::Leave,
                Page::Waiting => {
                    self.page = self.came_from;
                    self.notice = None;
                    Outcome::Stop
                }
                Page::Server => self.disconnect(),
                Page::MakeLobby | Page::Tournaments => {
                    self.page = Page::Server;
                    self.notice = None;
                    Outcome::Redraw
                }
                Page::Tournament | Page::MakeTournament => {
                    self.page = Page::Tournaments;
                    self.notice = None;
                    Outcome::Redraw
                }
                Page::Host | Page::Join | Page::Watch => {
                    self.page = Page::Home;
                    self.notice = None;
                    Outcome::Redraw
                }
            },
            Intent::Typed(field, text) => {
                let text = text.trim().to_string();
                match field {
                    Field::Address => {
                        if text != self.address {
                            self.listed = false;
                            self.matches.clear();
                        }
                        self.address = text;
                    }
                    Field::Lobby => self.lobby = text,
                    Field::Password => self.password = text,
                    // Digits only: a letter in a port is a typo, not a
                    // port, and is dropped as the terminal's field drops it.
                    Field::Port => self.port = text.chars().filter(char::is_ascii_digit).collect(),
                    Field::LobbyName => self.make.name = text,
                    Field::LobbyPassword => self.make.password = text,
                    Field::TournamentName => self.make_tournament.name = text,
                }
                Outcome::Redraw
            }
            Intent::SetReach(reach) => {
                self.reach = reach;
                Outcome::Redraw
            }
            Intent::SetFormat(format) => {
                match self.page {
                    Page::MakeLobby => self.make.format = format,
                    Page::MakeTournament => self.make_tournament.format = format,
                    _ => self.format = format,
                }
                Outcome::Redraw
            }
            Intent::SetClosed(closed) => {
                self.make.closed = closed;
                Outcome::Redraw
            }
            Intent::SetCasual(casual) => {
                self.make.casual = casual;
                Outcome::Redraw
            }
            Intent::SetWatchFrom(side) => {
                self.watch_from = side;
                Outcome::Redraw
            }
            Intent::Go => self.go(),
            Intent::List if self.page == Page::Watch => {
                if self.address.is_empty() {
                    self.notice = Some("Type a server's address or a host's ticket first".to_string());
                    return Outcome::Redraw;
                }
                self.notice = None;
                Outcome::List { url: normalize_address(&self.address) }
            }
            Intent::List => Outcome::Nothing,
            Intent::Watch(index) => match self.matches.get(index) {
                Some(summary) if self.page == Page::Watch => {
                    let match_id = summary.match_id;
                    Outcome::Watch { url: normalize_address(&self.address), match_id }
                }
                _ => Outcome::Nothing,
            },
            Intent::JoinLobby(index) => {
                let Some(server) = &self.server else { return Outcome::Nothing };
                let Some(lobby) = server.lobbies.get(index) else { return Outcome::Nothing };
                if server.lobby.as_ref().is_some_and(|current| current.id == lobby.id) {
                    return Outcome::Nothing;
                }
                self.notice = None;
                let password = lobby.password.then(|| self.password.clone()).filter(|password| !password.is_empty());
                Outcome::JoinLobby { id: lobby.id.clone(), password }
            }
            Intent::JoinById => {
                if self.server.is_none() {
                    return Outcome::Nothing;
                }
                if self.lobby.is_empty() {
                    self.notice = Some("Type the lobby's code first".to_string());
                    return Outcome::Redraw;
                }
                self.notice = None;
                Outcome::JoinLobby { id: self.lobby.clone(), password: Some(self.password.clone()).filter(|password| !password.is_empty()) }
            }
            Intent::LeaveLobby => match &self.server {
                Some(server) if server.lobby.is_some() && server.seeking.is_none() => Outcome::LeaveLobby,
                _ => Outcome::Nothing,
            },
            Intent::Refresh => {
                if self.server.is_none() {
                    return Outcome::Nothing;
                }
                match self.page {
                    Page::Tournaments | Page::Tournament | Page::MakeTournament => Outcome::ListTournaments,
                    _ => Outcome::ListLobbies,
                }
            }
            Intent::SetChair(chair) => match &mut self.server {
                Some(server) if server.seeking.is_none() => {
                    server.chair = chair;
                    Outcome::Redraw
                }
                _ => Outcome::Nothing,
            },
            // On a tournament's page the deck drop-downs are the entry's.
            Intent::SetCorpDeck(index) if self.page == Page::Tournament => match &mut self.server {
                Some(server) if index < server.tournament_side_decks(Side::Corp).len() => {
                    server.tournament_corp = index;
                    Outcome::Redraw
                }
                _ => Outcome::Nothing,
            },
            Intent::SetRunnerDeck(index) if self.page == Page::Tournament => match &mut self.server {
                Some(server) if index < server.tournament_side_decks(Side::Runner).len() => {
                    server.tournament_runner = index;
                    Outcome::Redraw
                }
                _ => Outcome::Nothing,
            },
            Intent::SetCorpDeck(index) => match &mut self.server {
                Some(server) if index < server.corp_decks().len() => {
                    server.corp_deck = index;
                    Outcome::Redraw
                }
                _ => Outcome::Nothing,
            },
            Intent::SetRunnerDeck(index) => match &mut self.server {
                Some(server) if index < server.runner_decks().len() => {
                    server.runner_deck = index;
                    Outcome::Redraw
                }
                _ => Outcome::Nothing,
            },
            Intent::Seek => {
                let Some(server) = &self.server else { return Outcome::Nothing };
                if server.seeking.is_some() {
                    return Outcome::Nothing;
                }
                let Some(lobby) = &server.lobby else {
                    self.notice = Some("Join a lobby first".to_string());
                    return Outcome::Redraw;
                };
                match server.chair() {
                    Ok(chair) => {
                        self.notice = None;
                        Outcome::Seek(chair)
                    }
                    Err(side) => {
                        self.notice = Some(format!("No {side:?} deck is legal in {:?}: build one, or choose another lobby", lobby.format));
                        Outcome::Redraw
                    }
                }
            }
            Intent::CancelSeek => match &self.server {
                Some(server) if server.seeking.is_some() => Outcome::CancelSeek,
                _ => Outcome::Nothing,
            },
            Intent::OpenTournament(index) => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                let Some(info) = server.tournaments.get(index) else { return Outcome::Nothing };
                let format = info.format;
                server.tournament = Some(info.id.clone());
                self.page = Page::Tournament;
                self.notice = None;
                Outcome::TournamentDecks(format)
            }
            Intent::Register => {
                let Some(server) = &self.server else { return Outcome::Nothing };
                let Some(info) = server.open_tournament().filter(|_| self.page == Page::Tournament) else { return Outcome::Nothing };
                if !server.may_register() {
                    return Outcome::Nothing;
                }
                let (tournament, format) = (info.id.clone(), info.format);
                match (server.tournament_chosen(Side::Corp), server.tournament_chosen(Side::Runner)) {
                    (Some(corp), Some(runner)) => {
                        let (corp, runner) = (Box::new(corp.clone()), Box::new(runner.clone()));
                        self.notice = None;
                        Outcome::Register { tournament, corp, runner }
                    }
                    (None, _) => {
                        self.notice = Some(format!("No Corp deck is legal in {:?}: build one under Decks", format));
                        Outcome::Redraw
                    }
                    (_, None) => {
                        self.notice = Some(format!("No Runner deck is legal in {:?}: build one under Decks", format));
                        Outcome::Redraw
                    }
                }
            }
            // Withdraw while registering, drop in the rounds: one message.
            Intent::Unregister => match &self.server {
                Some(server) if self.page == Page::Tournament && ((server.is_entered() && server.may_register()) || server.may_drop()) => {
                    self.notice = None;
                    Outcome::Unregister { tournament: server.tournament.clone().expect("entered in the open tournament") }
                }
                _ => Outcome::Nothing,
            },
            Intent::Sit => match &self.server {
                Some(server) if self.page == Page::Tournament && server.may_sit() && server.seeking.is_none() => {
                    self.notice = None;
                    Outcome::Sit { tournament: server.tournament.clone().expect("a table in the open tournament") }
                }
                _ => Outcome::Nothing,
            },
            Intent::OfferDraw => match &self.server {
                Some(server) if self.page == Page::Tournament && server.may_offer_draw() => {
                    self.notice = None;
                    Outcome::OfferDraw { tournament: server.tournament.clone().expect("a table in the open tournament") }
                }
                _ => Outcome::Nothing,
            },
            Intent::BeginRound => match &self.server {
                Some(server) if self.page == Page::Tournament && server.next_round().is_some() => {
                    self.notice = None;
                    Outcome::BeginRound { tournament: server.tournament.clone().expect("the open tournament") }
                }
                _ => Outcome::Nothing,
            },
            Intent::FinishTournament => match &self.server {
                Some(server) if self.page == Page::Tournament && server.may_finish() => {
                    self.notice = None;
                    Outcome::FinishTournament { tournament: server.tournament.clone().expect("the open tournament") }
                }
                _ => Outcome::Nothing,
            },
            Intent::RecordResult(table, outcome) => match &self.server {
                Some(server) if self.page == Page::Tournament && server.tables_to_record().contains(&table) => {
                    self.notice = None;
                    Outcome::RecordResult { tournament: server.tournament.clone().expect("the open tournament"), table, outcome }
                }
                _ => Outcome::Nothing,
            },
            Intent::Disconnect => self.disconnect(),
            Intent::Waiting(status) => {
                if self.page != Page::Waiting {
                    self.came_from = self.page;
                }
                self.page = Page::Waiting;
                self.status = status;
                self.notice = None;
                Outcome::Redraw
            }
            Intent::Status(status) => {
                self.status = status;
                Outcome::Redraw
            }
            Intent::Failed(reason) => {
                if matches!(self.page, Page::Waiting | Page::Server | Page::MakeLobby | Page::Tournaments | Page::Tournament | Page::MakeTournament) {
                    self.page = self.came_from;
                }
                self.server = None;
                self.notice = Some(reason);
                Outcome::Redraw
            }
            Intent::Listed(matches) => {
                self.notice = matches.is_empty().then(|| format!("{} is hosting no matches right now", normalize_address(&self.address)));
                self.matches = matches;
                self.listed = true;
                Outcome::Redraw
            }
            Intent::Attached { lobbies, hosting } => {
                let address = if hosting { "this machine".to_string() } else { normalize_address(&self.address) };
                match &mut self.server {
                    // A reattach: the list is fresh, the rest stands.
                    Some(server) => server.lobbies = lobbies,
                    None => self.server = Some(ServerState::new(address, hosting, lobbies)),
                }
                if self.page == Page::Waiting {
                    self.page = Page::Server;
                }
                self.notice = None;
                // A hosted server has one lobby, the format the host chose:
                // the host is put in it, so the game is a chair and a deck
                // away.
                let server = self.server.as_ref().expect("just set");
                match (hosting, server.lobby.as_ref(), server.lobbies.first()) {
                    (true, None, Some(only)) => Outcome::JoinLobby { id: only.id.clone(), password: None },
                    _ => Outcome::Redraw,
                }
            }
            Intent::Lobbies(lobbies) => {
                if let Some(server) = &mut self.server {
                    server.lobbies = lobbies;
                }
                Outcome::Redraw
            }
            Intent::LobbyJoined(lobby) => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                let format = lobby.format;
                // The list predates the join: the row is brought level
                // with what the server just said, so it counts this
                // person.
                if let Some(row) = server.lobbies.iter_mut().find(|row| row.id == lobby.id) {
                    *row = lobby.clone();
                }
                server.lobby = Some(lobby);
                self.notice = None;
                if self.page == Page::MakeLobby {
                    self.page = Page::Server;
                }
                Outcome::Decks(format)
            }
            Intent::LobbyLeft => {
                if let Some(server) = &mut self.server {
                    server.lobby = None;
                    server.seeking = None;
                }
                Outcome::Redraw
            }
            Intent::Refused(reason) => {
                self.notice = Some(reason);
                Outcome::Redraw
            }
            Intent::Queued(position) => {
                if let Some(server) = &mut self.server {
                    server.seeking = Some(position);
                }
                Outcome::Redraw
            }
            Intent::SeekCancelled => {
                if let Some(server) = &mut self.server {
                    server.seeking = None;
                }
                Outcome::Redraw
            }
            Intent::BackInLobby(lobby) => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                server.seeking = None;
                if lobby.is_some() {
                    server.lobby = lobby;
                }
                Outcome::Redraw
            }
            Intent::Link(line) => {
                if let Some(server) = &mut self.server {
                    server.link = line;
                }
                Outcome::Redraw
            }
            Intent::Standing { key, standing } => {
                if let Some(server) = &mut self.server {
                    server.key = key;
                    server.standing = standing;
                }
                Outcome::Redraw
            }
            Intent::Tournaments(tournaments) => {
                if let Some(server) = &mut self.server {
                    server.tournaments = tournaments;
                    server.tournaments_listed = true;
                }
                Outcome::Redraw
            }
            // After making one, its page; after entering or leaving the
            // open one, the page brought level.
            Intent::Tournament(info) => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                let format = info.format;
                let id = info.id.clone();
                // The form's answer is a tournament the list has never
                // seen; a push about another, while the form is up, is
                // put in the list and leaves the form alone.
                let new = !server.tournaments.iter().any(|listed| listed.id == id);
                server.put_tournament(info);
                self.notice = None;
                if self.page == Page::MakeTournament && new {
                    server.tournament = Some(id);
                    self.page = Page::Tournament;
                    return Outcome::TournamentDecks(format);
                }
                Outcome::Redraw
            }
        }
    }

    fn disconnect(&mut self) -> Outcome {
        self.server = None;
        self.page = self.came_from;
        self.notice = None;
        Outcome::Disconnect
    }

    fn go(&mut self) -> Outcome {
        match self.page {
            Page::Host => match self.port.parse::<u16>() {
                Ok(port) => Outcome::Host { port, reach: self.reach, format: self.format },
                Err(_) => {
                    self.notice = Some(format!("{:?} is not a port number", self.port));
                    Outcome::Redraw
                }
            },
            Page::Join if self.address.is_empty() => {
                self.notice = Some("Type the server's address, or paste a host's ticket".to_string());
                Outcome::Redraw
            }
            Page::Join => Outcome::Connect { url: normalize_address(&self.address) },
            Page::MakeLobby => {
                if self.server.is_none() {
                    return Outcome::Nothing;
                }
                if self.make.name.is_empty() {
                    self.notice = Some("Give the lobby a name".to_string());
                    return Outcome::Redraw;
                }
                self.notice = None;
                let password = Some(self.make.password.clone()).filter(|password| !password.is_empty());
                Outcome::CreateLobby { name: self.make.name.clone(), format: self.make.format, closed: self.make.closed, password, casual: self.make.casual }
            }
            Page::MakeTournament => {
                if self.server.is_none() {
                    return Outcome::Nothing;
                }
                if self.make_tournament.name.is_empty() {
                    self.notice = Some("Give the tournament a name".to_string());
                    return Outcome::Redraw;
                }
                self.notice = None;
                Outcome::CreateTournament { name: self.make_tournament.name.clone(), format: self.make_tournament.format }
            }
            Page::Home | Page::Watch | Page::Waiting | Page::Server | Page::Tournaments | Page::Tournament => Outcome::Nothing,
        }
    }
}

/// A lobby as a line names it: a player's lobby by its name and format,
/// the server's own by the format alone, since that is its name.
pub fn lobby_title(lobby: &LobbyInfo) -> String {
    let format = netrunner_client::settings::format_name(lobby.format);
    let format = format.chars().next().map_or(String::new(), |first| first.to_uppercase().chain(format.chars().skip(1)).collect());
    if lobby.permanent { format } else { format!("{} · {format}", lobby.name) }
}

/// Who a hosted game is for, in a pill's few words; the whole sentence
/// (`Reach::label`) goes under the pills.
pub fn reach_pill(reach: Reach) -> &'static str {
    match reach {
        Reach::ThisMachine => "This machine",
        Reach::Network => "My network",
        Reach::Internet => "The internet",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(id: &str) -> DeckFile {
        netrunner_core::decks::by_id(id).expect("a built-in deck")
    }

    fn form() -> OnlineForm {
        OnlineForm::new("ws://127.0.0.1:8080".to_string(), NsgFormat::Startup)
    }

    fn lobby(id: &str, format: NsgFormat) -> LobbyInfo {
        LobbyInfo { id: id.into(), name: id.into(), format, permanent: true, closed: false, password: false, rated: true, players: 0, seeking: 0 }
    }

    /// Connected to a server with two lobbies, as a joiner.
    fn attached() -> OnlineForm {
        let mut form = form();
        form.apply(Intent::Open(Page::Join));
        assert_eq!(form.apply(Intent::Go), Outcome::Connect { url: "ws://127.0.0.1:8080".to_string() });
        form.apply(Intent::Waiting("Connecting…".to_string()));
        assert_eq!(form.apply(Intent::Attached { lobbies: vec![lobby("startup", NsgFormat::Startup), lobby("standard", NsgFormat::Standard)], hosting: false }), Outcome::Redraw);
        assert_eq!(form.page, Page::Server);
        form
    }

    #[test]
    fn escape_walks_back_one_page_and_then_off_the_screen() {
        let mut form = form();
        form.apply(Intent::Open(Page::Join));
        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert_eq!(form.page, Page::Home);
        assert_eq!(form.apply(Intent::Back), Outcome::Leave);
    }

    /// A port is digits, and one that is not a port is refused before
    /// anything is bound. Hosting asks for no deck: the deck is chosen
    /// when the game is looked for.
    #[test]
    fn a_port_is_digits_and_a_bad_one_is_refused_at_the_form() {
        let mut form = form();
        form.apply(Intent::Open(Page::Host));
        form.apply(Intent::Typed(Field::Port, "9x9".to_string()));
        assert_eq!(form.port, "99");
        form.apply(Intent::Typed(Field::Port, "99999".to_string()));
        assert_eq!(form.apply(Intent::Go), Outcome::Redraw);
        assert!(form.notice.as_deref().is_some_and(|notice| notice.contains("not a port")));
        form.apply(Intent::Typed(Field::Port, "0".to_string()));
        form.apply(Intent::SetReach(Reach::Internet));
        form.apply(Intent::SetFormat(NsgFormat::Standard));
        assert_eq!(form.apply(Intent::Go), Outcome::Host { port: 0, reach: Reach::Internet, format: NsgFormat::Standard });
    }

    /// Join dials what was typed, an address given its scheme and port
    /// (a ticket is passed as it is: `hosting::normalize_address`, tested
    /// there), and asks for no lobby and no deck: those are the Server
    /// page's.
    #[test]
    fn join_dials_the_address_and_nothing_more() {
        let mut form = form();
        form.apply(Intent::Open(Page::Join));
        form.apply(Intent::Typed(Field::Address, " 192.168.1.5 ".to_string()));
        assert_eq!(form.apply(Intent::Go), Outcome::Connect { url: "ws://192.168.1.5:8080".to_string() });
        form.apply(Intent::Typed(Field::Address, String::new()));
        assert_eq!(form.apply(Intent::Go), Outcome::Redraw, "nothing to dial");
    }

    /// Waiting remembers the form it came from: Stop and a failure both
    /// reopen it, the failure with its reason.
    #[test]
    fn waiting_goes_back_to_the_form_it_came_from() {
        let mut form = form();
        form.apply(Intent::Open(Page::Join));
        form.apply(Intent::Waiting("Connecting…".to_string()));
        assert_eq!(form.page, Page::Waiting);
        form.apply(Intent::Status("Reconnecting".to_string()));
        assert_eq!(form.apply(Intent::Back), Outcome::Stop);
        assert_eq!(form.page, Page::Join);
        form.apply(Intent::Waiting("Connecting…".to_string()));
        form.apply(Intent::Failed("refused".to_string()));
        assert_eq!((form.page, form.notice.as_deref()), (Page::Join, Some("refused")));
    }

    /// Watching lists the address's matches, says so when there are none,
    /// and forgets a list when the address changes.
    #[test]
    fn a_list_belongs_to_its_address() {
        let mut form = form();
        form.apply(Intent::Open(Page::Watch));
        assert_eq!(form.apply(Intent::List), Outcome::List { url: "ws://127.0.0.1:8080".to_string() });
        form.apply(Intent::Listed(Vec::new()));
        assert!(form.listed && form.notice.as_deref().is_some_and(|notice| notice.contains("no matches")));
        assert_eq!(form.apply(Intent::Watch(0)), Outcome::Nothing, "nothing to watch");
        form.apply(Intent::Typed(Field::Address, "elsewhere".to_string()));
        assert!(!form.listed);
    }

    /// Attached, the Server page lists the lobbies; one is joined by its
    /// row, which asks for the decks legal in its format; a closed one by
    /// the code typed, with the password; the one the person is in is not
    /// joined again.
    #[test]
    fn the_server_page_joins_a_lobby_by_row_or_by_code() {
        let mut form = attached();
        assert_eq!(form.server.as_ref().unwrap().lobbies.len(), 2);
        assert_eq!(form.apply(Intent::JoinLobby(1)), Outcome::JoinLobby { id: "standard".into(), password: None });
        assert_eq!(form.apply(Intent::LobbyJoined(LobbyInfo { players: 1, ..lobby("standard", NsgFormat::Standard) })), Outcome::Decks(NsgFormat::Standard));
        assert_eq!(form.server.as_ref().unwrap().lobbies[1].players, 1, "the row counts the person who just joined");
        assert_eq!(form.apply(Intent::JoinLobby(1)), Outcome::Nothing, "already in it");
        assert_eq!(form.apply(Intent::JoinById), Outcome::Redraw, "no code typed");
        form.apply(Intent::Typed(Field::Lobby, "k7m2qx".to_string()));
        form.apply(Intent::Typed(Field::Password, "swordfish".to_string()));
        assert_eq!(form.apply(Intent::JoinById), Outcome::JoinLobby { id: "k7m2qx".into(), password: Some("swordfish".into()) });
        form.apply(Intent::Refused("that is not lobby K7M2QX's password".to_string()));
        assert_eq!(form.server.as_ref().unwrap().lobby.as_ref().map(|lobby| lobby.id.as_str()), Some("standard"), "a refusal changes nothing");
        assert!(form.notice.is_some());
        assert_eq!(form.apply(Intent::Refresh), Outcome::ListLobbies);
        form.apply(Intent::Lobbies(vec![lobby("startup", NsgFormat::Startup)]));
        assert_eq!(form.server.as_ref().unwrap().lobbies.len(), 1);
    }

    /// A listed lobby that asks for a password is joined with the one
    /// typed in the password field.
    #[test]
    fn a_listed_lobby_with_a_password_takes_the_one_typed() {
        let mut form = attached();
        let guarded = LobbyInfo { password: true, ..lobby("K7M2QX", NsgFormat::Startup) };
        form.apply(Intent::Lobbies(vec![guarded]));
        assert_eq!(form.apply(Intent::JoinLobby(0)), Outcome::JoinLobby { id: "K7M2QX".into(), password: None });
        form.apply(Intent::Typed(Field::Password, "swordfish".to_string()));
        assert_eq!(form.apply(Intent::JoinLobby(0)), Outcome::JoinLobby { id: "K7M2QX".into(), password: Some("swordfish".into()) });
    }

    /// In a lobby, a game is looked for in a chair with that chair's deck
    /// — both decks for Either — from the decks legal in the lobby's
    /// format; a side with no legal deck is said rather than sent; while
    /// looking the chair is fixed and a cancel is offered; the end of the
    /// game puts the person back in the lobby, not looking.
    #[test]
    fn a_game_is_looked_for_in_a_chair_with_its_decks() {
        let mut form = attached();
        assert_eq!(form.apply(Intent::Seek), Outcome::Redraw, "no lobby yet");
        assert!(form.notice.as_deref().is_some_and(|notice| notice.contains("Join a lobby")));
        form.apply(Intent::JoinLobby(0));
        form.apply(Intent::LobbyJoined(lobby("startup", NsgFormat::Startup)));
        form.set_decks(vec![deck("brick_stack"), deck("glyph_of_warding"), deck("stolen_goods")]);
        let Outcome::Seek(Chair::Corp(corp)) = form.apply(Intent::Seek) else { panic!("the Corp chair with its deck") };
        assert_eq!(corp.id, "brick_stack");
        form.apply(Intent::SetCorpDeck(1));
        form.apply(Intent::SetChair(ChairChoice::Either));
        let Outcome::Seek(Chair::Random { corp, runner }) = form.apply(Intent::Seek) else { panic!("either chair, both decks") };
        assert_eq!((corp.id.as_str(), runner.id.as_str()), ("glyph_of_warding", "stolen_goods"));
        form.apply(Intent::Queued(1));
        assert_eq!(form.server.as_ref().unwrap().seeking, Some(1));
        assert_eq!(form.apply(Intent::Seek), Outcome::Nothing, "one seek at a time");
        assert_eq!(form.apply(Intent::SetChair(ChairChoice::Runner)), Outcome::Nothing, "the chair is fixed while looking");
        assert_eq!(form.apply(Intent::LeaveLobby), Outcome::Nothing, "and so is the lobby");
        assert_eq!(form.apply(Intent::CancelSeek), Outcome::CancelSeek);
        form.apply(Intent::SeekCancelled);
        assert_eq!(form.server.as_ref().unwrap().seeking, None);
        form.apply(Intent::SetChair(ChairChoice::Runner));
        form.set_decks(vec![deck("brick_stack")]);
        assert_eq!(form.apply(Intent::Seek), Outcome::Redraw);
        assert!(form.notice.as_deref().is_some_and(|notice| notice.contains("No Runner deck")));
        form.apply(Intent::Queued(2));
        form.apply(Intent::BackInLobby(Some(LobbyInfo { players: 2, ..lobby("startup", NsgFormat::Startup) })));
        let server = form.server.as_ref().unwrap();
        assert_eq!((server.seeking, server.lobby.as_ref().map(|lobby| lobby.players)), (None, Some(2)));
        assert_eq!(form.apply(Intent::LeaveLobby), Outcome::LeaveLobby);
        form.apply(Intent::LobbyLeft);
        assert!(form.server.as_ref().unwrap().lobby.is_none());
    }

    /// The decks read afresh keep each side's choice by id.
    #[test]
    fn a_deck_chosen_before_is_chosen_again_when_the_list_is_read_afresh() {
        let mut form = attached();
        form.apply(Intent::LobbyJoined(lobby("startup", NsgFormat::Startup)));
        form.set_decks(vec![deck("brick_stack"), deck("glyph_of_warding"), deck("stolen_goods"), deck("dashing_mad")]);
        form.apply(Intent::SetCorpDeck(1));
        form.apply(Intent::SetRunnerDeck(1));
        form.set_decks(vec![deck("fine_print"), deck("glyph_of_warding"), deck("dashing_mad")]);
        let server = form.server.as_ref().unwrap();
        assert_eq!((server.chosen(Side::Corp).unwrap().id.as_str(), server.chosen(Side::Runner).unwrap().id.as_str()), ("glyph_of_warding", "dashing_mad"));
        form.set_decks(vec![deck("brick_stack")]);
        let server = form.server.as_ref().unwrap();
        assert_eq!((server.corp_deck, server.runner_deck), (0, 0), "a deck no longer legal falls back to the first");
    }

    /// A lobby is made from its own page, for the format of the lobby the
    /// person is in, listed or closed, with or without a password; the
    /// server's answer lands the person in it.
    #[test]
    fn a_lobby_is_made_from_its_page() {
        let mut form = attached();
        form.apply(Intent::JoinLobby(1));
        form.apply(Intent::LobbyJoined(lobby("standard", NsgFormat::Standard)));
        form.apply(Intent::Open(Page::MakeLobby));
        assert_eq!((form.page, form.make.format), (Page::MakeLobby, NsgFormat::Standard));
        assert_eq!(form.apply(Intent::Go), Outcome::Redraw, "a lobby needs a name");
        form.apply(Intent::Typed(Field::LobbyName, " Friday ".to_string()));
        form.apply(Intent::SetFormat(NsgFormat::Startup));
        form.apply(Intent::SetClosed(true));
        form.apply(Intent::Typed(Field::LobbyPassword, "swordfish".to_string()));
        form.apply(Intent::SetCasual(true));
        assert_eq!(form.apply(Intent::Go), Outcome::CreateLobby { name: "Friday".into(), format: NsgFormat::Startup, closed: true, password: Some("swordfish".into()), casual: true });
        let made = LobbyInfo { closed: true, password: true, permanent: false, rated: false, ..lobby("K7M2QX", NsgFormat::Startup) };
        assert_eq!(form.apply(Intent::LobbyJoined(made.clone())), Outcome::Decks(NsgFormat::Startup));
        assert_eq!((form.page, form.server.as_ref().unwrap().lobby.as_ref()), (Page::Server, Some(&made)));
        form.apply(Intent::Open(Page::MakeLobby));
        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert_eq!(form.page, Page::Server);
    }

    /// The host's own server has one lobby, which the host is put in; a
    /// disconnect lets the connection go and reopens the form the person
    /// came from; a connection down for good does the same with the reason.
    #[test]
    fn a_host_is_put_in_the_one_lobby_and_a_disconnect_reopens_the_form() {
        let mut form = form();
        form.apply(Intent::Open(Page::Host));
        form.apply(Intent::Typed(Field::Port, "0".to_string()));
        assert!(matches!(form.apply(Intent::Go), Outcome::Host { .. }));
        form.apply(Intent::Waiting("Hosting…".to_string()));
        assert_eq!(form.apply(Intent::Attached { lobbies: vec![lobby("startup", NsgFormat::Startup)], hosting: true }), Outcome::JoinLobby { id: "startup".into(), password: None });
        assert!(form.server.as_ref().is_some_and(|server| server.hosting && server.address == "this machine"));
        form.apply(Intent::LobbyJoined(lobby("startup", NsgFormat::Startup)));
        assert_eq!(form.apply(Intent::Attached { lobbies: vec![lobby("startup", NsgFormat::Startup)], hosting: true }), Outcome::Redraw, "a reattach lists afresh and joins nothing: the machine rejoins by itself");
        form.apply(Intent::Link(Some("Connection lost".to_string())));
        assert!(form.server.as_ref().unwrap().link.is_some());
        form.apply(Intent::Link(None));
        assert!(form.server.as_ref().unwrap().link.is_none());
        assert_eq!(form.apply(Intent::Back), Outcome::Disconnect);
        assert_eq!((form.page, form.server.is_none()), (Page::Host, true));

        let mut form = attached();
        form.apply(Intent::Failed("could not reconnect within 60s".to_string()));
        assert_eq!((form.page, form.server.is_none(), form.notice.is_some()), (Page::Join, true, true));
    }

    /// The line over Find a game says what the lobby offers and, once
    /// the server has answered, what this person stands to gain there —
    /// a casual lobby says so whatever the standing, and the host's own
    /// server never rates (Phase 4 §7 stage 5).
    #[test]
    fn the_lobby_says_whether_a_game_there_counts() {
        let mut guest = attached();
        guest.apply(Intent::JoinLobby(0));
        guest.apply(Intent::LobbyJoined(lobby("startup", NsgFormat::Startup)));
        let line = |form: &OnlineForm| {
            let server = form.server.as_ref().unwrap();
            server.rating_line(server.lobby.as_ref().unwrap())
        };
        assert_eq!(line(&guest), "Rated here.", "before the server has answered");
        guest.apply(Intent::Standing { key: None, standing: StandingHere::Unrated });
        assert_eq!(line(&guest), "Rated here. No rated games here yet.");
        guest.apply(Intent::Standing { key: None, standing: StandingHere::NoKey });
        assert_eq!(line(&guest), "Unrated for you: this client has no key, so nothing here counts.");
        guest.apply(Intent::LobbyJoined(LobbyInfo { rated: false, permanent: false, name: "Friday".into(), ..lobby("K7M2QX", NsgFormat::Startup) }));
        assert_eq!(line(&guest), "Unrated: nothing in this lobby is rated.");

        let mut host = form();
        host.apply(Intent::Open(Page::Host));
        host.apply(Intent::Typed(Field::Port, "0".to_string()));
        host.apply(Intent::Go);
        host.apply(Intent::Attached { lobbies: vec![LobbyInfo { rated: false, ..lobby("startup", NsgFormat::Startup) }], hosting: true });
        host.apply(Intent::LobbyJoined(LobbyInfo { rated: false, ..lobby("startup", NsgFormat::Startup) }));
        assert_eq!(line(&host), "Unrated: a game hosted from this machine is never rated.");
    }

    #[test]
    fn a_servers_own_lobby_is_named_by_its_format_alone() {
        assert_eq!(lobby_title(&lobby("startup", NsgFormat::Startup)), "Startup");
        assert_eq!(lobby_title(&LobbyInfo { name: "Friday".into(), permanent: false, ..lobby("K7M2QX", NsgFormat::Standard) }), "Friday · Standard");
    }

    fn key(byte: u8) -> PublicKey {
        netrunner_identity::Identity::from_secret([byte; 32]).public_key()
    }

    fn tournament_info(id: &str, organizer: PublicKey, entrants: Vec<netrunner_server::protocol::Entrant>) -> TournamentInfo {
        TournamentInfo { id: id.into(), name: "Friday".into(), format: NsgFormat::Startup, organizer, state: TournamentState::Registering, entrants, seeding: Vec::new(), rounds: Vec::new(), dropped: Vec::new(), draw_offers: Vec::new() }
    }

    fn entrant(byte: u8) -> netrunner_server::protocol::Entrant {
        let identity = netrunner_identity::Identity::from_secret([byte; 32]);
        netrunner_server::protocol::Entrant { name: format!("p{byte}"), key: identity.public_key(), corp_hash: "c".into(), runner_hash: "r".into(), commitment: identity.sign(b"t", "{}".into()) }
    }

    /// Opening the tournaments page asks the server for the list; a row
    /// opens its tournament and asks for the decks legal in its format;
    /// Register sends the two chosen, and a side with no legal deck is
    /// said rather than sent; the server's answer brings the page level;
    /// Withdraw is offered to an entrant and sent; Back walks the pages.
    #[test]
    fn a_tournament_is_opened_entered_and_left_from_its_page() {
        let mut form = attached();
        form.apply(Intent::Standing { key: Some(key(2)), standing: StandingHere::Unrated });
        assert_eq!(form.apply(Intent::Open(Page::Tournaments)), Outcome::ListTournaments);
        assert_eq!(form.page, Page::Tournaments);
        assert!(!form.server.as_ref().unwrap().tournaments_listed, "not an answer yet");
        form.apply(Intent::Tournaments(vec![tournament_info("K7M2QX", key(1), vec![])]));
        assert!(form.server.as_ref().unwrap().tournaments_listed);
        assert_eq!(form.apply(Intent::OpenTournament(1)), Outcome::Nothing, "no such row");
        assert_eq!(form.apply(Intent::OpenTournament(0)), Outcome::TournamentDecks(NsgFormat::Startup));
        assert_eq!(form.page, Page::Tournament);
        assert_eq!(form.apply(Intent::Refresh), Outcome::ListTournaments, "Refresh here lists the tournaments, not the lobbies");

        form.set_tournament_decks(vec![deck("brick_stack")]);
        assert_eq!(form.apply(Intent::Register), Outcome::Redraw, "no Runner deck legal");
        assert!(form.notice.as_deref().is_some_and(|notice| notice.contains("No Runner deck")));
        form.set_tournament_decks(vec![deck("brick_stack"), deck("glyph_of_warding"), deck("stolen_goods")]);
        assert_eq!(form.apply(Intent::SetCorpDeck(1)), Outcome::Redraw, "the tournament's own choice, not the lobby's");
        assert_eq!(form.server.as_ref().unwrap().corp_deck, 0);
        let Outcome::Register { tournament, corp, runner } = form.apply(Intent::Register) else { panic!("the two decks chosen") };
        assert_eq!((tournament.as_str(), corp.id.as_str(), runner.id.as_str()), ("K7M2QX", "glyph_of_warding", "stolen_goods"));
        assert_eq!(form.apply(Intent::Unregister), Outcome::Nothing, "not entered yet");

        let entered = tournament_info("K7M2QX", key(1), vec![entrant(2)]);
        assert_eq!(form.apply(Intent::Tournament(entered.clone())), Outcome::Redraw);
        let server = form.server.as_ref().unwrap();
        assert!(server.is_entered() && server.may_register());
        assert_eq!(server.open_tournament(), Some(&entered), "the list learnt it too");
        assert_eq!(form.apply(Intent::Unregister), Outcome::Unregister { tournament: "K7M2QX".into() });
        form.apply(Intent::Refused("registration has closed".to_string()));
        assert!(form.notice.is_some() && form.page == Page::Tournament, "a refusal changes nothing");

        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert_eq!(form.page, Page::Tournaments);
        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert_eq!(form.page, Page::Server);
        assert_eq!(form.apply(Intent::Register), Outcome::Nothing, "only from the page");
    }

    /// A key that has none cannot register or hold one; a tournament that
    /// has closed takes no entry; the drop-downs read the tournament's
    /// decks afresh by id.
    #[test]
    fn a_keyless_client_and_a_closed_tournament_offer_no_entry() {
        let mut form = attached();
        form.apply(Intent::Standing { key: None, standing: StandingHere::NoKey });
        form.apply(Intent::Open(Page::Tournaments));
        form.apply(Intent::Tournaments(vec![tournament_info("K7M2QX", key(1), vec![])]));
        assert_eq!(form.apply(Intent::Open(Page::MakeTournament)), Outcome::Redraw);
        assert!(form.page == Page::Tournaments && form.notice.as_deref().is_some_and(|notice| notice.contains("no")));
        form.apply(Intent::OpenTournament(0));
        form.set_tournament_decks(vec![deck("brick_stack"), deck("stolen_goods")]);
        assert!(!form.server.as_ref().unwrap().may_register());
        assert_eq!(form.apply(Intent::Register), Outcome::Nothing);

        form.apply(Intent::Standing { key: Some(key(2)), standing: StandingHere::Unrated });
        assert!(form.server.as_ref().unwrap().may_register());
        form.set_tournament_decks(vec![deck("brick_stack"), deck("glyph_of_warding"), deck("stolen_goods"), deck("dashing_mad")]);
        form.apply(Intent::SetCorpDeck(1));
        form.apply(Intent::SetRunnerDeck(1));
        form.set_tournament_decks(vec![deck("fine_print"), deck("glyph_of_warding"), deck("dashing_mad")]);
        let server = form.server.as_ref().unwrap();
        assert_eq!((server.tournament_chosen(Side::Corp).unwrap().id.as_str(), server.tournament_chosen(Side::Runner).unwrap().id.as_str()), ("glyph_of_warding", "dashing_mad"));
    }

    /// A tournament is held from its form — a name, the format of the
    /// lobby the person is in — and the server's answer opens its page,
    /// asking for its format's decks.
    #[test]
    fn a_tournament_is_held_from_its_form_and_its_page_opens() {
        let mut form = attached();
        form.apply(Intent::Standing { key: Some(key(1)), standing: StandingHere::Unrated });
        form.apply(Intent::JoinLobby(1));
        form.apply(Intent::LobbyJoined(lobby("standard", NsgFormat::Standard)));
        form.apply(Intent::Open(Page::Tournaments));
        assert_eq!(form.apply(Intent::Open(Page::MakeTournament)), Outcome::Redraw);
        assert_eq!((form.page, form.make_tournament.format), (Page::MakeTournament, NsgFormat::Standard));
        assert_eq!(form.apply(Intent::Go), Outcome::Redraw, "a tournament needs a name");
        form.apply(Intent::Typed(Field::TournamentName, " Friday ".to_string()));
        form.apply(Intent::SetFormat(NsgFormat::Startup));
        assert_eq!(form.format, NsgFormat::Startup, "Host's format is untouched");
        assert_eq!(form.apply(Intent::Go), Outcome::CreateTournament { name: "Friday".into(), format: NsgFormat::Startup });
        let made = tournament_info("K7M2QX", key(1), vec![]);
        assert_eq!(form.apply(Intent::Tournament(made.clone())), Outcome::TournamentDecks(NsgFormat::Startup));
        let server = form.server.as_ref().unwrap();
        assert_eq!((form.page, server.open_tournament()), (Page::Tournament, Some(&made)));
        assert!(!server.is_entered() && server.may_register());
        form.apply(Intent::Back);
        form.apply(Intent::Open(Page::MakeTournament));
        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert_eq!(form.page, Page::Tournaments);
    }

    /// A tournament in its rounds: a paired key is offered its seat and the
    /// bye is not; a seat taken is a seek, stood up from; the organizer
    /// alone is offered the next round once every table has a result, a
    /// result for a table nobody played, and the end; back from the game,
    /// the page is the tournament's.
    #[test]
    fn a_round_is_sat_at_by_the_paired_and_run_by_the_organizer() {
        use netrunner_server::protocol::swiss::{Round, Table};
        let (ann, bo, cy) = (key(1), key(2), key(3));
        let mut playing = tournament_info("K7M2QX", ann, vec![entrant(1), entrant(2), entrant(3)]);
        playing.seeding = vec![bo, ann, cy];
        playing.rounds = vec![Round { tables: vec![Table { corp: ann, runner: bo, result: None }], bye: Some(cy) }];
        playing.state = TournamentState::Playing { round: 1 };

        // bo, paired: the seat, and nothing of the organizer's.
        let mut form = attached();
        form.apply(Intent::Standing { key: Some(bo), standing: StandingHere::Unrated });
        form.apply(Intent::Open(Page::Tournaments));
        form.apply(Intent::Tournaments(vec![playing.clone()]));
        form.apply(Intent::OpenTournament(0));
        let server = form.server.as_ref().unwrap();
        assert!(server.may_sit() && !server.may_register() && server.next_round().is_none() && !server.may_finish() && server.tables_to_record().is_empty());
        assert_eq!(form.apply(Intent::BeginRound), Outcome::Nothing);
        assert_eq!(form.apply(Intent::Sit), Outcome::Sit { tournament: "K7M2QX".into() });
        form.apply(Intent::Queued(1));
        assert_eq!(form.apply(Intent::Sit), Outcome::Nothing, "seated already");
        assert_eq!(form.apply(Intent::CancelSeek), Outcome::CancelSeek);
        form.apply(Intent::SeekCancelled);
        form.reopen(true);
        assert_eq!(form.page, Page::Tournament, "back from the game, the tournament's page");
        // bo may offer a draw once; with the offer standing, not again.
        assert_eq!(form.apply(Intent::OfferDraw), Outcome::OfferDraw { tournament: "K7M2QX".into() });
        let mut offered = playing.clone();
        offered.draw_offers = vec![netrunner_server::protocol::DrawOffer { table: 0, by: bo }];
        form.apply(Intent::Tournament(offered));
        assert_eq!(form.apply(Intent::OfferDraw), Outcome::Nothing, "offered already");
        assert!(form.server.as_ref().unwrap().may_sit(), "an offer is not a seat");
        // bo may drop; dropped (and forfeit), bo has no seat and nothing
        // more to drop from.
        assert!(form.server.as_ref().unwrap().may_drop());
        assert_eq!(form.apply(Intent::Unregister), Outcome::Unregister { tournament: "K7M2QX".into() });
        let mut left = playing.clone();
        left.dropped = vec![bo];
        left.rounds[0].tables[0].result = Some(TableOutcome::CorpWon);
        form.apply(Intent::Tournament(left));
        let server = form.server.as_ref().unwrap();
        assert!(!server.may_drop() && !server.may_sit() && server.is_entered());
        assert_eq!(form.apply(Intent::Unregister), Outcome::Nothing, "dropped already");

        // cy, the bye: no seat.
        let mut bye = attached();
        bye.apply(Intent::Standing { key: Some(cy), standing: StandingHere::Unrated });
        bye.apply(Intent::Open(Page::Tournaments));
        bye.apply(Intent::Tournaments(vec![playing.clone()]));
        bye.apply(Intent::OpenTournament(0));
        assert_eq!(bye.apply(Intent::Sit), Outcome::Nothing);
        assert_eq!(bye.apply(Intent::OfferDraw), Outcome::Nothing, "no table, no draw");

        // ann, the organizer and a player: the seat, and once the table
        // has its result the next round or the end; a result recorded
        // only for a table that has none.
        let mut organizer = attached();
        organizer.apply(Intent::Standing { key: Some(ann), standing: StandingHere::Unrated });
        organizer.apply(Intent::Open(Page::Tournaments));
        organizer.apply(Intent::Tournaments(vec![playing.clone()]));
        organizer.apply(Intent::OpenTournament(0));
        assert!(organizer.server.as_ref().unwrap().may_sit());
        assert_eq!(organizer.server.as_ref().unwrap().next_round(), None, "a table is open");
        assert_eq!(organizer.server.as_ref().unwrap().tables_to_record(), vec![0]);
        assert_eq!(organizer.apply(Intent::RecordResult(1, TableOutcome::Tie)), Outcome::Nothing, "no such table");
        assert_eq!(organizer.apply(Intent::RecordResult(0, TableOutcome::Tie)), Outcome::RecordResult { tournament: "K7M2QX".into(), table: 0, outcome: TableOutcome::Tie });
        let mut done = playing.clone();
        done.rounds[0].tables[0].result = Some(TableOutcome::Tie);
        assert_eq!(organizer.apply(Intent::Tournament(done.clone())), Outcome::Redraw, "pushed, not the form's");
        let server = organizer.server.as_ref().unwrap();
        assert!(!server.may_sit() && server.tables_to_record().is_empty() && server.may_finish());
        assert_eq!(server.next_round().as_deref(), Some("Begin round 2"));
        assert_eq!(organizer.apply(Intent::BeginRound), Outcome::BeginRound { tournament: "K7M2QX".into() });
        assert_eq!(organizer.apply(Intent::FinishTournament), Outcome::FinishTournament { tournament: "K7M2QX".into() });
        let mut finished = done.clone();
        finished.state = TournamentState::Finished;
        organizer.apply(Intent::Tournament(finished));
        let server = organizer.server.as_ref().unwrap();
        assert!(server.next_round().is_none() && !server.may_finish() && !server.may_sit() && !server.may_register());

        // Before any round, two entrants let the organizer begin.
        let mut fresh = attached();
        fresh.apply(Intent::Standing { key: Some(ann), standing: StandingHere::Unrated });
        fresh.apply(Intent::Open(Page::Tournaments));
        fresh.apply(Intent::Tournaments(vec![tournament_info("K7M2QX", ann, vec![entrant(1)])]));
        fresh.apply(Intent::OpenTournament(0));
        assert_eq!(fresh.server.as_ref().unwrap().next_round(), None, "one entrant pairs nobody");
        fresh.apply(Intent::Tournament(tournament_info("K7M2QX", ann, vec![entrant(1), entrant(2)])));
        assert_eq!(fresh.server.as_ref().unwrap().next_round().as_deref(), Some("Begin round 1"));
        // A push about another tournament while the form is up leaves the
        // form alone.
        fresh.apply(Intent::Open(Page::MakeTournament));
        assert_eq!(fresh.apply(Intent::Tournament(tournament_info("K7M2QX", ann, vec![entrant(1), entrant(2), entrant(3)]))), Outcome::Redraw);
        assert_eq!(fresh.page, Page::MakeTournament);
    }

    /// Back from the board the Server page is up, still attached; after a
    /// game watched, or once the connection has gone, Home is.
    #[test]
    fn reopening_returns_to_the_server_page_while_attached() {
        let mut form = attached();
        form.reopen(true);
        assert_eq!(form.page, Page::Server);
        form.reopen(false);
        assert_eq!((form.page, form.server.is_none()), (Page::Home, true));
    }
}
