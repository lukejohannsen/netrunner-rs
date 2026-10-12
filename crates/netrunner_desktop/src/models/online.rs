//! The Play Online screen's state: which page is up, what the person has
//! typed and chosen, where the connection stands, and what a press asks
//! the screen to do.
//!
//! **Play Online is the Server Dashboard** (Phase 7 §14, decided 12
//! October 2026 over the person's wireframes): the screen connects to the
//! Default server and opens on it — the two **active decks** (Phase 7
//! §16) with the formats each is legal in, the three doors, **Ranked
//! ladder**, **Casual / unranked** and **Tournaments & events**, and a
//! column of ways elsewhere. A door opens the **queue form** over the
//! dashboard, not a page: the format as pills, greyed with the reason
//! when an active deck is not legal there (`online::queue_problem`), and
//! the role preference, Prefer Corp / Any / Prefer Runner; **Find match**
//! joins that queue and looks for a game with both decks, and the
//! **search** replaces the form — what is looked for, the time waited,
//! Abort — until the board. The game is played through the same handle
//! every game uses (`MatchHandle::start_remote`), and the board leads back
//! to the dashboard, still attached, for the next one. With nothing to
//! connect to the screen opens on the person's servers instead.
//!
//! **What went with the dashboard**: the list of lobbies as the way in,
//! the chair as a choice (a seek is always `Chair::Either` with the two
//! active decks and a soft preference, so the queue never deadlocks on
//! two Corps), a deck chosen per lobby (the active decks are the decks),
//! and a bot in the server's own queues — **a bot sits only in a friend
//! lobby that allows one**, asked for on that lobby's page. Hosting and
//! Watch are rows of the dashboard's column rather than a Home of their
//! own.
//!
//! **A game online is played in a format, and the format is the queue.**
//! A server pairs a player only with a player in the same lobby. There is
//! no list of waiting players to pick an opponent from (declined, 26
//! September 2026): a queue pairs whoever is waiting, and a friend lobby —
//! made under Casual, or joined there by its code and its password if it
//! has one — is how people who know each other meet (Phase 4 §9).
//!
//! **No I/O here.** A press that needs the network — start hosting, dial,
//! list a server's matches, join a lobby, look for a game — is an
//! [`Outcome`] the screen carries out, and what comes back is an
//! [`Intent`] (`Attached`, `LobbyJoined`, `Queued`, `Failed`…). So every
//! rule of the form — a port that is not a number is refused before
//! anything is bound, Escape closes the form before it leaves the screen,
//! a failed dial reopens the page it came from with the reason, a format
//! an active deck is not legal in is greyed and never sent — is tested
//! without a socket.
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
use std::collections::BTreeMap;
use std::time::Instant;

use netrunner_client::identity::{ProfileHere, PublicKey};
use netrunner_client::servers::{ServerBook, ServerEdit};
use netrunner_client::online::{queue_problem, ActiveDeck};
use netrunner_client::tournament;
use netrunner_core::decks::DeckFile;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;
use netrunner_server::protocol::swiss::Outcome as TableOutcome;
use netrunner_server::protocol::{queue_id, Chair, LadderRow, Level, LobbyInfo, Opponent, Profile, QueueMode, TournamentInfo, TournamentState};
use netrunner_server::MatchSummary;

/// The page up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Attached: the Server Dashboard — the active decks, the three doors
    /// and the ways elsewhere; the queue form and the search are overlays
    /// on it (`ServerState::queue`, `ServerState::searching`).
    Dashboard,
    Host,
    /// The person's servers (Phase 7 §14): each by name, the Default, a
    /// Connect or Disconnect, what it holds of the person; and an address
    /// or a ticket to connect to once.
    Servers,
    /// Adding a server, or editing one.
    EditServer,
    /// A server's matches, to watch one.
    Watch,
    /// Hosting or dialling, until attached (or, watching, until a place
    /// comes).
    Waiting,
    /// The form for a friend lobby of the person's own.
    MakeLobby,
    /// In a friend lobby: who is there, a game looked for among them, and
    /// a bot where the maker let them in.
    FriendLobby,
    /// Attached: the server's tournaments.
    Tournaments,
    /// One tournament: its entrants, and this key's entry.
    Tournament,
    /// The form for a tournament of this key's own.
    MakeTournament,
    /// Attached: what this server keeps of the person — the standing per
    /// format, the titles and badges earned and the ones worn, chosen
    /// here — and its ladder (Phase 4 §10).
    Profile,
}

/// A line the person types into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// Join's and Watch's: a server's address or a host's ticket.
    Address,
    /// The Casual form's: a friend lobby's code, and its password.
    Lobby,
    Password,
    Port,
    /// Make a lobby's.
    LobbyName,
    LobbyPassword,
    /// Hold a tournament's.
    TournamentName,
    /// Add or edit a server's.
    ServerName,
    ServerAddress,
}

impl Field {
    /// The longest text the field takes. A ticket is a few hundred
    /// characters of base32 and is pasted, never typed.
    pub fn max_len(self) -> usize {
        match self {
            Field::Address | Field::ServerAddress => 1024,
            Field::Lobby => 16,
            Field::Password | Field::LobbyPassword => 64,
            Field::Port => 5,
            Field::LobbyName | Field::TournamentName => 40,
            Field::ServerName => netrunner_client::servers::MAX_NAME,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Open(Page),
    /// Escape or Back: the queue form closed, a search aborted, a friend
    /// lobby left, one page out — or off the screen from the dashboard
    /// and from the servers with nothing attached.
    Back,
    /// A field was typed into and committed.
    Typed(Field, String),
    SetReach(Reach),
    /// The format a hosted game offers, or the one a made lobby is for.
    SetFormat(NsgFormat),
    /// Make a friend lobby's: whether its members may ask for a bot.
    SetLobbyBots(bool),
    /// Which side of the table a spectator sits nearer.
    SetWatchFrom(Side),
    /// Host's, Join's and Make a lobby's primary button.
    Go,
    /// List the matches at Watch's address.
    List,
    Watch(usize),
    /// The dashboard's doors: the queue form for a mode opened over it,
    /// its format chosen, closed; and Find match, which joins the queue
    /// and looks for a game with both active decks.
    OpenQueue(QueueMode),
    SetQueueFormat(NsgFormat),
    CloseQueue,
    FindMatch,
    /// The side preferred, if any (Phase 4 §9): on the queue form and in
    /// a friend lobby alike.
    SetPrefer(Option<Side>),
    /// The Casual form's: a friend lobby joined by the code typed; the
    /// friend lobby's page: out of it, and the list of lobbies again.
    JoinById,
    LeaveLobby,
    Refresh,
    /// A friend lobby's: whom to look for a game against — a person, or a
    /// bot at a rung the lobby offers.
    SetOpponent(Option<Level>),
    /// A tournament's entry decks (`Page::Tournament`).
    SetCorpDeck(usize),
    SetRunnerDeck(usize),
    /// A friend lobby's Find a game, and Abort anywhere a game is looked
    /// for.
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
    Profile { key: Option<PublicKey>, profile: ProfileHere },
    /// The server's tournaments, and one as it now stands after making,
    /// entering or leaving it. A refusal is a `Refused`.
    Tournaments(Vec<TournamentInfo>),
    Tournament(TournamentInfo),
    /// The attached connection's link: its status line while it is down,
    /// `None` when it is up again.
    Link(Option<String>),
    /// The person's servers as the file now has them: on opening the
    /// screen and after every change.
    Servers(ServerBook),
    /// What this machine knows of the person: the key, the record.
    Local(Vec<String>),
    /// A server's answer about the person, fetched for its row.
    RowProfile { address: String, answer: Result<Option<Profile>, String> },
    /// The servers page's: a row connected to, made or unmade the
    /// Default, opened to edit (or a new one), removed.
    ConnectRow(usize),
    ToggleDefault(usize),
    EditRow(Option<usize>),
    RemoveRow(usize),
    SaveServer,
    /// On opening the screen with a Default and nothing attached.
    ConnectDefault,
    /// The active decks, read afresh by the screen (on entry, and after
    /// the picker chose one).
    ActiveDecks(Box<[Option<ActiveDeck>; 2]>),
    /// The profile page's: the pop-up of the titles or the badges earned
    /// opened, closed, and one picked from it by its place in the
    /// profile's list (or none) — worn at once; a format's ladder.
    Choose(AwardKind),
    CloseChooser,
    Pick(Option<usize>),
    ShowLadder(NsgFormat),
    Ladder { format: NsgFormat, rows: Vec<LadderRow> },
}

/// Which of a profile's awards the chooser lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwardKind {
    Title,
    Badge,
}

/// A row's answer about the person, as the servers page draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowProfile {
    Asking,
    Lines(Vec<String>),
    Failed(String),
}

/// The server being added (`original` none) or edited.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditServer {
    pub original: Option<String>,
    pub name: String,
    pub address: String,
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
    List { url: String },
    Watch { url: String, match_id: uuid::Uuid },
    /// Stop waiting: whatever is hosting or dialling is dropped.
    Stop,
    /// Let the connection go, and the hosted server with it.
    Disconnect,
    ListLobbies,
    JoinLobby { id: String, password: Option<String> },
    LeaveLobby,
    CreateLobby { name: String, format: NsgFormat, password: Option<String>, bots: bool },
    Seek(Chair, Opponent),
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
    /// With the table's seed commitment as the page read it (Phase 4 §7
    /// stage 6c).
    Sit { tournament: String, seed_commitment: String },
    BeginRound { tournament: String },
    FinishTournament { tournament: String },
    RecordResult { tournament: String, table: usize, outcome: TableOutcome },
    OfferDraw { tournament: String },
    /// Change the person's servers; the screen writes the file and hands
    /// the list back as `Intent::Servers`.
    EditServers(ServerEdit),
    /// Ask these servers what they keep of the person, each on its own
    /// socket, the answers coming back as `Intent::RowProfile`.
    FetchProfiles(Vec<String>),
    /// Wear these, of what was earned.
    SetProfile { title: Option<String>, badge: Option<String> },
    Ladder(NsgFormat),
}

