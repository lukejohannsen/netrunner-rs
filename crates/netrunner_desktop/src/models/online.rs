//! The Play Online screen's state: which page is up, what the person has
//! typed and chosen, where the connection stands, and what a press asks
//! the screen to do.
//!
//! **Three ways in, as the terminal offers them** (`netrunner_cli::tui::
//! online`): host a game on this machine and give the opponent an address
//! or a ticket; connect to a server by address or ticket; or list a
//! server's matches and watch one. Hosting and connecting both end on the
//! **Server page**, attached (Phase 4 §7 stage 4c): the server's lobbies
//! listed, one joined, a friend queue made or joined by its code,
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
//! 2026): a lobby pairs whoever is waiting, and a friend queue — joined by
//! its code, and its password if it has one — is how two people who know
//! each other meet (Phase 4 §9).
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
use std::collections::BTreeMap;

use netrunner_client::identity::{ProfileHere, PublicKey};
use netrunner_client::servers::{ServerBook, ServerEdit};
use netrunner_client::tournament;
use netrunner_core::decks::DeckFile;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;
use netrunner_server::protocol::swiss::Outcome as TableOutcome;
use netrunner_server::protocol::{Chair, LadderRow, Level, LobbyInfo, Opponent, Profile, TournamentInfo, TournamentState};
use netrunner_server::MatchSummary;

/// The page up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// The three ways in.
    Home,
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
    /// The Server page's: a friend queue's code, and its password — also
    /// the password a listed lobby asks for.
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
    /// Make a friend queue's: whether its members may ask for a bot.
    SetLobbyBots(bool),
    /// Which side of the table a spectator sits nearer.
    SetWatchFrom(Side),
    /// Host's, Join's and Make a lobby's primary button.
    Go,
    /// List the matches at Watch's address.
    List,
    Watch(usize),
    /// The Server page: a listed lobby by its row, a friend queue by the
    /// code typed, out of the lobby, the list again.
    JoinLobby(usize),
    JoinById,
    LeaveLobby,
    Refresh,
    SetChair(ChairChoice),
    /// Either chair's preferred side, if any (Phase 4 §9).
    SetPrefer(Option<Side>),
    /// Whom to look for a game against: a person, or a bot at a rung the
    /// lobby offers.
    SetOpponent(Option<Level>),
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
    /// The lobby this connection is in.
    pub lobby: Option<LobbyInfo>,
    /// Looking for a game, at this position in the queue.
    pub seeking: Option<usize>,
    /// The decks legal in the lobby's format, built-in and saved
    /// (`netrunner_client::online::deck_choices`), both sides.
    pub decks: Vec<DeckFile>,
    pub chair: ChairChoice,
    /// Either chair's preferred side, if any.
    pub prefer: Option<Side>,
    /// A bot to play rather than whoever the queue pairs, at a rung the
    /// lobby offers; `None` is a person.
    pub bot: Option<Level>,
    /// Which of each side's decks (`corp_decks`, `runner_decks`).
    pub corp_deck: usize,
    pub runner_deck: usize,
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
            seeking: None,
            decks: Vec::new(),
            chair: ChairChoice::Corp,
            prefer: None,
            bot: None,
            corp_deck: 0,
            runner_deck: 0,
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

    /// The line over Find a game: what the lobby offers and what this
    /// person stands to gain there (`identity::lobby_rating_line`).
    pub fn rating_line(&self, lobby: &LobbyInfo) -> String {
        netrunner_client::identity::lobby_rating_line(lobby.rated, self.hosting, lobby.format, &self.profile)
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
            ChairChoice::Either => Chair::Either { corp: deck(Side::Corp)?, runner: deck(Side::Runner)?, prefer: self.prefer },
        })
    }

    /// Whom the seek is against: the bot chosen if the lobby offers its
    /// rung, else a person.
    pub fn opponent(&self) -> Opponent {
        match (self.bot, &self.lobby) {
            (Some(level), Some(lobby)) if lobby.bots.contains(&level) => Opponent::Bot { level, style: None },
            _ => Opponent::Person,
        }
    }
}

/// The friend queue being made: casual, and joined by its code.
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
    /// The active deck a side (`Settings::active_deck`, Phase 7 §16):
    /// what a lobby's or a tournament's deck starts on when nothing has
    /// been chosen on this page yet — Corp, then Runner.
    pub preferred: [Option<String>; 2],
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
    /// The person's servers, as the file last had them.
    pub book: ServerBook,
    /// Each row's answer about the person, by address.
    pub row_profiles: BTreeMap<String, RowProfile>,
    /// What this machine knows of the person, for the servers page.
    pub local: Vec<String>,
    pub edit: EditServer,
}

