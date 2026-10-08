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

use netrunner_client::hosting::{normalize_address, Reach, DEFAULT_PORT};
use netrunner_core::decks::DeckFile;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;
use netrunner_server::protocol::{Chair, LobbyInfo};
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
            Field::LobbyName => 40,
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
    CreateLobby { name: String, format: NsgFormat, closed: bool, password: Option<String> },
    Seek(Chair),
    CancelSeek,
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
}

impl ServerState {
    fn new(address: String, hosting: bool, lobbies: Vec<LobbyInfo>) -> Self {
        ServerState { address, hosting, lobbies, lobby: None, seeking: None, decks: Vec::new(), chair: ChairChoice::Corp, corp_deck: 0, runner_deck: 0, link: None }
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
            make: MakeLobby { name: String::new(), format, closed: false, password: String::new() },
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
            self.page = Page::Server;
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
                Page::MakeLobby => {
                    self.page = Page::Server;
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
                    _ => self.format = format,
                }
                Outcome::Redraw
            }
            Intent::SetClosed(closed) => {
                self.make.closed = closed;
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
                Outcome::ListLobbies
            }
            Intent::SetChair(chair) => match &mut self.server {
                Some(server) if server.seeking.is_none() => {
                    server.chair = chair;
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
                if matches!(self.page, Page::Waiting | Page::Server | Page::MakeLobby) {
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
                Outcome::CreateLobby { name: self.make.name.clone(), format: self.make.format, closed: self.make.closed, password }
            }
            Page::Home | Page::Watch | Page::Waiting | Page::Server => Outcome::Nothing,
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
        LobbyInfo { id: id.into(), name: id.into(), format, permanent: true, closed: false, password: false, players: 0, seeking: 0 }
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
        assert_eq!(form.apply(Intent::Go), Outcome::CreateLobby { name: "Friday".into(), format: NsgFormat::Startup, closed: true, password: Some("swordfish".into()) });
        let made = LobbyInfo { closed: true, password: true, permanent: false, ..lobby("K7M2QX", NsgFormat::Startup) };
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

    #[test]
    fn a_servers_own_lobby_is_named_by_its_format_alone() {
        assert_eq!(lobby_title(&lobby("startup", NsgFormat::Startup)), "Startup");
        assert_eq!(lobby_title(&LobbyInfo { name: "Friday".into(), permanent: false, ..lobby("K7M2QX", NsgFormat::Standard) }), "Friday · Standard");
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