/// The connection, as the page draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerState {
    /// Where the connection went, for the heading.
    pub address: String,
    /// This person's own server, whose addresses the page gives out.
    pub hosting: bool,
    pub lobbies: Vec<LobbyInfo>,
    /// The lobby this connection is in: a queue while a game is looked
    /// for there, or a friend lobby.
    pub lobby: Option<LobbyInfo>,
    /// The queue form, while a door is open over the dashboard.
    pub queue: Option<QueueForm>,
    /// Find match pressed: the queue is being joined, and the seek goes
    /// the moment the server says the person is in it. Kept so a refusal
    /// reopens the form.
    pub pending: Option<QueueForm>,
    /// A game being looked for.
    pub searching: Option<Searching>,
    /// The side preferred, if any.
    pub prefer: Option<Side>,
    /// In a friend lobby: a bot to play rather than whoever is there, at
    /// a rung the lobby offers; `None` is a person.
    pub bot: Option<Level>,
    /// The link's status line while it is down.
    pub link: Option<String>,
    /// This person's standing at the server, as it last answered: asked
    /// on attaching and after every game.
    pub profile: ProfileHere,
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
    /// The profile page's pop-up, while it is open: the titles or the
    /// badges earned, to scroll through and pick one from.
    pub choosing: Option<AwardKind>,
    /// The ladder shown, once answered.
    pub ladder_format: NsgFormat,
    pub ladder: Option<Vec<LadderRow>>,
}

impl ServerState {
    fn new(address: String, hosting: bool, lobbies: Vec<LobbyInfo>) -> Self {
        ServerState {
            address,
            hosting,
            lobbies,
            lobby: None,
            queue: None,
            pending: None,
            searching: None,
            prefer: None,
            bot: None,
            link: None,
            profile: ProfileHere::Unasked,
            key: None,
            tournaments: Vec::new(),
            tournaments_listed: false,
            tournament: None,
            tournament_decks: Vec::new(),
            tournament_corp: 0,
            tournament_runner: 0,
            choosing: None,
            ladder_format: netrunner_client::settings::DEFAULT_FORMAT,
            ladder: None,
        }
    }

    /// The profile this server answered with, if it keeps one.
    pub fn known_profile(&self) -> Option<&Profile> {
        match &self.profile {
            ProfileHere::Known(profile) => Some(profile),
            _ => None,
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

    /// The line under the queue form's title: what the queue offers and
    /// what this person stands to gain there (`identity::lobby_rating_line`).
    pub fn rating_line(&self, lobby: &LobbyInfo) -> String {
        netrunner_client::identity::lobby_rating_line(lobby.rated, self.hosting, lobby.format, &self.profile)
    }

    /// Whether the server lists a queue of this mode at all.
    pub fn offers(&self, mode: QueueMode) -> bool {
        self.lobbies.iter().any(|lobby| lobby.mode == mode)
    }

    /// The formats the server's queues of `mode` are in, as listed.
    pub fn queue_formats(&self, mode: QueueMode) -> Vec<NsgFormat> {
        self.lobbies.iter().filter(|lobby| lobby.mode == mode).map(|lobby| lobby.format).collect()
    }

    /// The server's queue of `mode` in `format`, as listed.
    pub fn queue(&self, mode: QueueMode, format: NsgFormat) -> Option<&LobbyInfo> {
        self.lobbies.iter().find(|lobby| lobby.mode == mode && lobby.format == format)
    }

    /// Why the Ranked door is shut, when it is: a hosted game is never
    /// rated, and a daemon without a data directory keeps no book.
    pub fn ranked_shut(&self) -> Option<&'static str> {
        if self.hosting {
            Some("A game hosted from this machine is never rated")
        } else if !self.offers(QueueMode::Ranked) {
            Some("This server keeps no ratings")
        } else {
            None
        }
    }

    /// Whether a game is being looked for: in a queue, a friend lobby or
    /// at a tournament table.
    pub fn seeking(&self) -> bool {
        self.searching.is_some()
    }

    /// The friend lobby this connection is in, if it is in one.
    pub fn friend_lobby(&self) -> Option<&LobbyInfo> {
        self.lobby.as_ref().filter(|lobby| !lobby.permanent())
    }

    /// Whom a friend lobby's seek is against: the bot chosen if the lobby
    /// offers its rung, else a person.
    pub fn opponent(&self) -> Opponent {
        match (self.bot, &self.lobby) {
            (Some(level), Some(lobby)) if lobby.bots.contains(&level) => Opponent::Bot { level, style: None },
            _ => Opponent::Person,
        }
    }
}

/// The queue form open over the dashboard: the door pressed, and the
/// format chosen on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueForm {
    pub mode: QueueMode,
    pub format: NsgFormat,
}

/// A game being looked for, as the search over the dashboard draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct Searching {
    /// Where: a queue or a friend lobby; `None` at a tournament table.
    pub lobby: Option<LobbyInfo>,
    pub prefer: Option<Side>,
    pub opponent: Opponent,
    /// The place in the queue, once the server has said.
    pub position: Option<usize>,
    /// When the seek was sent, for the time waited.
    pub since: Instant,
}

impl Searching {
    fn new(lobby: Option<LobbyInfo>, prefer: Option<Side>, opponent: Opponent) -> Self {
        Searching { lobby, prefer, opponent, position: None, since: Instant::now() }
    }

    /// Whether this search is in one of the server's own queues, which
    /// is left once the search is over: the dashboard is not a queue.
    pub fn in_a_queue(&self) -> bool {
        self.lobby.as_ref().is_some_and(LobbyInfo::permanent)
    }
}

/// The friend lobby being made: casual, and joined by its code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MakeLobby {
    pub name: String,
    pub format: NsgFormat,
    pub password: String,
    /// Its members may ask for a bot.
    pub bots: bool,
}

/// The tournament being held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MakeTournament {
    pub name: String,
    pub format: NsgFormat,
}

#[derive(Debug, Clone)]
pub struct OnlineForm {
    /// The active deck a side (`Settings::active_deck`, Phase 7 §16),
    /// Corp then Runner: the decks every seek brings, drawn on the
    /// dashboard with their legality, and what a tournament's entry
    /// starts on. `None` where the id names no deck.
    pub active: [Option<ActiveDeck>; 2],
    pub page: Page,
    /// The page Waiting or the dashboard was entered from, which a Stop,
    /// a disconnect or a failure reopens.
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
    /// The person's servers, as the file last had them.
    pub book: ServerBook,
    /// Each row's answer about the person, by address.
    pub row_profiles: BTreeMap<String, RowProfile>,
    /// What this machine knows of the person, for the servers page.
    pub local: Vec<String>,
    pub edit: EditServer,
}

impl OnlineForm {
    /// `address` is where Connect once and Watch start: the last one
    /// used, or the terminal's default server. `format` is the one
    /// Settings names, where Host, Make a lobby and the queue form start.
    /// The page is the servers until something is attached.
    pub fn new(address: String, format: NsgFormat) -> Self {
        OnlineForm {
            active: [None, None],
            page: Page::Servers,
            came_from: Page::Servers,
            address,
            lobby: String::new(),
            password: String::new(),
            port: DEFAULT_PORT.to_string(),
            reach: Reach::Network,
            format,
            make: MakeLobby { name: String::new(), format, password: String::new(), bots: false },
            make_tournament: MakeTournament { name: String::new(), format },
            watch_from: Side::Corp,
            matches: Vec::new(),
            listed: false,
            status: String::new(),
            notice: None,
            server: None,
            book: ServerBook::default(),
            row_profiles: BTreeMap::new(),
            local: Vec::new(),
            edit: EditServer::default(),
        }
    }

    /// What the person calls the server at `address`: its name in their
    /// list, else the address.
    pub fn server_name(&self, address: &str) -> String {
        self.book.find(address).map_or_else(|| address.to_string(), |server| server.name.clone())
    }

    /// The address of the server attached to, when it is one of the
    /// person's (not their own, hosted).
    pub fn attached_address(&self) -> Option<&str> {
        self.server.as_ref().filter(|server| !server.hosting).map(|server| server.address.as_str())
    }

    /// Back on the screen after a game or a visit elsewhere: everything
    /// typed kept, and the dashboard up if the connection is still
    /// attached (`attached`), the servers otherwise.
    pub fn reopen(&mut self, attached: bool) {
        self.notice = None;
        if attached && self.server.is_some() {
            // Back from a tournament game, its page: the result is there
            // and the next round will be. Back from a friend lobby's game,
            // the lobby.
            let on_tournament = self.page == Page::Tournament && self.server.as_ref().is_some_and(|server| server.tournament.is_some());
            let in_friend_lobby = self.page == Page::FriendLobby && self.server.as_ref().is_some_and(|server| server.friend_lobby().is_some());
            if !on_tournament && !in_friend_lobby {
                self.page = Page::Dashboard;
            }
        } else {
            self.server = None;
            self.page = Page::Servers;
        }
    }

    /// The active decks (`Intent::ActiveDecks`), in a chain.
    pub fn with_active(mut self, active: [Option<ActiveDeck>; 2]) -> Self {
        self.active = active;
        self
    }

    pub fn active(&self, side: Side) -> Option<&ActiveDeck> {
        self.active[side as usize].as_ref()
    }

    /// The format pills of the open queue form: each format the server's
    /// queues of that mode are in, with why it cannot be chosen when an
    /// active deck is not legal there.
    pub fn queue_choices(&self) -> Vec<(NsgFormat, Option<String>)> {
        let Some((server, queue)) = self.server.as_ref().and_then(|server| Some((server, server.queue?))) else { return Vec::new() };
        server.queue_formats(queue.mode).into_iter().map(|format| (format, queue_problem(format, self.active(Side::Corp), self.active(Side::Runner)))).collect()
    }