impl OnlineForm {
    /// `address` is where Join and Watch start: the last one used, or the
    /// terminal's default server. `format` is the one Settings names,
    /// where Host and Make a lobby start.
    pub fn new(address: String, format: NsgFormat) -> Self {
        OnlineForm {
            preferred: [None, None],
            page: Page::Home,
            came_from: Page::Home,
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

    /// The active decks, which a lobby's and a tournament's choice start
    /// on (`preferred`).
    pub fn with_preferred(mut self, preferred: [Option<String>; 2]) -> Self {
        self.preferred = preferred;
        self
    }

    /// The decks legal in the lobby's format, read afresh: each side's
    /// chosen deck stays chosen if it is still among them, else the
    /// active deck for the side if it is, and the first otherwise.
    pub fn set_decks(&mut self, decks: Vec<DeckFile>) {
        let preferred = self.preferred.clone();
        let Some(server) = &mut self.server else { return };
        let keep = |chosen: Option<&DeckFile>, side: Side| {
            let among: Vec<&DeckFile> = decks.iter().filter(|deck| deck.side == side).collect();
            let position = |id: &str| among.iter().position(|deck| deck.id == id);
            chosen.and_then(|deck| position(&deck.id)).or_else(|| preferred[side as usize].as_deref().and_then(position)).unwrap_or(0)
        };
        let (corp, runner) = (keep(server.chosen(Side::Corp), Side::Corp), keep(server.chosen(Side::Runner), Side::Runner));
        server.decks = decks;
        server.corp_deck = corp;
        server.runner_deck = runner;
    }

    /// The decks legal in the open tournament's format, read afresh: each
    /// side's chosen deck stays chosen if it is still among them, else the
    /// active deck for the side if it is.
    pub fn set_tournament_decks(&mut self, decks: Vec<DeckFile>) {
        let preferred = self.preferred.clone();
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
                Page::Home => Outcome::Leave,
                Page::EditServer => {
                    self.page = Page::Servers;
                    self.notice = None;
                    Outcome::Redraw
                }
                // Opened from the Server page while attached, it leads back
                // there; otherwise Home.
                Page::Servers => {
                    self.page = if self.server.is_some() { Page::Server } else { Page::Home };
                    self.notice = None;
                    Outcome::Redraw
                }
                // The pop-up first, then the page.
                Page::Profile => {
                    if let Some(server) = self.server.as_mut().filter(|server| server.choosing.is_some()) {
                        server.choosing = None;
                        return Outcome::Redraw;
                    }
                    self.page = Page::Server;
                    self.notice = None;
                    Outcome::Redraw
                }
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
                Page::Host | Page::Watch => {
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
            Intent::SetPrefer(prefer) => match &mut self.server {
                Some(server) if server.seeking.is_none() => {
                    server.prefer = prefer;
                    Outcome::Redraw
                }
                _ => Outcome::Nothing,
            },
            Intent::SetOpponent(bot) => match &mut self.server {
                Some(server) if server.seeking.is_none() && bot.is_none_or(|level| server.lobby.as_ref().is_some_and(|lobby| lobby.bots.contains(&level))) => {
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
                        Outcome::Seek(chair, server.opponent())
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
                    match server.open_tournament().and_then(|info| tournament::my_seed_commitment(info, server.key.as_ref())) {
                        Some(seed_commitment) => {
                            self.notice = None;
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
                if matches!(self.page, Page::Waiting | Page::Server | Page::MakeLobby | Page::Tournaments | Page::Tournament | Page::MakeTournament | Page::Profile) {
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
                // A bot chosen in a lobby that offered one is not carried
                // into one that does not.
                if server.bot.is_some_and(|level| !lobby.bots.contains(&level)) {
                    server.bot = None;
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
                if self.server.is_some() || self.page != Page::Home {
                    return Outcome::Nothing;
                }
                self.address = default.clone();
                self.came_from = Page::Home;
                Outcome::Connect { url: default }
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
        // Disconnect a Connect again.
        if self.page != Page::Servers {
            self.page = self.came_from;
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
            Page::Home | Page::Watch | Page::Waiting | Page::Server | Page::Tournaments | Page::Tournament | Page::Profile => Outcome::Nothing,
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

    fn form() -> OnlineForm {
        OnlineForm::new("ws://127.0.0.1:8080".to_string(), NsgFormat::Startup)
    }

    fn lobby(id: &str, format: NsgFormat) -> LobbyInfo {
        LobbyInfo { id: id.into(), name: id.into(), format, mode: QueueMode::Ranked, password: false, rated: true, bots: Vec::new(), players: 0, seeking: 0 }
    }

    /// Connected to a server with two lobbies, as a joiner.
    fn attached() -> OnlineForm {
        let mut form = form();
        form.apply(Intent::Open(Page::Servers));
        assert_eq!(form.apply(Intent::Go), Outcome::Connect { url: "ws://127.0.0.1:8080".to_string() });
        form.apply(Intent::Waiting("Connecting…".to_string()));
        assert_eq!(form.apply(Intent::Attached { lobbies: vec![lobby("startup", NsgFormat::Startup), lobby("standard", NsgFormat::Standard)], hosting: false }), Outcome::Redraw);
        assert_eq!(form.page, Page::Server);
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
        assert_eq!(form.page, Page::Server, "opened while attached, it leads back to the server");
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
        assert_eq!(form.apply(Intent::ConnectDefault), Outcome::Nothing, "only from Home");
        form.apply(Intent::Back);
        assert_eq!(form.apply(Intent::ConnectDefault), Outcome::Connect { url: "ws://club.example.net:8080".to_string() });
        form.apply(Intent::Waiting("Connecting…".to_string()));
        form.apply(Intent::Failed("refused".to_string()));
        assert_eq!(form.page, Page::Home, "a Default that does not answer leaves the person at Home");
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
        form.apply(Intent::LobbyJoined(lobby("standard", NsgFormat::Standard)));
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
        assert_eq!(form.page, Page::Server);
    }

    #[test]
    fn escape_walks_back_one_page_and_then_off_the_screen() {
        let mut form = form();
        form.apply(Intent::Open(Page::Servers));
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

    /// Attached, the Server page lists the lobbies; one is joined by its
    /// row, which asks for the decks legal in its format; a friend queue by
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
        let Outcome::Seek(Chair::Corp(corp), Opponent::Person) = form.apply(Intent::Seek) else { panic!("the Corp chair with its deck") };
        assert_eq!(corp.id, "brick_stack");
        form.apply(Intent::SetCorpDeck(1));
        form.apply(Intent::SetChair(ChairChoice::Either));
        form.apply(Intent::SetPrefer(Some(Side::Runner)));
        assert_eq!(form.apply(Intent::SetOpponent(Some(Level::Elite))), Outcome::Nothing, "a Ranked queue offers no bot");
        let Outcome::Seek(Chair::Either { corp, runner, prefer }, Opponent::Person) = form.apply(Intent::Seek) else { panic!("either chair, both decks") };
        assert_eq!((corp.id.as_str(), runner.id.as_str(), prefer), ("glyph_of_warding", "stolen_goods", Some(Side::Runner)));
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

    /// A friend queue is made from its own page, for the format of the
    /// lobby the person is in, with or without a password and bots; the
    /// server's answer lands the person in it.
    #[test]
    fn a_friend_queue_is_made_from_its_page() {
        let mut form = attached();
        form.apply(Intent::JoinLobby(1));
        form.apply(Intent::LobbyJoined(lobby("standard", NsgFormat::Standard)));
        form.apply(Intent::Open(Page::MakeLobby));
        assert_eq!((form.page, form.make.format), (Page::MakeLobby, NsgFormat::Standard));
        assert_eq!(form.apply(Intent::Go), Outcome::Redraw, "a lobby needs a name");
        form.apply(Intent::Typed(Field::LobbyName, " Friday ".to_string()));
        form.apply(Intent::SetFormat(NsgFormat::Startup));
        form.apply(Intent::Typed(Field::LobbyPassword, "swordfish".to_string()));
        form.apply(Intent::SetLobbyBots(true));
        assert_eq!(form.apply(Intent::Go), Outcome::CreateLobby { name: "Friday".into(), format: NsgFormat::Startup, password: Some("swordfish".into()), bots: true });
        let made = LobbyInfo { mode: QueueMode::Friends, password: true, rated: false, bots: Level::ALL.to_vec(), ..lobby("K7M2QX", NsgFormat::Startup) };
        assert_eq!(form.apply(Intent::LobbyJoined(made.clone())), Outcome::Decks(NsgFormat::Startup));
        assert_eq!((form.page, form.server.as_ref().unwrap().lobby.as_ref()), (Page::Server, Some(&made)));
        // A bot is asked for only where the lobby offers its rung, and is
        // let go in a lobby that offers none.
        form.set_decks(vec![deck("brick_stack")]);
        assert_eq!(form.apply(Intent::SetOpponent(Some(Level::Veteran))), Outcome::Redraw);
        assert_eq!(form.apply(Intent::Seek), Outcome::Seek(Chair::Corp(Box::new(deck("brick_stack"))), Opponent::Bot { level: Level::Veteran, style: None }));
        form.apply(Intent::SeekCancelled);
        form.apply(Intent::LobbyJoined(lobby("startup", NsgFormat::Startup)));
        assert_eq!(form.server.as_ref().unwrap().bot, None);
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
        assert_eq!((form.page, form.server.is_none(), form.notice.is_some()), (Page::Servers, true, true));
    }

    /// The line over Find a game says what the lobby offers and, once
    /// the server has answered, what this person stands to gain there —
    /// an unrated lobby says so whatever the standing, and the host's own
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
        host.apply(Intent::Attached { lobbies: vec![LobbyInfo { rated: false, ..lobby("startup", NsgFormat::Startup) }], hosting: true });
        host.apply(Intent::LobbyJoined(LobbyInfo { rated: false, ..lobby("startup", NsgFormat::Startup) }));
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