    /// The format a door opens on: the one chosen last if the queues of
    /// this mode include it and both active decks are legal there, else
    /// the first they are, else the first listed.
    fn starting_format(&self, mode: QueueMode) -> Option<NsgFormat> {
        let server = self.server.as_ref()?;
        let formats = server.queue_formats(mode);
        let legal = |format: &NsgFormat| queue_problem(*format, self.active(Side::Corp), self.active(Side::Runner)).is_none();
        formats.iter().find(|format| **format == self.format && legal(format)).or_else(|| formats.iter().find(|format| legal(format))).or(formats.first()).copied()
    }

    /// The chair every seek takes: Either, with both active decks and the
    /// side preferred — or why not, when a deck is missing or not legal
    /// in `format`.
    fn chair(&self, format: NsgFormat) -> Result<Chair, String> {
        if let Some(problem) = queue_problem(format, self.active(Side::Corp), self.active(Side::Runner)) {
            return Err(problem);
        }
        let deck = |side: Side| Box::new(self.active(side).expect("checked above").deck.clone());
        let prefer = self.server.as_ref().and_then(|server| server.prefer);
        Ok(Chair::Either { corp: deck(Side::Corp), runner: deck(Side::Runner), prefer })
    }

    /// The decks legal in the open tournament's format, read afresh: each
    /// side's chosen deck stays chosen if it is still among them, else the
    /// active deck for the side if it is.
    pub fn set_tournament_decks(&mut self, decks: Vec<DeckFile>) {
        let preferred = [self.active(Side::Corp).map(|deck| deck.deck.id.clone()), self.active(Side::Runner).map(|deck| deck.deck.id.clone())];
        let Some(server) = &mut self.server else { return };
        let keep = |chosen: Option<&DeckFile>, side: Side| {
            let among: Vec<&DeckFile> = decks.iter().filter(|deck| deck.side == side).collect();
            let position = |id: &str| among.iter().position(|deck| deck.id == id);
            chosen.and_then(|deck| position(&deck.id)).or_else(|| preferred[side as usize].as_deref().and_then(position)).unwrap_or(0)
        };
        let (corp, runner) = (keep(server.tournament_chosen(Side::Corp), Side::Corp), keep(server.tournament_chosen(Side::Runner), Side::Runner));
        server.tournament_decks = decks;
        server.tournament_corp = corp;
        server.tournament_runner = runner;
    }

    pub fn apply(&mut self, intent: Intent) -> Outcome {
        match intent {
            Intent::Open(Page::MakeLobby) => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                self.notice = None;
                self.page = Page::MakeLobby;
                // A lobby is made for the format on the form it was opened
                // from, else of the lobby the person is in, else of the
                // server's first, since that is the game they came to play.
                self.make.format = server.queue.map(|queue| queue.format).or_else(|| server.lobby.as_ref().or(server.lobbies.first()).map(|lobby| lobby.format)).unwrap_or(self.make.format);
                server.queue = None;
                Outcome::Redraw
            }
            Intent::Open(Page::Dashboard | Page::FriendLobby) => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                self.notice = None;
                server.queue = None;
                self.page = if server.friend_lobby().is_some() { Page::FriendLobby } else { Page::Dashboard };
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
            // Each row that proved a key is asked what it keeps of the
            // person, every time the page opens: the server's store is the
            // truth and nothing here is kept. The server attached to has
            // answered on its own connection already.
            Intent::Open(Page::Servers) => {
                self.notice = None;
                self.page = Page::Servers;
                let attached = self.attached_address().map(str::to_string);
                let asked: Vec<String> = self.book.servers.iter().filter(|server| server.key.is_some() && Some(server.address.as_str()) != attached.as_deref()).map(|server| server.address.clone()).collect();
                self.row_profiles = asked.iter().map(|address| (address.clone(), RowProfile::Asking)).collect();
                if asked.is_empty() { Outcome::Redraw } else { Outcome::FetchProfiles(asked) }
            }
            // What to wear starts on what is worn, and the ladder on the
            // lobby's format, since that is the table the person is at.
            Intent::Open(Page::Profile) => {
                let format = self.format;
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                self.notice = None;
                self.page = Page::Profile;
                server.choosing = None;
                server.ladder_format = server.lobby.as_ref().map_or(format, |lobby| lobby.format);
                server.ladder = None;
                Outcome::Ladder(server.ladder_format)
            }
            Intent::Open(page) => {
                self.notice = None;
                self.page = page;
                Outcome::Redraw
            }
            Intent::Back => match self.page {
                // The overlays first — the form, then the search — and
                // then off the screen: the connection goes with it, and
                // the Default is dialled again next time.
                Page::Dashboard => {
                    let Some(server) = &mut self.server else { return Outcome::Leave };
                    if server.queue.take().is_some() {
                        self.notice = None;
                        return Outcome::Redraw;
                    }
                    if server.seeking() {
                        return Outcome::CancelSeek;
                    }
                    Outcome::Leave
                }
                // Out of the lobby, once nothing is being looked for in it.
                Page::FriendLobby => {
                    let Some(server) = &self.server else { return Outcome::Leave };
                    if server.seeking() {
                        return Outcome::CancelSeek;
                    }
                    self.notice = None;
                    if server.lobby.is_some() { Outcome::LeaveLobby } else { Outcome::Leave }
                }
                Page::EditServer => {
                    self.page = Page::Servers;
                    self.notice = None;
                    Outcome::Redraw
                }
                // Opened from the dashboard while attached, it leads back
                // there; with nothing attached it is the way off.
                Page::Servers => {
                    if self.server.is_none() {
                        return Outcome::Leave;
                    }
                    self.page = Page::Dashboard;
                    self.notice = None;
                    Outcome::Redraw
                }
                // The pop-up first, then the page.
                Page::Profile => {
                    if let Some(server) = self.server.as_mut().filter(|server| server.choosing.is_some()) {
                        server.choosing = None;
                        return Outcome::Redraw;
                    }
                    self.page = Page::Dashboard;
                    self.notice = None;
                    Outcome::Redraw
                }
                Page::Waiting => {
                    self.page = self.came_from;
                    self.notice = None;
                    Outcome::Stop
                }
                Page::MakeLobby | Page::Tournaments => {
                    self.page = Page::Dashboard;
                    self.notice = None;
                    Outcome::Redraw
                }
                Page::Tournament | Page::MakeTournament => {
                    self.page = Page::Tournaments;
                    self.notice = None;
                    Outcome::Redraw
                }
                Page::Host | Page::Watch => {
                    self.page = if self.server.is_some() { Page::Dashboard } else { Page::Servers };
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
                    Field::ServerName => self.edit.name = text,
                    Field::ServerAddress => self.edit.address = text,
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
            Intent::SetLobbyBots(bots) => {
                self.make.bots = bots;
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
            // A door: the form for its queues, on the format chosen last
            // where both decks are legal there, and the list read afresh so
            // the form counts who is looking. Ranked is shut where the
            // server rates nothing.
            Intent::OpenQueue(mode) => {
                let format = self.starting_format(mode);
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                if server.seeking() {
                    return Outcome::Nothing;
                }
                if mode == QueueMode::Ranked
                    && let Some(why) = server.ranked_shut()
                {
                    self.notice = Some(why.to_string());
                    return Outcome::Redraw;
                }
                let Some(format) = format else {
                    self.notice = Some(format!("This server lists no {} queue", mode.name()));
                    return Outcome::Redraw;
                };
                self.notice = None;
                server.queue = Some(QueueForm { mode, format });
                Outcome::ListLobbies
            }
            // A greyed pill, pressed, says why rather than changing anything.
            Intent::SetQueueFormat(format) => {
                let problem = queue_problem(format, self.active(Side::Corp), self.active(Side::Runner));
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                let Some(queue) = &mut server.queue else { return Outcome::Nothing };
                if !server.lobbies.iter().any(|lobby| lobby.mode == queue.mode && lobby.format == format) {
                    return Outcome::Nothing;
                }
                if let Some(problem) = problem {
                    self.notice = Some(problem);
                    return Outcome::Redraw;
                }
                queue.format = format;
                self.format = format;
                self.notice = None;
                Outcome::Redraw
            }
            Intent::CloseQueue => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                server.queue = None;
                server.pending = None;
                self.notice = None;
                Outcome::Redraw
            }
            // Find match: both active decks checked against the format —
            // a greyed pill is never sent — then the queue joined, and the
            // seek goes when the server says the person is in it
            // (`LobbyJoined`), or at once when they are already.
            Intent::FindMatch => {
                let Some(queue) = self.server.as_ref().and_then(|server| server.queue) else { return Outcome::Nothing };
                let chair = match self.chair(queue.format) {
                    Ok(chair) => chair,
                    Err(problem) => {
                        self.notice = Some(problem);
                        return Outcome::Redraw;
                    }
                };
                let server = self.server.as_mut().expect("a queue form is on a server");
                if server.seeking() {
                    return Outcome::Nothing;
                }
                self.notice = None;
                server.queue = None;
                let id = queue_id(queue.mode, queue.format);
                if server.lobby.as_ref().is_some_and(|current| current.id == id) {
                    server.searching = Some(Searching::new(server.lobby.clone(), server.prefer, Opponent::Person));
                    return Outcome::Seek(chair, Opponent::Person);
                }
                server.pending = Some(queue);
                Outcome::JoinLobby { id, password: None }
            }
            Intent::JoinById => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                if server.seeking() {
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
                Some(server) if server.lobby.is_some() && !server.seeking() => Outcome::LeaveLobby,
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
            Intent::SetPrefer(prefer) => match &mut self.server {
                Some(server) if !server.seeking() => {
                    server.prefer = prefer;
                    Outcome::Redraw
                }
                _ => Outcome::Nothing,
            },
            // A bot only in a friend lobby that offers its rung.
            Intent::SetOpponent(bot) => match &mut self.server {
                Some(server) if !server.seeking() && bot.is_none_or(|level| server.friend_lobby().is_some_and(|lobby| lobby.bots.contains(&level))) => {
                    server.bot = bot;
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
            Intent::SetCorpDeck(_) | Intent::SetRunnerDeck(_) => Outcome::Nothing,
            // A friend lobby's Find a game: both active decks, the side
            // preferred, and the bot chosen where the lobby lets one in.
            Intent::Seek => {
                let Some(server) = &self.server else { return Outcome::Nothing };
                if server.seeking() {
                    return Outcome::Nothing;
                }
                let Some(lobby) = server.friend_lobby().cloned() else {
                    self.notice = Some("Join a friend lobby first, or find a match from a door".to_string());
                    return Outcome::Redraw;
                };
                match self.chair(lobby.format) {
                    Ok(chair) => {
                        let server = self.server.as_mut().expect("checked above");
                        let opponent = server.opponent();
                        server.searching = Some(Searching::new(Some(lobby), server.prefer, opponent));
                        self.notice = None;
                        Outcome::Seek(chair, opponent)
                    }
                    Err(problem) => {
                        self.notice = Some(format!("{problem}: choose another deck on the dashboard, or build one"));
                        Outcome::Redraw
                    }
                }
            }
            Intent::CancelSeek => match &self.server {
                Some(server) if server.seeking() => Outcome::CancelSeek,
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
            Intent::Sit => match &mut self.server {
                Some(server) if self.page == Page::Tournament && server.may_sit() && !server.seeking() => {
                    match server.open_tournament().and_then(|info| tournament::my_seed_commitment(info, server.key.as_ref())) {
                        Some(seed_commitment) => {
                            self.notice = None;
                            server.searching = Some(Searching::new(None, None, Opponent::Person));
                            Outcome::Sit { tournament: server.tournament.clone().expect("a table in the open tournament"), seed_commitment }
                        }
                        None => {
                            self.notice = Some("This table has no seed commitment yet: refresh the tournament".to_string());
                            Outcome::Redraw
                        }
                    }
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
                if matches!(self.page, Page::Waiting | Page::Dashboard | Page::MakeLobby | Page::FriendLobby | Page::Tournaments | Page::Tournament | Page::MakeTournament | Page::Profile) {
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
                    self.page = Page::Dashboard;
                }
                self.notice = None;
                Outcome::Redraw
            }
            Intent::Lobbies(lobbies) => {
                if let Some(server) = &mut self.server {
                    server.lobbies = lobbies;
                }
                Outcome::Redraw
            }
            Intent::LobbyJoined(lobby) => {
                let prefer = self.server.as_ref().and_then(|server| server.prefer);
                let chair = self.chair(lobby.format);
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                // The list predates the join: the row is brought level
                // with what the server just said, so it counts this
                // person.
                if let Some(row) = server.lobbies.iter_mut().find(|row| row.id == lobby.id) {
                    *row = lobby.clone();
                }
                // A bot chosen in a lobby that offered one is not carried
                // into one that does not.
                if server.bot.is_some_and(|level| !lobby.bots.contains(&level)) {
                    server.bot = None;
                }
                self.notice = None;
                // Find match's queue, joined: the seek goes now.
                if server.pending.take().is_some_and(|queue| queue_id(queue.mode, queue.format) == lobby.id) {
                    server.lobby = Some(lobby.clone());
                    self.page = Page::Dashboard;
                    return match chair {
                        Ok(chair) => {
                            server.searching = Some(Searching::new(Some(lobby), prefer, Opponent::Person));
                            Outcome::Seek(chair, Opponent::Person)
                        }
                        Err(problem) => {
                            self.notice = Some(problem);
                            Outcome::LeaveLobby
                        }
                    };
                }
                let friend = !lobby.permanent();
                server.lobby = Some(lobby);
                if friend {
                    server.queue = None;
                    self.page = Page::FriendLobby;
                } else if self.page == Page::FriendLobby {
                    self.page = Page::Dashboard;
                }
                Outcome::Redraw
            }
            Intent::LobbyLeft => {
                if let Some(server) = &mut self.server {
                    server.lobby = None;
                    server.searching = None;
                    if self.page == Page::FriendLobby {
                        self.page = Page::Dashboard;
                    }
                }
                Outcome::Redraw
            }
            // A refusal of what Find match sent — the join, or the seek
            // before the server had queued it — reopens the form with the
            // reason, so the person can change the format or the decks.
            Intent::Refused(reason) => {
                self.notice = Some(reason);
                if let Some(server) = &mut self.server {
                    if let Some(queue) = server.pending.take() {
                        server.queue = Some(queue);
                    } else if let Some(searching) = server.searching.take_if(|searching| searching.position.is_none())
                        && let Some(lobby) = searching.lobby.filter(LobbyInfo::permanent)
                    {
                        server.queue = Some(QueueForm { mode: lobby.mode, format: lobby.format });
                    }
                }
                Outcome::Redraw
            }
            Intent::Queued(position) => {
                if let Some(server) = &mut self.server {
                    match &mut server.searching {
                        Some(searching) => searching.position = Some(position),
                        none => *none = Some(Searching { position: Some(position), ..Searching::new(server.lobby.clone(), server.prefer, Opponent::Person) }),
                    }
                }
                Outcome::Redraw
            }
            // The search is over: the dashboard is not a queue, so one of
            // the server's own is left; a friend lobby is stayed in.
            Intent::SeekCancelled => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                let in_a_queue = server.searching.take().is_some_and(|searching| searching.in_a_queue()) || server.lobby.as_ref().is_some_and(LobbyInfo::permanent);
                if in_a_queue { Outcome::LeaveLobby } else { Outcome::Redraw }
            }
            Intent::BackInLobby(lobby) => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                server.searching = None;
                if let Some(lobby) = lobby {
                    if server.bot.is_some_and(|level| !lobby.bots.contains(&level)) {
                        server.bot = None;
                    }
                    server.lobby = Some(lobby);
                }
                if server.lobby.as_ref().is_some_and(LobbyInfo::permanent) { Outcome::LeaveLobby } else { Outcome::Redraw }
            }
            Intent::Link(line) => {
                if let Some(server) = &mut self.server {
                    server.link = line;
                }
                Outcome::Redraw
            }
            Intent::Profile { key, profile } => {
                if let Some(server) = &mut self.server {
                    server.key = key;
                    server.profile = profile;
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
            Intent::Servers(book) => {
                self.book = book;
                Outcome::Redraw
            }
            Intent::Local(lines) => {
                self.local = lines;
                Outcome::Redraw
            }
            Intent::RowProfile { address, answer } => {
                let row = match answer {
                    Ok(profile) => RowProfile::Lines(netrunner_client::identity::profile_lines(profile.as_ref())),
                    Err(reason) => RowProfile::Failed(reason),
                };
                self.row_profiles.insert(address, row);
                Outcome::Redraw
            }
            // One connection at a time: connecting to another server lets
            // the one attached go (the screen's new connection replaces
            // it), and the attached one's row is its Disconnect instead.
            Intent::ConnectRow(index) => {
                let Some(row) = self.book.servers.get(index) else { return Outcome::Nothing };
                if self.attached_address() == Some(row.address.as_str()) {
                    return Outcome::Nothing;
                }
                self.address = row.address.clone();
                self.server = None;
                self.came_from = Page::Servers;
                self.notice = None;
                Outcome::Connect { url: row.address.clone() }
            }
            Intent::ToggleDefault(index) => {
                let Some(row) = self.book.servers.get(index) else { return Outcome::Nothing };
                let is_default = self.book.default.as_deref() == Some(row.address.as_str());
                self.notice = None;
                Outcome::EditServers(ServerEdit::Default((!is_default).then(|| row.address.clone())))
            }
            Intent::EditRow(index) => {
                self.edit = match index.and_then(|index| self.book.servers.get(index)) {
                    Some(row) => EditServer { original: Some(row.address.clone()), name: row.name.clone(), address: row.address.clone() },
                    None if index.is_some() => return Outcome::Nothing,
                    None => EditServer::default(),
                };
                self.notice = None;
                self.page = Page::EditServer;
                Outcome::Redraw
            }
            Intent::RemoveRow(index) => {
                let Some(row) = self.book.servers.get(index) else { return Outcome::Nothing };
                self.notice = None;
                Outcome::EditServers(ServerEdit::Remove(row.address.clone()))
            }
            // Checked against the list as this page has it, so a name taken
            // or an address listed twice is said on the form; the file is
            // checked again when it is written.
            Intent::SaveServer => {
                if self.page != Page::EditServer {
                    return Outcome::Nothing;
                }
                let edit = match &self.edit.original {
                    Some(address) => ServerEdit::Edit { address: address.clone(), name: self.edit.name.clone(), new_address: self.edit.address.clone() },
                    None => ServerEdit::Add { name: self.edit.name.clone(), address: self.edit.address.clone() },
                };
                match edit.check(&self.book) {
                    Ok(()) => {
                        self.notice = None;
                        self.page = Page::Servers;
                        Outcome::EditServers(edit)
                    }
                    Err(reason) => {
                        self.notice = Some(reason);
                        Outcome::Redraw
                    }
                }
            }
            Intent::ConnectDefault => {
                let Some(default) = self.book.default_server().map(|server| server.address.clone()) else { return Outcome::Nothing };
                if self.server.is_some() || self.page != Page::Servers {
                    return Outcome::Nothing;
                }
                self.address = default.clone();
                self.came_from = Page::Servers;
                Outcome::Connect { url: default }
            }
            Intent::ActiveDecks(active) => {
                self.active = *active;
                Outcome::Redraw
            }
            // Nothing earned is nothing to choose from.
            Intent::Choose(kind) => {
                let Some(server) = self.server.as_mut().filter(|_| self.page == Page::Profile) else { return Outcome::Nothing };
                let earned = server.known_profile().map_or(0, |profile| match kind {
                    AwardKind::Title => profile.titles.len(),
                    AwardKind::Badge => profile.badges.len(),
                });
                if earned == 0 {
                    return Outcome::Nothing;
                }
                server.choosing = Some(kind);
                Outcome::Redraw
            }
            Intent::CloseChooser => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                server.choosing = None;
                Outcome::Redraw
            }
            // A pick is worn at once, the other kind left as it is worn:
            // one press, not a choice and then a Save.
            Intent::Pick(index) => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                let (Some(kind), Some(profile)) = (server.choosing, server.known_profile()) else { return Outcome::Nothing };
                let earned = match kind {
                    AwardKind::Title => &profile.titles,
                    AwardKind::Badge => &profile.badges,
                };
                let picked = match index {
                    Some(index) => match earned.get(index) {
                        Some(award) => Some(award.name.clone()),
                        None => return Outcome::Nothing,
                    },
                    None => None,
                };
                let (title, badge) = match kind {
                    AwardKind::Title => (picked, profile.badge.clone()),
                    AwardKind::Badge => (profile.title.clone(), picked),
                };
                server.choosing = None;
                self.notice = None;
                Outcome::SetProfile { title, badge }
            }
            Intent::ShowLadder(format) => {
                let Some(server) = &mut self.server else { return Outcome::Nothing };
                server.ladder_format = format;
                server.ladder = None;
                Outcome::Ladder(format)
            }
            Intent::Ladder { format, rows } => {
                if let Some(server) = &mut self.server
                    && server.ladder_format == format
                {
                    server.ladder = Some(rows);
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
        // From the servers page the person stays on it, the row's
        // Disconnect a Connect again; from the dashboard it is the page
        // the connection was made from, the servers.
        if self.page != Page::Servers {
            self.page = if self.came_from == Page::Host { Page::Servers } else { self.came_from };
        }
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
            Page::Servers if self.address.is_empty() => {
                self.notice = Some("Type the server's address, or paste a host's ticket".to_string());
                Outcome::Redraw
            }
            Page::Servers => Outcome::Connect { url: normalize_address(&self.address) },
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
                Outcome::CreateLobby { name: self.make.name.clone(), format: self.make.format, password, bots: self.make.bots }
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
            Page::EditServer => self.apply(Intent::SaveServer),
            Page::Dashboard | Page::FriendLobby | Page::Watch | Page::Waiting | Page::Tournaments | Page::Tournament | Page::Profile => Outcome::Nothing,
        }
    }
}

/// A lobby as a line names it (`netrunner_client::online::lobby_title`):
/// "Ranked · Startup", or a friend queue's name and format.
pub fn lobby_title(lobby: &LobbyInfo) -> String {
    netrunner_client::online::lobby_title(lobby)
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
    use netrunner_server::protocol::QueueMode;

    fn deck(id: &str) -> DeckFile {
        netrunner_core::decks::by_id(id).expect("a built-in deck")
    }

    /// An active deck legal everywhere but `illegal_in`.
    fn active(id: &str, illegal_in: &[(NsgFormat, &str)]) -> ActiveDeck {
        ActiveDeck::fixture(deck(id), id, illegal_in)
    }

    fn form() -> OnlineForm {
        OnlineForm::new("ws://127.0.0.1:8080".to_string(), NsgFormat::Startup).with_active([Some(active("brick_stack", &[])), Some(active("stolen_goods", &[]))])
    }

    /// A lobby by its id: the server's queues by their mode and format,
    /// anything else a friend lobby.
    fn lobby(id: &str, format: NsgFormat) -> LobbyInfo {
        let mode = if id.starts_with("ranked-") {
            QueueMode::Ranked
        } else if id.starts_with("casual-") {
            QueueMode::Casual
        } else {
            QueueMode::Friends
        };
        LobbyInfo { id: id.into(), name: id.into(), format, mode, password: false, rated: mode == QueueMode::Ranked, bots: Vec::new(), players: 0, seeking: 0 }
    }

    /// The server's queues: Ranked and Casual in Startup and Standard.
    fn queues() -> Vec<LobbyInfo> {
        vec![lobby("ranked-startup", NsgFormat::Startup), lobby("ranked-standard", NsgFormat::Standard), lobby("casual-startup", NsgFormat::Startup), lobby("casual-standard", NsgFormat::Standard)]
    }

    /// Connected to a server with those queues, on the dashboard.
    fn attached() -> OnlineForm {
        let mut form = form();
        assert_eq!(form.apply(Intent::Go), Outcome::Connect { url: "ws://127.0.0.1:8080".to_string() });
        form.apply(Intent::Waiting("Connecting…".to_string()));
        assert_eq!(form.apply(Intent::Attached { lobbies: queues(), hosting: false }), Outcome::Redraw);
        assert_eq!(form.page, Page::Dashboard);
        form
    }

    /// Two servers, one met (its key proved) and the Default.
    fn book() -> ServerBook {
        let mut book = ServerBook::default();
        book.add("Club", "club.example.net").unwrap();
        book.add("Home", "localhost").unwrap();
        book.remember_key("ws://club.example.net:8080", netrunner_identity_key(1));
        book.set_default(Some("ws://club.example.net:8080")).unwrap();
        book
    }

    fn netrunner_identity_key(seed: u8) -> PublicKey {
        netrunner_identity::Identity::from_secret([seed; 32]).public_key()
    }

    /// The servers page asks each met server what it keeps of the person,
    /// every time it opens, and draws the answers as they come; a server
    /// never met is not asked.
    #[test]
    fn the_servers_page_asks_each_met_server_and_draws_its_answer() {
        let mut form = form();
        form.apply(Intent::Servers(book()));
        assert_eq!(form.apply(Intent::Open(Page::Servers)), Outcome::FetchProfiles(vec!["ws://club.example.net:8080".to_string()]));
        assert_eq!(form.row_profiles.get("ws://club.example.net:8080"), Some(&RowProfile::Asking));
        form.apply(Intent::RowProfile { address: "ws://club.example.net:8080".to_string(), answer: Ok(None) });
        assert_eq!(form.row_profiles.get("ws://club.example.net:8080"), Some(&RowProfile::Lines(vec!["Nothing kept there yet".to_string()])));
        form.apply(Intent::RowProfile { address: "ws://club.example.net:8080".to_string(), answer: Err("refused".to_string()) });
        assert_eq!(form.row_profiles.get("ws://club.example.net:8080"), Some(&RowProfile::Failed("refused".to_string())));
    }

    /// A row's Connect dials it; one attached elsewhere is let go for it,
    /// and a failure comes back to the servers page with the reason. A
    /// Disconnect there stays there.
    #[test]
    fn a_row_connects_one_at_a_time_and_a_failure_comes_back_to_the_list() {
        let mut form = attached();
        form.apply(Intent::Servers(book()));
        form.apply(Intent::Open(Page::Servers));
        assert_eq!(form.apply(Intent::ConnectRow(1)), Outcome::Connect { url: "ws://localhost:8080".to_string() });
        assert!(form.server.is_none(), "the server attached before is let go");
        form.apply(Intent::Waiting("Connecting…".to_string()));
        form.apply(Intent::Failed("refused".to_string()));
        assert_eq!((form.page, form.notice.as_deref()), (Page::Servers, Some("refused")));
        form.apply(Intent::ConnectRow(0));
        form.apply(Intent::Waiting("Connecting…".to_string()));
        form.apply(Intent::Attached { lobbies: vec![lobby("startup", NsgFormat::Startup)], hosting: false });
        form.apply(Intent::Open(Page::Servers));
        assert_eq!(form.attached_address(), Some("ws://club.example.net:8080"));
        assert_eq!(form.apply(Intent::ConnectRow(0)), Outcome::Nothing, "already attached there");
        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert_eq!(form.page, Page::Dashboard, "opened while attached, it leads back to the dashboard");
        form.apply(Intent::Open(Page::Servers));
        assert_eq!(form.apply(Intent::Disconnect), Outcome::Disconnect);
        assert_eq!((form.page, form.server.is_none()), (Page::Servers, true));
    }

    #[test]
    fn the_default_is_toggled_and_connected_to_on_opening_alone() {
        let mut form = form();
        assert_eq!(form.apply(Intent::ConnectDefault), Outcome::Nothing, "no default, nothing dialled");
        form.apply(Intent::Servers(book()));
        assert_eq!(form.apply(Intent::ToggleDefault(0)), Outcome::EditServers(ServerEdit::Default(None)));
        assert_eq!(form.apply(Intent::ToggleDefault(1)), Outcome::EditServers(ServerEdit::Default(Some("ws://localhost:8080".to_string()))));
        form.apply(Intent::Open(Page::Host));
        assert_eq!(form.apply(Intent::ConnectDefault), Outcome::Nothing, "only from the servers page, on opening");
        form.apply(Intent::Back);
        assert_eq!(form.page, Page::Servers, "with nothing attached, Host leads back to the servers");
        assert_eq!(form.apply(Intent::ConnectDefault), Outcome::Connect { url: "ws://club.example.net:8080".to_string() });
        form.apply(Intent::Waiting("Connecting…".to_string()));
        form.apply(Intent::Failed("refused".to_string()));
        assert_eq!(form.page, Page::Servers, "a Default that does not answer leaves the person at the servers, with the reason");
    }

    /// A server is added or edited on a form checked against the list, so
    /// a name taken is said there and nothing is written.
    #[test]
    fn a_server_is_added_and_edited_on_a_checked_form() {
        let mut form = form();
        form.apply(Intent::Servers(book()));
        form.apply(Intent::Open(Page::Servers));
        form.apply(Intent::EditRow(None));
        assert_eq!(form.page, Page::EditServer);
        form.apply(Intent::Typed(Field::ServerName, "club".to_string()));
        form.apply(Intent::Typed(Field::ServerAddress, "elsewhere.example.net".to_string()));
        assert_eq!(form.apply(Intent::Go), Outcome::Redraw);
        assert!(form.notice.as_deref().is_some_and(|notice| notice.contains("already called")), "{:?}", form.notice);
        form.apply(Intent::Typed(Field::ServerName, "Elsewhere".to_string()));
        assert_eq!(form.apply(Intent::Go), Outcome::EditServers(ServerEdit::Add { name: "Elsewhere".to_string(), address: "elsewhere.example.net".to_string() }));
        assert_eq!(form.page, Page::Servers);
        form.apply(Intent::EditRow(Some(1)));
        assert_eq!((form.edit.name.as_str(), form.edit.address.as_str()), ("Home", "ws://localhost:8080"), "the row's own, to change");
        form.apply(Intent::Typed(Field::ServerName, "Kitchen".to_string()));
        assert_eq!(form.apply(Intent::SaveServer), Outcome::EditServers(ServerEdit::Edit { address: "ws://localhost:8080".to_string(), name: "Kitchen".to_string(), new_address: "ws://localhost:8080".to_string() }));
        form.apply(Intent::EditRow(Some(0)));
        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert_eq!(form.page, Page::Servers, "Cancel");
        assert_eq!(form.apply(Intent::RemoveRow(0)), Outcome::EditServers(ServerEdit::Remove("ws://club.example.net:8080".to_string())));
    }

    /// The profile page's pop-up lists what was earned, and a pick is worn
    /// at once; the ladder of the lobby's format is read first.
    #[test]
    fn the_profile_page_wears_a_pick_from_what_was_earned_and_reads_a_ladder() {
        use netrunner_server::protocol::{Award, Profile};
        let mut form = attached();
        let award = |name: &str| Award { name: name.to_string(), earned_at: 1, source: "event:abc".to_string() };
        let profile = Profile { name: "ann".into(), first_seen: 1, last_seen: 2, standings: Vec::new(), titles: vec![award("Gauntlet Runner"), award("Founder")], badges: vec![award("first-gauntlet")], title: Some("Founder".into()), badge: None };
        form.apply(Intent::Profile { key: Some(netrunner_identity_key(2)), profile: ProfileHere::Known(profile.clone()) });
        form.apply(Intent::LobbyJoined(lobby("casual-standard", NsgFormat::Standard)));
        assert_eq!(form.apply(Intent::Open(Page::Profile)), Outcome::Ladder(NsgFormat::Standard), "the lobby's ladder first");
        assert_eq!(form.apply(Intent::Pick(Some(0))), Outcome::Nothing, "nothing to pick from until the pop-up is open");
        form.apply(Intent::Choose(AwardKind::Title));
        assert_eq!(form.server.as_ref().unwrap().choosing, Some(AwardKind::Title));
        assert_eq!(form.apply(Intent::Pick(Some(0))), Outcome::SetProfile { title: Some("Gauntlet Runner".into()), badge: None }, "worn at once, the badge as it is worn");
        assert_eq!(form.server.as_ref().unwrap().choosing, None, "the pop-up closes on a pick");
        form.apply(Intent::Choose(AwardKind::Badge));
        assert_eq!(form.apply(Intent::Pick(Some(0))), Outcome::SetProfile { title: Some("Founder".into()), badge: Some("first-gauntlet".into()) });
        form.apply(Intent::Choose(AwardKind::Title));
        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert_eq!((form.page, form.server.as_ref().unwrap().choosing), (Page::Profile, None), "Escape closes the pop-up before the page");
        form.apply(Intent::Choose(AwardKind::Title));
        assert_eq!(form.apply(Intent::Pick(None)), Outcome::SetProfile { title: None, badge: None }, "None takes the title off");
        let mut bare = attached();
        bare.apply(Intent::Profile { key: None, profile: ProfileHere::Known(Profile { titles: Vec::new(), ..profile.clone() }) });
        bare.apply(Intent::Open(Page::Profile));
        assert_eq!(bare.apply(Intent::Choose(AwardKind::Title)), Outcome::Nothing, "nothing earned, nothing to open");
        assert_eq!(form.apply(Intent::ShowLadder(NsgFormat::Startup)), Outcome::Ladder(NsgFormat::Startup));
        form.apply(Intent::Ladder { format: NsgFormat::Standard, rows: Vec::new() });
        assert_eq!(form.server.as_ref().unwrap().ladder, None, "a late answer about another format is not drawn");
        form.apply(Intent::Ladder { format: NsgFormat::Startup, rows: Vec::new() });
        assert_eq!(form.server.as_ref().unwrap().ladder, Some(Vec::new()));
        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert_eq!(form.page, Page::Dashboard);
    }

    /// The screen opens on the servers with nothing attached, and Escape
    /// is the way off from there; attached, the dashboard is, and Escape
    /// closes the queue form first, then aborts a search, then leaves.
    #[test]
    fn escape_closes_the_form_then_aborts_then_leaves() {
        let mut form = form();
        assert_eq!(form.page, Page::Servers);
        form.apply(Intent::Open(Page::Host));
        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert_eq!(form.page, Page::Servers);
        assert_eq!(form.apply(Intent::Back), Outcome::Leave);

        let mut form = attached();
        form.apply(Intent::OpenQueue(QueueMode::Casual));
        assert!(form.server.as_ref().unwrap().queue.is_some());
        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert!(form.server.as_ref().unwrap().queue.is_none(), "the form closes first");
        form.apply(Intent::OpenQueue(QueueMode::Casual));
        form.apply(Intent::FindMatch);
        form.apply(Intent::LobbyJoined(lobby("casual-startup", NsgFormat::Startup)));
        assert!(form.server.as_ref().unwrap().seeking());
        assert_eq!(form.apply(Intent::Back), Outcome::CancelSeek, "then the search is aborted");
        form.apply(Intent::SeekCancelled);
        form.apply(Intent::LobbyLeft);
        assert_eq!(form.apply(Intent::Back), Outcome::Leave, "then off the screen");
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
        form.apply(Intent::Open(Page::Servers));
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
        form.apply(Intent::Open(Page::Servers));
        form.apply(Intent::Waiting("Connecting…".to_string()));
        assert_eq!(form.page, Page::Waiting);
        form.apply(Intent::Status("Reconnecting".to_string()));
        assert_eq!(form.apply(Intent::Back), Outcome::Stop);
        assert_eq!(form.page, Page::Servers);
        form.apply(Intent::Waiting("Connecting…".to_string()));
        form.apply(Intent::Failed("refused".to_string()));
        assert_eq!((form.page, form.notice.as_deref()), (Page::Servers, Some("refused")));
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

    /// A door opens the queue form on the format chosen last where both
    /// active decks are legal there, else the first they are; a format an
    /// active deck is not legal in is greyed with the reason and refused
    /// with it when pressed; Find match joins the queue and, once the
    /// server says the person is in it, looks for a game with both decks
    /// in Either chair and the side preferred; Abort stops the search and
    /// leaves the queue, since the dashboard is not one.
    #[test]
    fn find_match_joins_the_queue_and_seeks_with_both_active_decks() {
        let mut form = attached();
        form.active[0] = Some(active("brick_stack", &[(NsgFormat::Standard, "3 rotated cards")]));
        assert_eq!(form.apply(Intent::FindMatch), Outcome::Nothing, "no form open");
        assert_eq!(form.apply(Intent::OpenQueue(QueueMode::Ranked)), Outcome::ListLobbies, "the list is read afresh, for who is looking");
        assert_eq!(form.server.as_ref().unwrap().queue, Some(QueueForm { mode: QueueMode::Ranked, format: NsgFormat::Startup }));
        assert_eq!(form.queue_choices(), vec![(NsgFormat::Startup, None), (NsgFormat::Standard, Some("Corp deck: 3 rotated cards".to_string()))]);
        assert_eq!(form.apply(Intent::SetQueueFormat(NsgFormat::Standard)), Outcome::Redraw);
        assert_eq!((form.server.as_ref().unwrap().queue.unwrap().format, form.notice.as_deref()), (NsgFormat::Startup, Some("Corp deck: 3 rotated cards")), "a greyed pill says why");
        assert_eq!(form.apply(Intent::SetQueueFormat(NsgFormat::Eternal)), Outcome::Nothing, "no such queue");
        form.apply(Intent::SetPrefer(Some(Side::Runner)));
        assert_eq!(form.apply(Intent::FindMatch), Outcome::JoinLobby { id: "ranked-startup".into(), password: None });
        let server = form.server.as_ref().unwrap();
        assert!(server.queue.is_none() && server.pending.is_some() && !server.seeking(), "the seek waits for the join");
        let Outcome::Seek(Chair::Either { corp, runner, prefer }, Opponent::Person) = form.apply(Intent::LobbyJoined(LobbyInfo { players: 1, ..lobby("ranked-startup", NsgFormat::Startup) })) else { panic!("either chair, both decks") };
        assert_eq!((corp.id.as_str(), runner.id.as_str(), prefer), ("brick_stack", "stolen_goods", Some(Side::Runner)));
        let server = form.server.as_ref().unwrap();
        assert!(server.seeking() && server.pending.is_none() && form.page == Page::Dashboard);
        assert_eq!(server.lobbies[0].players, 1, "the row counts the person who just joined");
        assert_eq!(form.apply(Intent::OpenQueue(QueueMode::Casual)), Outcome::Nothing, "one search at a time");
        assert_eq!(form.apply(Intent::LeaveLobby), Outcome::Nothing, "and the queue is kept while looking");
        form.apply(Intent::Queued(2));
        assert_eq!(form.server.as_ref().unwrap().searching.as_ref().unwrap().position, Some(2));
        assert_eq!(form.apply(Intent::CancelSeek), Outcome::CancelSeek);
        assert_eq!(form.apply(Intent::SeekCancelled), Outcome::LeaveLobby, "the dashboard is not a queue");
        form.apply(Intent::LobbyLeft);
        let server = form.server.as_ref().unwrap();
        assert!(!server.seeking() && server.lobby.is_none());

        // Already in the queue (a refusal left the person there): the
        // seek goes at once.
        form.apply(Intent::LobbyJoined(lobby("ranked-startup", NsgFormat::Startup)));
        form.apply(Intent::OpenQueue(QueueMode::Ranked));
        assert!(matches!(form.apply(Intent::FindMatch), Outcome::Seek(Chair::Either { .. }, Opponent::Person)));
        // The end of the game puts the person back in the queue, which
        // is left for the dashboard.
        assert_eq!(form.apply(Intent::BackInLobby(Some(lobby("ranked-startup", NsgFormat::Startup)))), Outcome::LeaveLobby);
        assert!(!form.server.as_ref().unwrap().seeking());
    }

    /// With no deck chosen for a side every format is greyed and Find
    /// match says so; a server that lists no queue of the mode says so.
    #[test]
    fn a_missing_deck_greys_every_format() {
        let mut form = attached();
        form.active[1] = None;
        form.apply(Intent::OpenQueue(QueueMode::Casual));
        assert!(form.queue_choices().iter().all(|(_, problem)| problem.as_deref() == Some("No Runner deck is chosen")));
        assert_eq!(form.apply(Intent::FindMatch), Outcome::Redraw);
        assert_eq!(form.notice.as_deref(), Some("No Runner deck is chosen"));
        assert!(form.server.as_ref().unwrap().queue.is_some(), "the form stays up");
        form.apply(Intent::Lobbies(vec![lobby("ranked-startup", NsgFormat::Startup)]));
        form.apply(Intent::CloseQueue);
        assert_eq!(form.apply(Intent::OpenQueue(QueueMode::Casual)), Outcome::Redraw);
        assert!(form.notice.as_deref().is_some_and(|notice| notice.contains("no Casual queue")));
    }

    /// Ranked is shut on a hosted server and on one that keeps no book,
    /// with the reason.
    #[test]
    fn ranked_is_shut_where_the_server_rates_nothing() {
        let mut guest = attached();
        guest.apply(Intent::Lobbies(vec![lobby("casual-startup", NsgFormat::Startup)]));
        assert_eq!(guest.server.as_ref().unwrap().ranked_shut(), Some("This server keeps no ratings"));
        assert_eq!(guest.apply(Intent::OpenQueue(QueueMode::Ranked)), Outcome::Redraw);
        assert_eq!((guest.server.as_ref().unwrap().queue, guest.notice.as_deref()), (None, Some("This server keeps no ratings")));

        let mut host = form();
        host.apply(Intent::Open(Page::Host));
        host.apply(Intent::Typed(Field::Port, "0".to_string()));
        host.apply(Intent::Go);
        host.apply(Intent::Attached { lobbies: vec![LobbyInfo { rated: false, ..lobby("casual-startup", NsgFormat::Startup) }], hosting: true });
        assert_eq!(host.server.as_ref().unwrap().ranked_shut(), Some("A game hosted from this machine is never rated"));
    }

    /// A refusal of what Find match sent — the join, or the seek before
    /// it was queued — reopens the form with the reason.
    #[test]
    fn a_refusal_reopens_the_queue_form() {
        let mut form = attached();
        form.apply(Intent::OpenQueue(QueueMode::Casual));
        form.apply(Intent::FindMatch);
        form.apply(Intent::Refused("that lobby has gone".to_string()));
        let server = form.server.as_ref().unwrap();
        assert_eq!((server.queue, server.pending, form.notice.as_deref()), (Some(QueueForm { mode: QueueMode::Casual, format: NsgFormat::Startup }), None, Some("that lobby has gone")));
        form.apply(Intent::FindMatch);
        form.apply(Intent::LobbyJoined(lobby("casual-startup", NsgFormat::Startup)));
        assert!(form.server.as_ref().unwrap().seeking());
        form.apply(Intent::Refused("your deck is not legal here".to_string()));
        let server = form.server.as_ref().unwrap();
        assert!(!server.seeking() && server.queue.is_some(), "the seek refused before it was queued reopens the form");
        form.apply(Intent::FindMatch);
        form.apply(Intent::Queued(1));
        form.apply(Intent::Refused("a title you have not earned".to_string()));
        assert!(form.server.as_ref().unwrap().seeking(), "a refusal of something else leaves a queued search alone");
    }

    /// Under Casual a friend lobby is joined by the code typed, with the
    /// password, and its page opens; a refusal changes nothing.
    #[test]
    fn the_casual_form_joins_a_friend_lobby_by_code() {
        let mut form = attached();
        form.apply(Intent::OpenQueue(QueueMode::Casual));
        assert_eq!(form.apply(Intent::JoinById), Outcome::Redraw, "no code typed");
        form.apply(Intent::Typed(Field::Lobby, "k7m2qx".to_string()));
        form.apply(Intent::Typed(Field::Password, "swordfish".to_string()));
        assert_eq!(form.apply(Intent::JoinById), Outcome::JoinLobby { id: "k7m2qx".into(), password: Some("swordfish".into()) });
        form.apply(Intent::Refused("that is not lobby K7M2QX's password".to_string()));
        assert!(form.server.as_ref().unwrap().lobby.is_none() && form.notice.is_some() && form.server.as_ref().unwrap().queue.is_some(), "a refusal changes nothing");
        let friday = LobbyInfo { mode: QueueMode::Friends, name: "Friday".into(), password: true, rated: false, players: 2, ..lobby("K7M2QX", NsgFormat::Startup) };
        assert_eq!(form.apply(Intent::LobbyJoined(friday.clone())), Outcome::Redraw);
        let server = form.server.as_ref().unwrap();
        assert_eq!((form.page, server.friend_lobby(), server.queue), (Page::FriendLobby, Some(&friday), None));
        assert_eq!(form.apply(Intent::Refresh), Outcome::ListLobbies);
        form.apply(Intent::Lobbies(vec![lobby("ranked-startup", NsgFormat::Startup)]));
        assert_eq!(form.server.as_ref().unwrap().lobbies.len(), 1);
        assert_eq!(form.apply(Intent::Back), Outcome::LeaveLobby, "Escape leaves the lobby");
        form.apply(Intent::LobbyLeft);
        assert_eq!((form.page, form.server.as_ref().unwrap().lobby.is_none()), (Page::Dashboard, true));
    }

    /// In a friend lobby a game is looked for among its members with both
    /// active decks, against a person or — only where the maker let bots
    /// in — a bot at a rung the lobby offers; the end of the game keeps
    /// the person in the lobby.
    #[test]
    fn a_friend_lobby_offers_a_bot_only_where_it_lets_one_in() {
        let mut form = attached();
        assert_eq!(form.apply(Intent::Seek), Outcome::Redraw, "no friend lobby yet");
        assert!(form.notice.as_deref().is_some_and(|notice| notice.contains("friend lobby")));
        let plain = LobbyInfo { mode: QueueMode::Friends, name: "Friday".into(), rated: false, ..lobby("K7M2QX", NsgFormat::Startup) };
        form.apply(Intent::LobbyJoined(plain.clone()));
        assert_eq!(form.apply(Intent::SetOpponent(Some(Level::Elite))), Outcome::Nothing, "the maker kept bots out");
        form.apply(Intent::SetPrefer(Some(Side::Corp)));
        let Outcome::Seek(Chair::Either { corp, runner, prefer }, Opponent::Person) = form.apply(Intent::Seek) else { panic!("both decks, a person") };
        assert_eq!((corp.id.as_str(), runner.id.as_str(), prefer), ("brick_stack", "stolen_goods", Some(Side::Corp)));
        assert_eq!(form.server.as_ref().unwrap().searching.as_ref().unwrap().lobby, Some(plain.clone()));
        assert_eq!(form.apply(Intent::Seek), Outcome::Nothing, "one at a time");
        assert_eq!(form.apply(Intent::SeekCancelled), Outcome::Redraw, "a friend lobby is stayed in");
        assert_eq!(form.page, Page::FriendLobby);

        let bots = LobbyInfo { bots: Level::ALL.to_vec(), ..plain.clone() };
        form.apply(Intent::LobbyJoined(bots));
        assert_eq!(form.apply(Intent::SetOpponent(Some(Level::Veteran))), Outcome::Redraw);
        assert!(matches!(form.apply(Intent::Seek), Outcome::Seek(Chair::Either { .. }, Opponent::Bot { level: Level::Veteran, style: None })));
        assert_eq!(form.apply(Intent::BackInLobby(Some(plain.clone()))), Outcome::Redraw, "back in the lobby after the game");
        let server = form.server.as_ref().unwrap();
        assert_eq!((server.bot, server.seeking()), (None, false), "a bot chosen is let go in a lobby that offers none");
        form.reopen(true);
        assert_eq!(form.page, Page::FriendLobby, "back from the board, the lobby's page");

        // A deck not legal in the lobby's format is said, not sent.
        form.active[1] = Some(active("stolen_goods", &[(NsgFormat::Startup, "a rotated card")]));
        assert_eq!(form.apply(Intent::Seek), Outcome::Redraw);
        assert!(form.notice.as_deref().is_some_and(|notice| notice.starts_with("Runner deck: a rotated card")));
    }

    /// A friend lobby is made from its own page, for the format of the
    /// form it was opened from, with or without a password and bots; the
    /// server's answer lands the person on its page.
    #[test]
    fn a_friend_lobby_is_made_from_its_page() {
        let mut form = attached();
        form.apply(Intent::OpenQueue(QueueMode::Casual));
        form.apply(Intent::SetQueueFormat(NsgFormat::Standard));
        form.apply(Intent::Open(Page::MakeLobby));
        assert_eq!((form.page, form.make.format, form.server.as_ref().unwrap().queue), (Page::MakeLobby, NsgFormat::Standard, None));
        assert_eq!(form.apply(Intent::Go), Outcome::Redraw, "a lobby needs a name");
        form.apply(Intent::Typed(Field::LobbyName, " Friday ".to_string()));
        form.apply(Intent::SetFormat(NsgFormat::Startup));
        form.apply(Intent::Typed(Field::LobbyPassword, "swordfish".to_string()));
        form.apply(Intent::SetLobbyBots(true));
        assert_eq!(form.apply(Intent::Go), Outcome::CreateLobby { name: "Friday".into(), format: NsgFormat::Startup, password: Some("swordfish".into()), bots: true });
        let made = LobbyInfo { mode: QueueMode::Friends, password: true, rated: false, bots: Level::ALL.to_vec(), ..lobby("K7M2QX", NsgFormat::Startup) };
        assert_eq!(form.apply(Intent::LobbyJoined(made.clone())), Outcome::Redraw);
        assert_eq!((form.page, form.server.as_ref().unwrap().lobby.as_ref()), (Page::FriendLobby, Some(&made)));
        form.apply(Intent::Open(Page::MakeLobby));
        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert_eq!(form.page, Page::Dashboard);
    }

    /// The host's own server lands the host on the dashboard, its one
    /// Casual queue the door open; a disconnect from the servers page
    /// lets the connection go and stays there; a connection down for
    /// good reopens the page the person came from with the reason.
    #[test]
    fn a_host_lands_on_the_dashboard_and_a_disconnect_reopens_the_page() {
        let mut form = form();
        form.apply(Intent::Open(Page::Host));
        form.apply(Intent::Typed(Field::Port, "0".to_string()));
        assert!(matches!(form.apply(Intent::Go), Outcome::Host { .. }));
        form.apply(Intent::Waiting("Hosting…".to_string()));
        let hosted = || LobbyInfo { rated: false, ..lobby("casual-startup", NsgFormat::Startup) };
        assert_eq!(form.apply(Intent::Attached { lobbies: vec![hosted()], hosting: true }), Outcome::Redraw);
        assert!(form.server.as_ref().is_some_and(|server| server.hosting && server.address == "this machine" && server.lobby.is_none()));
        assert_eq!(form.page, Page::Dashboard);
        assert_eq!(form.apply(Intent::OpenQueue(QueueMode::Casual)), Outcome::ListLobbies);
        assert_eq!(form.apply(Intent::FindMatch), Outcome::JoinLobby { id: "casual-startup".into(), password: None });
        assert!(matches!(form.apply(Intent::LobbyJoined(hosted())), Outcome::Seek(..)));
        assert_eq!(form.apply(Intent::Attached { lobbies: vec![hosted()], hosting: true }), Outcome::Redraw, "a reattach lists afresh and joins nothing: the machine rejoins by itself");
        form.apply(Intent::Link(Some("Connection lost".to_string())));
        assert!(form.server.as_ref().unwrap().link.is_some());
        form.apply(Intent::Link(None));
        assert!(form.server.as_ref().unwrap().link.is_none());
        form.apply(Intent::SeekCancelled);
        form.apply(Intent::Open(Page::Servers));
        assert_eq!(form.apply(Intent::Disconnect), Outcome::Disconnect);
        assert_eq!((form.page, form.server.is_none()), (Page::Servers, true));

        let mut form = attached();
        form.apply(Intent::Failed("could not reconnect within 60s".to_string()));
        assert_eq!((form.page, form.server.is_none(), form.notice.is_some()), (Page::Servers, true, true));
    }

    /// The line under the queue form's title says what the queue offers
    /// and, once the server has answered, what this person stands to gain
    /// there — an unrated lobby says so whatever the standing, and the
    /// host's own server never rates (Phase 4 §7 stage 5).
    #[test]
    fn the_lobby_says_whether_a_game_there_counts() {
        let mut guest = attached();
        guest.apply(Intent::LobbyJoined(lobby("ranked-startup", NsgFormat::Startup)));
        let line = |form: &OnlineForm| {
            let server = form.server.as_ref().unwrap();
            server.rating_line(server.lobby.as_ref().unwrap())
        };
        assert_eq!(line(&guest), "Rated here.", "before the server has answered");
        guest.apply(Intent::Profile { key: None, profile: ProfileHere::Unkept });
        assert_eq!(line(&guest), "Rated here. No rated games here yet.");
        guest.apply(Intent::Profile { key: None, profile: ProfileHere::NoKey });
        assert_eq!(line(&guest), "Unrated for you: this client has no key, so nothing here counts.");
        guest.apply(Intent::LobbyJoined(LobbyInfo { rated: false, mode: QueueMode::Friends, name: "Friday".into(), ..lobby("K7M2QX", NsgFormat::Startup) }));
        assert_eq!(line(&guest), "Unrated: nothing in this lobby is rated.");

        let mut host = form();
        host.apply(Intent::Open(Page::Host));
        host.apply(Intent::Typed(Field::Port, "0".to_string()));
        host.apply(Intent::Go);
        host.apply(Intent::Attached { lobbies: vec![LobbyInfo { rated: false, ..lobby("casual-startup", NsgFormat::Startup) }], hosting: true });
        host.apply(Intent::LobbyJoined(LobbyInfo { rated: false, ..lobby("casual-startup", NsgFormat::Startup) }));
        assert_eq!(line(&host), "Unrated: a game hosted from this machine is never rated.");
    }

    #[test]
    fn a_servers_own_queue_is_named_by_its_mode_and_format() {
        assert_eq!(lobby_title(&lobby("ranked-startup", NsgFormat::Startup)), "Ranked · Startup");
        assert_eq!(lobby_title(&LobbyInfo { name: "Friday".into(), mode: QueueMode::Friends, ..lobby("K7M2QX", NsgFormat::Standard) }), "Friday · Standard");
    }

    fn key(byte: u8) -> PublicKey {
        netrunner_identity::Identity::from_secret([byte; 32]).public_key()
    }

    fn tournament_info(id: &str, organizer: PublicKey, entrants: Vec<netrunner_server::protocol::Entrant>) -> TournamentInfo {
        TournamentInfo { id: id.into(), name: "Friday".into(), format: NsgFormat::Startup, organizer, state: TournamentState::Registering, entrants, seeding: Vec::new(), rounds: Vec::new(), dropped: Vec::new(), draw_offers: Vec::new(), clock: None, seeds: Vec::new() }
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
        form.apply(Intent::Profile { key: Some(key(2)), profile: ProfileHere::Unkept });
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
        assert_eq!(form.apply(Intent::SetCorpDeck(1)), Outcome::Redraw, "the tournament's own choice");
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
        assert_eq!(form.page, Page::Dashboard);
        assert_eq!(form.apply(Intent::Register), Outcome::Nothing, "only from the page");
    }

    /// A key that has none cannot register or hold one; a tournament that
    /// has closed takes no entry; the drop-downs read the tournament's
    /// decks afresh by id.
    #[test]
    fn a_keyless_client_and_a_closed_tournament_offer_no_entry() {
        let mut form = attached();
        form.apply(Intent::Profile { key: None, profile: ProfileHere::NoKey });
        form.apply(Intent::Open(Page::Tournaments));
        form.apply(Intent::Tournaments(vec![tournament_info("K7M2QX", key(1), vec![])]));
        assert_eq!(form.apply(Intent::Open(Page::MakeTournament)), Outcome::Redraw);
        assert!(form.page == Page::Tournaments && form.notice.as_deref().is_some_and(|notice| notice.contains("no")));
        form.apply(Intent::OpenTournament(0));
        form.set_tournament_decks(vec![deck("brick_stack"), deck("stolen_goods")]);
        assert!(!form.server.as_ref().unwrap().may_register());
        assert_eq!(form.apply(Intent::Register), Outcome::Nothing);

        form.apply(Intent::Profile { key: Some(key(2)), profile: ProfileHere::Unkept });
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
        form.apply(Intent::Profile { key: Some(key(1)), profile: ProfileHere::Unkept });
        form.apply(Intent::LobbyJoined(lobby("casual-standard", NsgFormat::Standard)));
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
        playing.seeds = vec![netrunner_server::protocol::statements::TableSeed { round: 0, table: 0, commitment: "commitment".into(), reveal: None }];

        // bo, paired: the seat, and nothing of the organizer's.
        let mut form = attached();
        form.apply(Intent::Profile { key: Some(bo), profile: ProfileHere::Unkept });
        form.apply(Intent::Open(Page::Tournaments));
        form.apply(Intent::Tournaments(vec![playing.clone()]));
        form.apply(Intent::OpenTournament(0));
        let server = form.server.as_ref().unwrap();
        assert!(server.may_sit() && !server.may_register() && server.next_round().is_none() && !server.may_finish() && server.tables_to_record().is_empty());
        assert_eq!(form.apply(Intent::BeginRound), Outcome::Nothing);
        assert_eq!(form.apply(Intent::Sit), Outcome::Sit { tournament: "K7M2QX".into(), seed_commitment: "commitment".into() });
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
        bye.apply(Intent::Profile { key: Some(cy), profile: ProfileHere::Unkept });
        bye.apply(Intent::Open(Page::Tournaments));
        bye.apply(Intent::Tournaments(vec![playing.clone()]));
        bye.apply(Intent::OpenTournament(0));
        assert_eq!(bye.apply(Intent::Sit), Outcome::Nothing);
        assert_eq!(bye.apply(Intent::OfferDraw), Outcome::Nothing, "no table, no draw");

        // ann, the organizer and a player: the seat, and once the table
        // has its result the next round or the end; a result recorded
        // only for a table that has none.
        let mut organizer = attached();
        organizer.apply(Intent::Profile { key: Some(ann), profile: ProfileHere::Unkept });
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
        fresh.apply(Intent::Profile { key: Some(ann), profile: ProfileHere::Unkept });
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

    /// Back from the board the dashboard is up, still attached; after a
    /// game watched, or once the connection has gone, the servers are.
    #[test]
    fn reopening_returns_to_the_dashboard_while_attached() {
        let mut form = attached();
        form.reopen(true);
        assert_eq!(form.page, Page::Dashboard);
        form.reopen(false);
        assert_eq!((form.page, form.server.is_none()), (Page::Servers, true));
    }
}
