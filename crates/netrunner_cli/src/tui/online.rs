//! Play Online: the main menu's way to a game against a person.
//!
//! **Host** runs a human-vs-human `netrunner_server::Server` inside this
//! process and then attaches to it like any other client, over loopback —
//! so the host plays through a masked `ClientView` exactly as their
//! opponent does, and the process holding the real `GameState` is the
//! server task, not the player's screen. **Join** attaches to a host (or a
//! public `netrunner_server --serve` daemon) by address, or to a host by
//! the ticket it gave out (`netrunner_client::peer`). **Watch** lists a
//! server's matches and spectates one.
//!
//! **Attached, the screen is the Server page** (Phase 4 §7 stage 4d, the
//! desktop's Server page as a list of rows): the lobbies, each a row to
//! join; the list again; a friend queue of the person's own, made on a
//! form of its own; a friend queue's code and password; and in a lobby the
//! game to look for — the chair, the preference when either, a bot where
//! the lobby offers one, a deck per side the chair brings, Find a game and
//! Stop looking. The connection outlives the game: the board leads back
//! here, still attached and back in the lobby, and the next game is a
//! chair and a deck away. **A player brings their own deck** on the seek
//! (`Chair`), and its side is their seat; the server checks it against
//! the lobby's format and refuses an illegal one. There is no "let the
//! host deal": a server deals nobody a deck (Phase 4 §7 stage 3).
//!
//! **Tournaments are rows under the Server page** (Phase 4 §7 stage 6b):
//! the server's tournaments listed, one opened to its page — its code,
//! its entrants by name and key, and for a key that has one the two decks
//! to lock in and Register, or Withdraw — and one held from a form of its
//! own, by the key this connection proved. The statement a registration
//! signs and the salt it is kept under are the connection driver's
//! (`remote::Attached::register`); this screen chooses the decks. **The
//! rounds are the same page**: once begun, the standings stand where the
//! entrants did, each table is a line with its result, a paired player
//! sits at their table from a row (a seek, under the Server page's
//! `seeking`), and the organizer's rows — the next round, a result for a
//! table nobody played, the end — are drawn for the key that holds it.
//!
//! **Nothing here waits on the network with the keyboard dead.** The
//! connection runs as a task (`remote::spawn`) that the menu polls every
//! frame (`tick`), so every wait draws a line and Esc abandons it — which
//! closes the socket, so the daemon drops the waiter instead of pairing an
//! opponent with someone who has gone. The one short blocking call — a
//! `ListMatches` round trip — is bounded and runs under `block_in_place`.
//!
//! Hot-seat play on one screen is deliberately absent: it would show each
//! player the other's hidden cards.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use netrunner_core::cards::CardRegistry;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;
use netrunner_server::protocol::swiss::Outcome as TableOutcome;
use netrunner_server::protocol::{Chair, LadderRow, Level, LobbyInfo, Opponent, Profile, TournamentInfo, TournamentState};
use netrunner_server::serve::{ServeOptions, Server};
use netrunner_server::MatchSummary;

use netrunner_client::connection::{Goal, Link, Who};
use netrunner_client::hosting::{self, normalize_address, Invitation, Reach, Way};
use netrunner_client::identity::{ProfileHere, PublicKey};
use netrunner_client::online;
use netrunner_client::servers::{self as server_list, ServerBook, ServerEdit};
use netrunner_client::tournament;
use netrunner_client::remote::{Attached, AttachedEvent};
use netrunner_client::settings::format_name;
use netrunner_core::decks::DeckFile;
use netrunner_client::peer::Relay;
use crate::remote::{self, ConnectEvent, Connecting, Joined};

pub use netrunner_client::hosting::DEFAULT_PORT;

/// How long a blocking call here may hold the screen.
const BLOCKING_TIMEOUT: Duration = Duration::from_secs(3);

/// What one key (or one tick) did, for the menu.
pub enum OnlineStep {
    Continue,
    Back,
    /// A seat or a spectator's place is ready: play it.
    Play { joined: Box<Joined> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormKind {
    Host,
    Join,
}

/// The fields of the Host and Join forms, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Address,
    Port,
    Reach,
    Format,
    Go,
}

impl FormKind {
    fn fields(self) -> &'static [Field] {
        match self {
            FormKind::Join => &[Field::Address, Field::Go],
            FormKind::Host => &[Field::Port, Field::Reach, Field::Format, Field::Go],
        }
    }
}

#[derive(Debug, Clone)]
struct Form {
    kind: FormKind,
    cursor: usize,
    address: String,
    port: String,
    /// Who the hosted game is for: this machine, the network, or the
    /// internet by way of the router (`hosting::Reach`).
    reach: Reach,
    /// The format a hosted game offers: its server's one lobby.
    format: NsgFormat,
    /// The text field being typed into.
    editing: Option<Field>,
}

impl Form {
    fn field(&self) -> Field {
        self.kind.fields()[self.cursor]
    }

    fn text_mut(&mut self, field: Field) -> Option<&mut String> {
        match field {
            Field::Address => Some(&mut self.address),
            Field::Port => Some(&mut self.port),
            _ => None,
        }
    }
}

/// Which chair a game is looked for in. `Either` brings a deck of each
/// side and the server picks at pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChairChoice {
    Corp,
    Runner,
    Either,
}

impl ChairChoice {
    const ALL: [ChairChoice; 3] = [ChairChoice::Corp, ChairChoice::Runner, ChairChoice::Either];

    fn label(self) -> &'static str {
        match self {
            ChairChoice::Corp => "Corp",
            ChairChoice::Runner => "Runner",
            ChairChoice::Either => "either",
        }
    }

    fn step(self, back: bool) -> ChairChoice {
        let at = Self::ALL.iter().position(|chair| *chair == self).expect("every chair is listed");
        let len = Self::ALL.len();
        Self::ALL[if back { (at + len - 1) % len } else { (at + 1) % len }]
    }
}

/// A row of the Server page, top to bottom: what the cursor can rest on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    Lobby(usize),
    Refresh,
    Make,
    Code,
    CodePassword,
    JoinByCode,
    /// The server's tournaments, on a page of their own.
    Tournaments,
    /// What this server keeps of the person, and the person's servers.
    Profile,
    Servers,
    Chair,
    /// Either chair's preferred side.
    Prefer,
    /// A person, or a bot at a rung the lobby offers.
    Opponent,
    CorpDeck,
    RunnerDeck,
    /// Find a game, or stop looking.
    Seek,
    Leave,
    Disconnect,
}

/// The Server page's text fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerField {
    Code,
    CodePassword,
}

/// A row of the person's servers (Phase 7 §14): each server, then a
/// connection to an address or a ticket once, then Add.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServersRow {
    Server(usize),
    Once,
    Add,
}

/// The person's servers (`netrunner_client::servers`), what each met one
/// keeps of them, and the Server page it was opened from while attached.
#[derive(Debug, Clone)]
struct ServersPage {
    book: ServerBook,
    cursor: usize,
    /// Each met server's answer about the person, by address.
    profiles: BTreeMap<String, Vec<String>>,
    page: Option<ServerPage>,
}

impl ServersPage {
    fn rows(&self) -> Vec<ServersRow> {
        (0..self.book.servers.len()).map(ServersRow::Server).chain([ServersRow::Once, ServersRow::Add]).collect()
    }
}

/// Adding a server (`original` none), or editing one: Name, Address,
/// Save.
#[derive(Debug, Clone)]
struct EditServerForm {
    original: Option<String>,
    name: String,
    address: String,
    cursor: usize,
    editing: bool,
}

impl EditServerForm {
    const ROWS: usize = 3;
}

/// A server's answer about the person, for its row: its address, and
/// what it keeps of them or why it could not say.
type ProfileAnswer = (String, Result<Option<Profile>, String>);

/// Which of a profile's awards the chooser lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AwardKind {
    Title,
    Badge,
}

/// The profile page's rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProfileRow {
    Title,
    Badge,
    Ladder,
    Back,
}

const PROFILE_ROWS: [ProfileRow; 4] = [ProfileRow::Title, ProfileRow::Badge, ProfileRow::Ladder, ProfileRow::Back];

/// What the server attached to keeps of the person (Phase 4 §10): the
/// standing, the title and badge worn — each changed from a list of what
/// was earned that scrolls, a pick worn at once — and a format's ladder.
#[derive(Debug, Clone)]
struct ProfilePage {
    page: ServerPage,
    cursor: usize,
    /// The list open, and the row under its cursor (0 is None).
    choosing: Option<(AwardKind, usize)>,
    ladder_format: NsgFormat,
    ladder: Option<Vec<LadderRow>>,
}

impl ProfilePage {
    fn profile(&self) -> Option<&Profile> {
        match &self.page.profile {
            ProfileHere::Known(profile) => Some(profile),
            _ => None,
        }
    }

    fn earned(&self, kind: AwardKind) -> Vec<String> {
        self.profile()
            .map(|profile| match kind {
                AwardKind::Title => &profile.titles,
                AwardKind::Badge => &profile.badges,
            })
            .map(|earned| earned.iter().map(|award| award.name.clone()).collect())
            .unwrap_or_default()
    }
}

/// Attached: the page the connection is drawn on.
#[derive(Debug, Clone)]
struct ServerPage {
    hosting: bool,
    lobbies: Vec<LobbyInfo>,
    /// The lobby this connection is in.
    lobby: Option<LobbyInfo>,
    /// Looking for a game, at this position in the queue.
    seeking: Option<usize>,
    /// The decks legal in the lobby's format, both sides.
    decks: Vec<DeckFile>,
    chair: ChairChoice,
    /// Either chair's preferred side, if any (Phase 4 §9).
    prefer: Option<Side>,
    /// A bot to play instead of whoever the queue pairs, at a rung the
    /// lobby offers; `None` is a person.
    bot: Option<Level>,
    /// Which of each side's decks.
    corp_deck: usize,
    runner_deck: usize,
    cursor: usize,
    code: String,
    code_password: String,
    editing: Option<ServerField>,
    /// The link's status line while it is down.
    link: Option<String>,
    /// This person's standing at the server, as it last answered: asked
    /// on attaching and after every game.
    profile: ProfileHere,
    /// The key this connection proved, as the server named it in that
    /// answer: whose a tournament entry is, and who may hold one.
    key: Option<PublicKey>,
}

impl ServerPage {
    fn new(hosting: bool, lobbies: Vec<LobbyInfo>) -> Self {
        ServerPage {
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
            cursor: 0,
            code: String::new(),
            code_password: String::new(),
            editing: None,
            link: None,
            profile: ProfileHere::Unasked,
            key: None,
        }
    }

    fn rows(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = (0..self.lobbies.len()).map(Row::Lobby).collect();
        rows.extend([Row::Refresh, Row::Make, Row::Code, Row::CodePassword, Row::JoinByCode, Row::Tournaments]);
        // A hosted server keeps nothing of anyone.
        if !self.hosting {
            rows.push(Row::Profile);
        }
        rows.push(Row::Servers);
        if self.lobby.is_some() {
            rows.push(Row::Chair);
            if self.chair == ChairChoice::Either {
                rows.push(Row::Prefer);
            }
            if self.lobby.as_ref().is_some_and(|lobby| !lobby.bots.is_empty()) {
                rows.push(Row::Opponent);
            }
            if self.chair != ChairChoice::Runner {
                rows.push(Row::CorpDeck);
            }
            if self.chair != ChairChoice::Corp {
                rows.push(Row::RunnerDeck);
            }
            rows.push(Row::Seek);
            rows.push(Row::Leave);
        }
        rows.push(Row::Disconnect);
        rows
    }

    fn side_decks(&self, side: Side) -> Vec<&DeckFile> {
        self.decks.iter().filter(|deck| deck.side == side).collect()
    }

    fn chosen(&self, side: Side) -> Option<&DeckFile> {
        match side {
            Side::Corp => self.side_decks(Side::Corp).get(self.corp_deck).copied(),
            Side::Runner => self.side_decks(Side::Runner).get(self.runner_deck).copied(),
        }
    }

    /// The chair to look for a game in with its deck or decks — or which
    /// side has no legal deck to bring.
    fn chair(&self) -> Result<Chair, Side> {
        let deck = |side: Side| self.chosen(side).cloned().map(Box::new).ok_or(side);
        Ok(match self.chair {
            ChairChoice::Corp => Chair::Corp(deck(Side::Corp)?),
            ChairChoice::Runner => Chair::Runner(deck(Side::Runner)?),
            ChairChoice::Either => Chair::Either { corp: deck(Side::Corp)?, runner: deck(Side::Runner)?, prefer: self.prefer },
        })
    }

    /// Whom the seek is against: the bot chosen if the lobby offers its
    /// rung, else a person.
    fn opponent(&self) -> Opponent {
        match (self.bot, &self.lobby) {
            (Some(level), Some(lobby)) if lobby.bots.contains(&level) => Opponent::Bot { level, style: None },
            _ => Opponent::Person,
        }
    }

    /// The next preference along, or the one before.
    fn step_prefer(&mut self, back: bool) {
        let all = [None, Some(Side::Corp), Some(Side::Runner)];
        let at = all.iter().position(|prefer| *prefer == self.prefer).unwrap_or(0);
        self.prefer = all[if back { (at + 2) % 3 } else { (at + 1) % 3 }];
    }

    /// The next opponent along — a person, then each rung the lobby
    /// offers — or the one before.
    fn step_opponent(&mut self, back: bool) {
        let all: Vec<Option<Level>> = std::iter::once(None).chain(self.lobby.iter().flat_map(|lobby| lobby.bots.iter().copied().map(Some))).collect();
        let at = all.iter().position(|bot| *bot == self.bot).unwrap_or(0);
        let len = all.len();
        self.bot = all[if back { (at + len - 1) % len } else { (at + 1) % len }];
    }

    /// The decks legal in the lobby's format, read afresh: each side's
    /// chosen deck stays chosen if it is still among them.
    fn set_decks(&mut self, decks: Vec<DeckFile>, preferred: &[Option<String>; 2]) {
        let keep = |chosen: Option<&DeckFile>, side: Side| {
            let among: Vec<&DeckFile> = decks.iter().filter(|deck| deck.side == side).collect();
            let position = |id: &str| among.iter().position(|deck| deck.id == id);
            chosen.and_then(|deck| position(&deck.id)).or_else(|| preferred[side as usize].as_deref().and_then(position)).unwrap_or(0)
        };
        let (corp, runner) = (keep(self.chosen(Side::Corp), Side::Corp), keep(self.chosen(Side::Runner), Side::Runner));
        self.decks = decks;
        self.corp_deck = corp;
        self.runner_deck = runner;
    }

    /// The cursor put on `row`, where the page draws it.
    fn rest_on(&mut self, row: Row) {
        if let Some(at) = self.rows().iter().position(|r| *r == row) {
            self.cursor = at;
        }
    }
}

/// The form for a friend queue of the person's own: casual, and joined
/// by its code.
#[derive(Debug, Clone)]
struct MakeForm {
    name: String,
    format: NsgFormat,
    /// Its members may ask for a bot.
    bots: bool,
    password: String,
    cursor: usize,
    editing: Option<usize>,
}

impl MakeForm {
    /// Name, format, bots, password, Make.
    const ROWS: usize = 5;
    const PASSWORD: usize = 3;
}

/// The server's tournaments, listed (Phase 4 §7 stage 6b).
#[derive(Debug, Clone)]
struct TournamentsPage {
    list: Vec<TournamentInfo>,
    /// Whether the list is the server's answer, so an empty one can say
    /// "none" rather than nothing while the answer is on its way.
    listed: bool,
    cursor: usize,
}

/// A row of the tournaments page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TournamentsRow {
    Entry(usize),
    Refresh,
    Hold,
}

impl TournamentsPage {
    fn rows(&self) -> Vec<TournamentsRow> {
        let mut rows: Vec<TournamentsRow> = (0..self.list.len()).map(TournamentsRow::Entry).collect();
        rows.extend([TournamentsRow::Refresh, TournamentsRow::Hold]);
        rows
    }

    /// The server's list, read afresh: the cursor stays on its row.
    fn set(&mut self, list: Vec<TournamentInfo>) {
        self.list = list;
        self.listed = true;
        self.cursor = self.cursor.min(self.rows().len() - 1);
    }

    /// One tournament as the server now reports it, into the list — in
    /// its place if it is listed, at the end if it is new.
    fn put(&mut self, info: &TournamentInfo) {
        match self.list.iter_mut().find(|listed| listed.id == info.id) {
            Some(listed) => *listed = info.clone(),
            None => self.list.push(info.clone()),
        }
    }
}

/// One tournament's page: the two decks this player would lock in, from
/// those legal in its format, and the rows under them.
#[derive(Debug, Clone)]
struct TournamentPage {
    info: TournamentInfo,
    /// The decks legal in the tournament's format, both sides.
    decks: Vec<DeckFile>,
    corp_deck: usize,
    runner_deck: usize,
    cursor: usize,
}

/// A row of a tournament's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TournamentRow {
    CorpDeck,
    RunnerDeck,
    /// Register, or register again with the decks shown.
    Register,
    Withdraw,
    /// Take the seat at this round's table — or, seated and waiting for
    /// the opponent, stand up again.
    Sit,
    StandUp,
    /// Offer the opponent an intentional draw; theirs back is the tie.
    OfferDraw,
    /// Leave mid-event: the results stand, the key is paired no more.
    Drop,
    /// The organizer's: the next round, the end, a result for a table
    /// nobody played.
    BeginRound,
    Finish,
    Record(usize, TableOutcome),
    Refresh,
}

impl TournamentPage {
    /// The rows a key can act on: a registration's rows only while the
    /// tournament takes them and this client has a key to sign one with;
    /// in the rounds, the seat for a key paired this round, and the
    /// organizer's rows for the key that holds it (`seeking` is the Server
    /// page's: a seat taken is a seek).
    fn rows(&self, key: Option<&PublicKey>, seeking: bool) -> Vec<TournamentRow> {
        let info = &self.info;
        let mut rows = Vec::new();
        let organizer = tournament::is_organizer(info, key);
        match info.state {
            TournamentState::Registering => {
                if key.is_some() {
                    rows.extend([TournamentRow::CorpDeck, TournamentRow::RunnerDeck, TournamentRow::Register]);
                    if tournament::entry_of(info, key).is_some() {
                        rows.push(TournamentRow::Withdraw);
                    }
                }
                if organizer && info.entrants.len() >= 2 {
                    rows.push(TournamentRow::BeginRound);
                }
            }
            TournamentState::Playing { .. } => {
                if tournament::my_table(info, key).is_some_and(|(_, _, table)| table.result.is_none()) {
                    rows.push(if seeking { TournamentRow::StandUp } else { TournamentRow::Sit });
                }
                if tournament::may_offer_draw(info, key) {
                    rows.push(TournamentRow::OfferDraw);
                }
                if tournament::may_drop(info, key) {
                    rows.push(TournamentRow::Drop);
                }
                if let Some(current) = info.current_round().filter(|_| organizer) {
                    if current.complete() {
                        rows.extend([TournamentRow::BeginRound, TournamentRow::Finish]);
                    }
                    for (index, table) in current.tables.iter().enumerate() {
                        if table.result.is_none() {
                            rows.extend([TableOutcome::CorpWon, TableOutcome::RunnerWon, TableOutcome::Tie].map(|outcome| TournamentRow::Record(index, outcome)));
                        }
                    }
                }
            }
            TournamentState::Finished | TournamentState::Cancelled => {}
        }
        rows.push(TournamentRow::Refresh);
        rows
    }

    fn side_decks(&self, side: Side) -> Vec<&DeckFile> {
        self.decks.iter().filter(|deck| deck.side == side).collect()
    }

    fn chosen(&self, side: Side) -> Option<&DeckFile> {
        match side {
            Side::Corp => self.side_decks(Side::Corp).get(self.corp_deck).copied(),
            Side::Runner => self.side_decks(Side::Runner).get(self.runner_deck).copied(),
        }
    }

    /// Steps the chosen deck for `side`, round the list.
    fn step(&mut self, side: Side, back: bool) {
        let len = self.side_decks(side).len();
        if len == 0 {
            return;
        }
        let at = match side {
            Side::Corp => &mut self.corp_deck,
            Side::Runner => &mut self.runner_deck,
        };
        *at = if back { (*at + len - 1) % len } else { (*at + 1) % len };
    }
}

/// The form for a tournament of this key's own: a name and a format.
#[derive(Debug, Clone)]
struct MakeTournamentForm {
    name: String,
    format: NsgFormat,
    cursor: usize,
    editing: bool,
}

impl MakeTournamentForm {
    /// Name, format, Hold.
    const ROWS: usize = 3;
}

/// The in-process server while this player hosts, and what it gives out.
/// Dropping it stops the server — the match, if one is running, ends with
/// it — releases the router's mapping and closes the ticket's endpoint.
struct Hosting {
    task: tokio::task::JoinHandle<std::io::Result<()>>,
    invitation: Invitation,
}

impl Hosting {
    fn poll(&mut self) {
        self.invitation.poll();
    }
}

impl Drop for Hosting {
    fn drop(&mut self) {
        self.task.abort();
    }
}

enum Mode {
    Home { cursor: usize },
    Form(Form),
    /// Dialling or hosting, until attached. `back` is the form to reopen
    /// on a failure.
    Connecting { status: String, back: Box<Mode> },
    Server(ServerPage),
    MakeLobby { page: ServerPage, form: MakeForm },
    PickDeck { page: ServerPage, side: Side, cursor: usize },
    /// The server's tournaments, under the Server page, which Esc returns
    /// to.
    Tournaments { page: ServerPage, list: TournamentsPage },
    /// One tournament's page, under the list.
    /// Boxed: the page carries the tournament whole, and clippy's
    /// `large_enum_variant` is right that the enum should not.
    Tournament { page: ServerPage, list: TournamentsPage, tournament: Box<TournamentPage> },
    MakeTournament { page: ServerPage, list: TournamentsPage, form: MakeTournamentForm },
    /// The person's servers, and the form a server is added or edited on.
    Servers(Box<ServersPage>),
    EditServer { list: Box<ServersPage>, form: EditServerForm },
    /// What the server attached to keeps of the person.
    Profile(Box<ProfilePage>),
    /// A spectator waiting for a place.
    Waiting { connecting: Connecting, status: String, back: Box<Mode> },
    Watch { address: String, editing: bool, matches: Vec<MatchSummary>, cursor: usize },
}

const HOME: [(&str, &str); 3] = [
    ("Host a game", "Run a game on this machine and give your opponent the address or the ticket"),
    ("Join a server", "Your servers by name, one the Default this screen connects to; or a host's address or ticket, once"),
    ("Watch a game", "List a server's matches and spectate one"),
];

pub struct OnlineScreen {
    mode: Mode,
    /// Where the decks are read from, and what they are checked against,
    /// each time a lobby is joined: the decks legal in its format.
    decks_dir: PathBuf,
    registry: CardRegistry,
    player: String,
    format: NsgFormat,
    /// The address Join and Watch start from: `--server`, as the flag path
    /// uses it — which the settings file fills with the last server joined
    /// — and, once a connection is made here, that server.
    default_address: String,
    /// The address of the last connection made and not yet kept: the menu
    /// takes it after every step and writes it to the settings file, since
    /// this screen has no settings path of its own. Taken rather than
    /// written here so a test screen never touches a real file.
    used_address: Option<String>,
    /// The connection, for as long as the person stays attached — through
    /// the game and back.
    attached: Option<Attached>,
    hosting: Option<Hosting>,
    /// The host's lines: every address to give out, refreshed each tick
    /// while the router and the relay answer.
    hosting_lines: String,
    notice: Option<String>,
    /// The relay a hosted game's ticket goes through (`Settings::relay`),
    /// or why the setting could not be read — which only hosting for the
    /// internet needs to know, so it is kept rather than refused at the
    /// door.
    relay: Result<Relay, String>,
    /// Where the player's key lives (`netrunner_client::identity`). A
    /// server joined proves it; `None` plays unrated. A game hosted here
    /// never does: it is never rated.
    identity_dir: Option<std::path::PathBuf>,
    /// The active deck a side, which a lobby's and a tournament's choice
    /// start on (`with_preferred`).
    preferred: [Option<String>; 2],
    /// The address the connection went to, while attached to a server of
    /// the person's (not one hosted here).
    attached_url: Option<String>,
    /// The servers' answers for their rows, as each comes.
    profile_answers: Option<mpsc::Receiver<ProfileAnswer>>,
}

impl OnlineScreen {
    /// `format` is the one Settings names, where Host and Make a lobby
    /// start; the decks offered in a lobby are those legal in its own.
    pub fn open(
        decks_dir: &std::path::Path,
        registry: &CardRegistry,
        format: NsgFormat,
        player: String,
        default_address: String,
        relay: Result<Relay, String>,
    ) -> Result<Self, String> {
        Ok(OnlineScreen {
            mode: Mode::Home { cursor: 0 },
            decks_dir: decks_dir.to_path_buf(),
            registry: registry.clone(),
            player,
            format,
            default_address,
            used_address: None,
            attached: None,
            hosting: None,
            hosting_lines: String::new(),
            notice: None,
            relay,
            identity_dir: None,
            preferred: [None, None],
            attached_url: None,
            profile_answers: None,
        })
    }

    /// The person's list of servers, beside their key; `None` with no
    /// data directory.
    fn servers_path(&self) -> Option<PathBuf> {
        self.identity_dir.as_deref().map(server_list::path_in)
    }

    fn book(&self) -> ServerBook {
        self.servers_path().and_then(|path| ServerBook::load(&path).ok()).unwrap_or_default()
    }

    /// The list of servers, each met one asked what it keeps of the person
    /// (each on its own socket, no key made to look); the one attached
    /// answers on its own connection. `page` is the Server page it was
    /// opened from, which Esc returns to.
    fn servers_mode(&mut self, page: Option<ServerPage>) -> Mode {
        let book = self.book();
        let attached = page.as_ref().filter(|page| !page.hosting).and(self.attached_url.clone());
        let asked: Vec<String> = book.servers.iter().filter(|server| server.key.is_some() && Some(&server.address) != attached.as_ref()).map(|server| server.address.clone()).collect();
        let credentials = self.identity_dir.as_deref().and_then(|dir| netrunner_client::identity::existing(dir).ok().flatten());
        let mut profiles = BTreeMap::new();
        match (tokio::runtime::Handle::try_current(), credentials) {
            (Ok(runtime), Some(credentials)) if !asked.is_empty() => {
                let (tx, rx) = mpsc::channel();
                for address in asked {
                    profiles.insert(address.clone(), vec!["Asking…".to_string()]);
                    let (tx, credentials) = (tx.clone(), credentials.clone());
                    runtime.spawn(async move {
                        let answer = match tokio::time::timeout(Duration::from_secs(10), netrunner_client::remote::profile(&address, &credentials)).await {
                            Ok(Ok(profile)) => Ok(profile),
                            Ok(Err(error)) => Err(error.to_string()),
                            Err(_) => Err("No answer within 10s".to_string()),
                        };
                        let _ = tx.send((address, answer));
                    });
                }
                self.profile_answers = Some(rx);
            }
            _ => {}
        }
        Mode::Servers(Box::new(ServersPage { book, cursor: 0, profiles, page }))
    }

    /// Connects to `url` by the person's key, `back` the page a failure
    /// reopens. One connection at a time: whatever was attached is let go.
    fn connect(&mut self, url: String, back: Mode) {
        // A key file that is there and unreadable is shown, rather than
        // playing a game that quietly counts for nothing.
        let credentials = match &self.identity_dir {
            Some(dir) => match netrunner_client::identity::Credentials::in_dir(dir) {
                Ok(credentials) => Some(Box::new(credentials)),
                Err(error) => {
                    self.notice = Some(format!("Your key: {error}"));
                    self.mode = back;
                    return;
                }
            },
            None => None,
        };
        self.disconnect();
        self.used(&url);
        self.attached_url = Some(url.clone());
        let who = Who { player_name: self.player.clone(), credentials };
        self.attached = Some(remote::spawn(url.clone(), Goal::Attach(who)));
        self.mode = Mode::Connecting { status: format!("Connecting to {url}…"), back: Box::new(back) };
    }

    /// The Default, connected to as the screen opens (Phase 7 §14), when
    /// there is one; Home otherwise.
    pub fn connect_default(mut self) -> Self {
        if let Some(address) = self.book().default_server().map(|server| server.address.clone()) {
            self.connect(address, Mode::Home { cursor: 1 });
        }
        self
    }

    /// A change to the person's servers, written, and the list as the file
    /// now has it.
    fn edit_servers(&self, edit: &ServerEdit) -> Result<ServerBook, String> {
        let path = self.servers_path().ok_or("No data directory, so the list of servers cannot be kept")?;
        ServerBook::update(&path, |book| edit.apply(book))?;
        ServerBook::load(&path)
    }

    pub fn with_identity_dir(mut self, dir: Option<std::path::PathBuf>) -> Self {
        self.identity_dir = dir;
        self
    }

    /// The active deck a side (`Settings::active_deck`, Phase 7 §16):
    /// what a lobby's or a tournament's deck starts on.
    pub fn with_preferred(mut self, preferred: [Option<String>; 2]) -> Self {
        self.preferred = preferred;
        self
    }

    /// Where a side's choice starts among `decks`: the active deck if it
    /// is among them, else the first.
    fn preferred_index(&self, decks: &[DeckFile], side: Side) -> usize {
        let among: Vec<&DeckFile> = decks.iter().filter(|deck| deck.side == side).collect();
        self.preferred[side as usize].as_deref().and_then(|id| among.iter().position(|deck| deck.id == id)).unwrap_or(0)
    }

    /// A connection was made to `url`: the next form starts from it, and
    /// the menu keeps it (Phase 6 §3).
    fn used(&mut self, url: &str) {
        self.default_address = url.to_string();
        self.used_address = Some(url.to_string());
    }

    /// The address of a connection made since the last call, for the
    /// settings file.
    pub fn take_used_address(&mut self) -> Option<String> {
        self.used_address.take()
    }

    fn form(&self, kind: FormKind) -> Form {
        Form { kind, cursor: 0, address: self.default_address.clone(), port: DEFAULT_PORT.to_string(), reach: Reach::Network, format: self.format, editing: None }
    }

    /// Back from a game: the Server page again while the connection is
    /// attached — the next game is found where this one was — and Home
    /// otherwise.
    pub fn returned(&mut self) {
        if self.attached.is_some()
            && let Mode::Server(_) | Mode::Tournament { .. } = self.mode
        {
            return;
        }
        self.disconnect();
        self.mode = Mode::Home { cursor: 0 };
    }

    /// Lets the connection go, and the hosted server with it.
    fn disconnect(&mut self) {
        self.attached = None;
        self.attached_url = None;
        self.hosting = None;
        self.hosting_lines.clear();
    }

    /// The decks legal in `format`, built-in and saved.
    fn deck_choices(&self, format: NsgFormat) -> Vec<DeckFile> {
        online::deck_choices(&self.decks_dir, &self.registry, format)
    }

    /// Polls the connection. Called every frame, key or no key.
    pub fn tick(&mut self) -> OnlineStep {
        // The router answers seconds after the server is up, so the host's
        // lines follow it rather than being written once.
        if let Some(hosting) = &mut self.hosting {
            hosting.poll();
            self.hosting_lines = hosting_status(hosting);
        }
        // The servers' answers for their rows.
        let answers: Vec<_> = self.profile_answers.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        if let Mode::Servers(list) | Mode::EditServer { list, .. } = &mut self.mode {
            for (address, answer) in answers {
                let lines = match answer {
                    Ok(profile) => netrunner_client::identity::profile_lines(profile.as_ref()),
                    Err(reason) => vec![reason],
                };
                list.profiles.insert(address, lines);
            }
        }
        if let Some(step) = self.tick_attached() {
            return step;
        }
        let Mode::Waiting { connecting, status, .. } = &mut self.mode else { return OnlineStep::Continue };
        let mut outcome = None;
        while let Some(event) = connecting.poll() {
            match event {
                ConnectEvent::Queued(_) => {}
                ConnectEvent::Link(link) => {
                    if let Some(line) = link.status_line(std::time::Instant::now()) {
                        *status = line;
                    }
                }
                other => {
                    outcome = Some(other);
                    break;
                }
            }
        }
        let Some(outcome) = outcome else { return OnlineStep::Continue };
        let Mode::Waiting { back, .. } = std::mem::replace(&mut self.mode, Mode::Home { cursor: 0 }) else { unreachable!("checked above") };
        match outcome {
            ConnectEvent::Joined(joined) => OnlineStep::Play { joined },
            ConnectEvent::Failed(error) => {
                self.notice = Some(error.to_string());
                self.mode = *back;
                OnlineStep::Continue
            }
            ConnectEvent::Queued(_) | ConnectEvent::Link(_) => unreachable!("handled in the loop"),
        }
    }

    /// The attached connection's answers, each drawn on the Server page;
    /// a game found is played.
    fn tick_attached(&mut self) -> Option<OnlineStep> {
        let hosting = self.hosting.is_some();
        loop {
            let event = self.attached.as_mut()?.poll()?;
            match event {
                AttachedEvent::Attached(lobbies) => {
                    // The standing is asked on attaching and after every
                    // game, so the page shows the number the game moved.
                    self.attached.as_ref()?.profile();
                    match self.page_mut() {
                        // A reattach: the list is fresh, the rest stands.
                        Some(page) => page.lobbies = lobbies,
                        None => {
                            let page = ServerPage::new(hosting, lobbies);
                            // A hosted server has one lobby, the format the
                            // host chose: the host is put in it, so the
                            // game is a chair and a deck away.
                            if let (true, Some(only)) = (hosting, page.lobbies.first()) {
                                self.attached.as_ref()?.join_lobby(only.id.clone(), None);
                            }
                            self.mode = Mode::Server(page);
                            self.notice = None;
                        }
                    }
                }
                AttachedEvent::Lobbies(lobbies) => {
                    if let Some(page) = self.page_mut() {
                        page.lobbies = lobbies;
                    }
                }
                AttachedEvent::Tournaments(tournaments) => self.tournaments_listed(tournaments),
                AttachedEvent::Tournament(info) => self.tournament_answered(info),
                AttachedEvent::TournamentRefused(reason) => self.notice = Some(reason),
                // The terminal's event pages are Phase 4 §11's stage 2:
                // today it climbs one from `--event`, and an answer
                // pushed to the menu is a notice.
                AttachedEvent::Events { .. } => {}
                AttachedEvent::Event(info) => self.notice = Some(format!("{}: {}", info.name, netrunner_client::event::standing_line(&info))),
                AttachedEvent::EventRefused(reason) | AttachedEvent::AdminRefused(reason) => self.notice = Some(reason),
                // The menu asks no operator's question; `netrunner_cli admin` does.
                AttachedEvent::Admin(_) => {}
                AttachedEvent::LobbyJoined(lobby) => {
                    let decks = self.deck_choices(lobby.format);
                    let preferred = self.preferred.clone();
                    if let Mode::MakeLobby { page, .. } = &mut self.mode {
                        let page = std::mem::replace(page, ServerPage::new(hosting, Vec::new()));
                        self.mode = Mode::Server(page);
                    }
                    if let Some(page) = self.page_mut() {
                        // The list predates the join: the row is brought
                        // level with what the server just said.
                        if let Some(row) = page.lobbies.iter_mut().find(|row| row.id == lobby.id) {
                            *row = lobby.clone();
                        }
                        // A bot chosen in a lobby that offered one is not
                        // carried into one that does not.
                        if page.bot.is_some_and(|level| !lobby.bots.contains(&level)) {
                            page.bot = None;
                        }
                        page.lobby = Some(lobby);
                        page.set_decks(decks, &preferred);
                        page.rest_on(Row::Seek);
                    }
                    self.notice = None;
                }
                AttachedEvent::LobbyLeft => {
                    if let Some(page) = self.page_mut() {
                        page.lobby = None;
                        page.seeking = None;
                        page.cursor = 0;
                    }
                }
                AttachedEvent::LobbyRefused(reason) | AttachedEvent::SeekRefused(reason) | AttachedEvent::ProfileRefused(reason) => self.notice = Some(reason),
                AttachedEvent::Ladder { format, rows } => {
                    if let Mode::Profile(page) = &mut self.mode
                        && page.ladder_format == format
                    {
                        page.ladder = Some(rows);
                    }
                }
                AttachedEvent::Queued(position) => {
                    if let Some(page) = self.page_mut() {
                        page.seeking = Some(position);
                    }
                }
                AttachedEvent::SeekCancelled => {
                    if let Some(page) = self.page_mut() {
                        page.seeking = None;
                    }
                }
                AttachedEvent::BackInLobby(lobby) => {
                    self.attached.as_ref()?.profile();
                    if let Some(page) = self.page_mut() {
                        page.seeking = None;
                        if lobby.is_some() {
                            page.lobby = lobby;
                        }
                    }
                }
                AttachedEvent::Profile { key, profile } => {
                    if let Some(page) = self.page_mut() {
                        page.key = key;
                        page.profile = ProfileHere::from_reply(key, profile);
                    }
                }
                AttachedEvent::Link(Link::Down(error)) => {
                    self.notice = Some(error.to_string());
                    let back = match std::mem::replace(&mut self.mode, Mode::Home { cursor: 0 }) {
                        Mode::Connecting { back, .. } => *back,
                        _ if hosting => Mode::Form(self.form(FormKind::Host)),
                        _ => self.servers_mode(None),
                    };
                    self.disconnect();
                    self.mode = back;
                    return None;
                }
                AttachedEvent::Link(link) => {
                    if let Some(page) = self.page_mut() {
                        page.link = link.status_line(std::time::Instant::now());
                    }
                }
                AttachedEvent::Joined(joined) => return Some(OnlineStep::Play { joined }),
            }
        }
    }

    fn page_mut(&mut self) -> Option<&mut ServerPage> {
        match &mut self.mode {
            Mode::Server(page)
            | Mode::MakeLobby { page, .. }
            | Mode::PickDeck { page, .. }
            | Mode::Tournaments { page, .. }
            | Mode::Tournament { page, .. }
            | Mode::MakeTournament { page, .. } => Some(page),
            Mode::Profile(profile) => Some(&mut profile.page),
            Mode::Servers(list) | Mode::EditServer { list, .. } => list.page.as_mut(),
            _ => None,
        }
    }

    /// The server's list of tournaments: read into whichever of the
    /// tournament pages is up, and the open tournament brought level with
    /// its entry in it.
    fn tournaments_listed(&mut self, tournaments: Vec<TournamentInfo>) {
        match &mut self.mode {
            Mode::Tournaments { list, .. } | Mode::MakeTournament { list, .. } => list.set(tournaments),
            Mode::Tournament { list, tournament, .. } => {
                if let Some(info) = tournaments.iter().find(|info| info.id == tournament.info.id) {
                    tournament.info = info.clone();
                }
                list.set(tournaments);
            }
            _ => {}
        }
    }

    /// One tournament as the server now reports it, after making,
    /// entering or leaving one: the one just made opens its page; the one
    /// open is brought level; the list learns it either way.
    fn tournament_answered(&mut self, info: TournamentInfo) {
        self.notice = None;
        match std::mem::replace(&mut self.mode, Mode::Home { cursor: 0 }) {
            // The form's answer is a tournament the list has never seen; a
            // push about another, while the form is up, is put in the list
            // and leaves the form alone.
            Mode::MakeTournament { page, mut list, form } if list.list.iter().any(|listed| listed.id == info.id) => {
                list.put(&info);
                self.mode = Mode::MakeTournament { page, list, form };
            }
            Mode::MakeTournament { page, mut list, .. } => {
                list.put(&info);
                let decks = self.deck_choices(info.format);
                let (corp_deck, runner_deck) = (self.preferred_index(&decks, Side::Corp), self.preferred_index(&decks, Side::Runner));
                self.mode = Mode::Tournament { page, list, tournament: Box::new(TournamentPage { info, decks, corp_deck, runner_deck, cursor: 0 }) };
            }
            Mode::Tournament { page, mut list, mut tournament } => {
                list.put(&info);
                if tournament.info.id == info.id {
                    tournament.info = info;
                }
                self.mode = Mode::Tournament { page, list, tournament };
            }
            Mode::Tournaments { page, mut list } => {
                list.put(&info);
                self.mode = Mode::Tournaments { page, list };
            }
            other => self.mode = other,
        }
    }

    pub fn key(&mut self, key: KeyCode) -> OnlineStep {
        self.notice = None;
        match std::mem::replace(&mut self.mode, Mode::Home { cursor: 0 }) {
            Mode::Home { mut cursor } => match key {
                KeyCode::Up | KeyCode::Char('k') => self.mode = Mode::Home { cursor: (cursor + HOME.len() - 1) % HOME.len() },
                KeyCode::Down | KeyCode::Char('j') => self.mode = Mode::Home { cursor: (cursor + 1) % HOME.len() },
                KeyCode::Enter => {
                    self.mode = match cursor {
                        0 => Mode::Form(self.form(FormKind::Host)),
                        1 => self.servers_mode(None),
                        _ => Mode::Watch { address: self.default_address.clone(), editing: true, matches: Vec::new(), cursor: 0 },
                    };
                }
                KeyCode::Esc | KeyCode::Char('q') => return OnlineStep::Back,
                _ => {
                    cursor %= HOME.len();
                    self.mode = Mode::Home { cursor };
                }
            },
            Mode::Form(form) => return self.form_key(form, key),
            Mode::Connecting { status, back } => {
                if matches!(key, KeyCode::Esc | KeyCode::Char('q')) {
                    // Dropping the connection closes the socket with a
                    // `Close`, and the server with it.
                    self.disconnect();
                    self.mode = *back;
                } else {
                    self.mode = Mode::Connecting { status, back };
                }
            }
            Mode::Server(page) => self.server_key(page, key),
            Mode::MakeLobby { page, form } => self.make_key(page, form, key),
            Mode::Tournaments { page, list } => self.tournaments_key(page, list, key),
            Mode::Tournament { page, list, tournament } => self.tournament_key(page, list, tournament, key),
            Mode::MakeTournament { page, list, form } => self.make_tournament_key(page, list, form, key),
            Mode::Servers(list) => self.servers_key(list, key),
            Mode::EditServer { list, form } => self.edit_server_key(list, form, key),
            Mode::Profile(page) => self.profile_key(page, key),
            Mode::PickDeck { mut page, side, mut cursor } => {
                let len = page.side_decks(side).len().max(1);
                match key {
                    KeyCode::Up | KeyCode::Char('k') => {
                        cursor = (cursor + len - 1) % len;
                        self.mode = Mode::PickDeck { page, side, cursor };
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        cursor = (cursor + 1) % len;
                        self.mode = Mode::PickDeck { page, side, cursor };
                    }
                    KeyCode::Enter => {
                        match side {
                            Side::Corp => page.corp_deck = cursor,
                            Side::Runner => page.runner_deck = cursor,
                        }
                        self.mode = Mode::Server(page);
                    }
                    _ => self.mode = Mode::Server(page),
                }
            }
            Mode::Waiting { connecting, status, back } => {
                if matches!(key, KeyCode::Esc | KeyCode::Char('q')) {
                    drop(connecting);
                    self.mode = *back;
                } else {
                    self.mode = Mode::Waiting { connecting, status, back };
                }
            }
            Mode::Watch { mut address, mut editing, matches, mut cursor } => {
                if editing {
                    match key {
                        KeyCode::Char(c) if !c.is_control() => address.push(c),
                        KeyCode::Backspace => {
                            address.pop();
                        }
                        KeyCode::Esc => return OnlineStep::Back,
                        KeyCode::Enter => return self.fetch_matches(address),
                        _ => {}
                    }
                    self.mode = Mode::Watch { address, editing, matches, cursor };
                    return OnlineStep::Continue;
                }
                match key {
                    KeyCode::Up | KeyCode::Char('k') if !matches.is_empty() => cursor = (cursor + matches.len() - 1) % matches.len(),
                    KeyCode::Down | KeyCode::Char('j') if !matches.is_empty() => cursor = (cursor + 1) % matches.len(),
                    KeyCode::Char('r') => return self.fetch_matches(address),
                    KeyCode::Char('a') => editing = true,
                    KeyCode::Enter if !matches.is_empty() => {
                        let url = normalize_address(&address);
                        self.used(&url);
                        let match_id = matches[cursor].match_id;
                        let back = Box::new(Mode::Watch { address, editing, matches, cursor });
                        self.mode = Mode::Waiting { connecting: remote::watch(url.clone(), match_id), status: format!("Connecting to {url}…"), back };
                        return OnlineStep::Continue;
                    }
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.mode = Mode::Home { cursor: 2 };
                        return OnlineStep::Continue;
                    }
                    _ => {}
                }
                self.mode = Mode::Watch { address, editing, matches, cursor };
            }
        }
        OnlineStep::Continue
    }

    fn form_key(&mut self, mut form: Form, key: KeyCode) -> OnlineStep {
        if let Some(field) = form.editing {
            match key {
                KeyCode::Char(c) if !c.is_control() && (field != Field::Port || c.is_ascii_digit()) => {
                    form.text_mut(field).expect("an editable field").push(c);
                }
                KeyCode::Backspace => {
                    form.text_mut(field).expect("an editable field").pop();
                }
                KeyCode::Enter | KeyCode::Esc | KeyCode::Tab | KeyCode::Down => {
                    form.editing = None;
                    if matches!(key, KeyCode::Tab | KeyCode::Down) {
                        form.cursor = (form.cursor + 1) % form.kind.fields().len();
                    }
                }
                _ => {}
            }
            self.mode = Mode::Form(form);
            return OnlineStep::Continue;
        }
        let len = form.kind.fields().len();
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Home { cursor: if form.kind == FormKind::Host { 0 } else { 1 } };
                return OnlineStep::Continue;
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => form.cursor = (form.cursor + len - 1) % len,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => form.cursor = (form.cursor + 1) % len,
            KeyCode::Left if form.field() == Field::Reach => form.reach = form.reach.step(true),
            KeyCode::Right | KeyCode::Char(' ') if form.field() == Field::Reach => form.reach = form.reach.step(false),
            KeyCode::Left if form.field() == Field::Format => form.format = step_format(form.format, true),
            KeyCode::Right | KeyCode::Char(' ') if form.field() == Field::Format => form.format = step_format(form.format, false),
            KeyCode::Enter => match form.field() {
                Field::Address | Field::Port => form.editing = Some(form.field()),
                Field::Reach => form.reach = form.reach.step(false),
                Field::Format => form.format = step_format(form.format, false),
                Field::Go => return self.go(form),
            },
            _ => {}
        }
        self.mode = Mode::Form(form);
        OnlineStep::Continue
    }

    /// Submits a form: attaches to the address, or starts the server and
    /// attaches to it.
    fn go(&mut self, form: Form) -> OnlineStep {
        // Joining is connecting by the key; hosting proves none, since a
        // game on this machine is never rated.
        let credentials = None;
        let (url, status) = match form.kind {
            FormKind::Join => {
                if form.address.trim().is_empty() {
                    self.notice = Some("Type the server's address, or paste a host's ticket".to_string());
                    self.mode = Mode::Form(form);
                    return OnlineStep::Continue;
                }
                let url = normalize_address(&form.address);
                self.connect(url, Mode::Form(form));
                return OnlineStep::Continue;
            }
            FormKind::Host => {
                let Ok(port) = form.port.parse::<u16>() else {
                    self.notice = Some(format!("{:?} is not a port number", form.port));
                    self.mode = Mode::Form(form);
                    return OnlineStep::Continue;
                };
                // The ticket on the person's own key, so it is the same
                // every time they host and a friend can save it (Phase 7
                // §15); a key that cannot be made hosts on a throwaway one.
                let key = self.identity_dir.as_deref().and_then(|dir| netrunner_client::identity::Credentials::in_dir(dir).ok()).map(|credentials| credentials.identity.secret_bytes());
                match start_hosting(port, form.reach, form.format, &self.relay, key) {
                    Ok((hosting, local_url)) => {
                        self.hosting_lines = hosting_status(&hosting);
                        self.hosting = Some(hosting);
                        (local_url, "Hosting — connecting to your own server…".to_string())
                    }
                    Err(error) => {
                        self.notice = Some(format!("Could not host on port {port}: {error}"));
                        self.mode = Mode::Form(form);
                        return OnlineStep::Continue;
                    }
                }
            }
        };
        let who = Who { player_name: self.player.clone(), credentials };
        self.attached_url = None;
        self.attached = Some(remote::spawn(url, Goal::Attach(who)));
        self.mode = Mode::Connecting { status, back: Box::new(Mode::Form(form)) };
        OnlineStep::Continue
    }

    /// A key on the Server page.
    fn server_key(&mut self, mut page: ServerPage, key: KeyCode) {
        if let Some(field) = page.editing {
            let text = match field {
                ServerField::Code => &mut page.code,
                ServerField::CodePassword => &mut page.code_password,
            };
            match key {
                KeyCode::Char(c) if !c.is_control() => text.push(c),
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Enter | KeyCode::Esc | KeyCode::Tab | KeyCode::Down => {
                    page.editing = None;
                    if matches!(key, KeyCode::Tab | KeyCode::Down) {
                        page.cursor = (page.cursor + 1) % page.rows().len();
                    }
                }
                _ => {}
            }
            self.mode = Mode::Server(page);
            return;
        }
        let rows = page.rows();
        let len = rows.len();
        page.cursor = page.cursor.min(len - 1);
        let row = rows[page.cursor];
        let Some(attached) = &self.attached else {
            self.disconnect();
            self.mode = Mode::Home { cursor: 0 };
            return;
        };
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                let hosting = page.hosting;
                self.disconnect();
                self.mode = if hosting { Mode::Form(self.form(FormKind::Host)) } else { self.servers_mode(None) };
                return;
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => page.cursor = (page.cursor + len - 1) % len,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => page.cursor = (page.cursor + 1) % len,
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if row == Row::Chair && page.seeking.is_none() => page.chair = page.chair.step(key == KeyCode::Left),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if row == Row::Prefer && page.seeking.is_none() => page.step_prefer(key == KeyCode::Left),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if row == Row::Opponent && page.seeking.is_none() => page.step_opponent(key == KeyCode::Left),
            KeyCode::Char('r') => attached.list_lobbies(),
            KeyCode::Enter => match row {
                Row::Lobby(index) => {
                    let lobby = &page.lobbies[index];
                    if page.lobby.as_ref().is_none_or(|current| current.id != lobby.id) {
                        let password = lobby.password.then(|| page.code_password.clone()).filter(|password| !password.is_empty());
                        attached.join_lobby(lobby.id.clone(), password);
                    }
                }
                Row::Refresh => attached.list_lobbies(),
                Row::Make => {
                    let format = page.lobby.as_ref().or(page.lobbies.first()).map_or(self.format, |lobby| lobby.format);
                    let form = MakeForm { name: String::new(), format, bots: false, password: String::new(), cursor: 0, editing: None };
                    self.mode = Mode::MakeLobby { page, form };
                    return;
                }
                Row::Code => page.editing = Some(ServerField::Code),
                Row::CodePassword => page.editing = Some(ServerField::CodePassword),
                Row::JoinByCode => {
                    if page.code.trim().is_empty() {
                        self.notice = Some("Type the lobby's code first".to_string());
                    } else {
                        attached.join_lobby(page.code.trim().to_string(), Some(page.code_password.clone()).filter(|password| !password.is_empty()));
                    }
                }
                Row::Tournaments => {
                    attached.list_tournaments();
                    self.mode = Mode::Tournaments { page, list: TournamentsPage { list: Vec::new(), listed: false, cursor: 0 } };
                    return;
                }
                Row::Chair => {
                    if page.seeking.is_none() {
                        page.chair = page.chair.step(false);
                    }
                }
                Row::Prefer => {
                    if page.seeking.is_none() {
                        page.step_prefer(false);
                    }
                }
                Row::Opponent => {
                    if page.seeking.is_none() {
                        page.step_opponent(false);
                    }
                }
                Row::CorpDeck | Row::RunnerDeck => {
                    if page.seeking.is_none() {
                        let side = if row == Row::CorpDeck { Side::Corp } else { Side::Runner };
                        let cursor = if side == Side::Corp { page.corp_deck } else { page.runner_deck };
                        if page.side_decks(side).is_empty() {
                            self.notice = Some(format!("No {side:?} deck is legal in this lobby's format: build one under Decks"));
                        } else {
                            self.mode = Mode::PickDeck { page, side, cursor };
                            return;
                        }
                    }
                }
                Row::Seek => {
                    if page.seeking.is_some() {
                        attached.cancel_seek();
                    } else {
                        match page.chair() {
                            Ok(chair) => attached.seek(chair, page.opponent()),
                            Err(side) => self.notice = Some(format!("No {side:?} deck is legal in this lobby's format: build one under Decks")),
                        }
                    }
                }
                Row::Leave => {
                    if page.seeking.is_none() {
                        attached.leave_lobby();
                    }
                }
                Row::Disconnect => {
                    let hosting = page.hosting;
                    self.disconnect();
                    self.mode = if hosting { Mode::Form(self.form(FormKind::Host)) } else { self.servers_mode(None) };
                    return;
                }
                Row::Profile => {
                    if let ProfileHere::Known(_) = page.profile {
                        let ladder_format = page.lobby.as_ref().map_or(self.format, |lobby| lobby.format);
                        attached.ladder(ladder_format);
                        self.mode = Mode::Profile(Box::new(ProfilePage { page, cursor: 0, choosing: None, ladder_format, ladder: None }));
                        return;
                    }
                    self.notice = Some(match page.profile {
                        ProfileHere::NoKey => "You connected without a key, so this server keeps nothing of you".to_string(),
                        ProfileHere::Unkept => "This server keeps nothing: it rates no game and grants nothing".to_string(),
                        _ => "Asking the server — try again in a moment".to_string(),
                    });
                }
                Row::Servers => {
                    self.mode = self.servers_mode(Some(page));
                    return;
                }
            },
            _ => {}
        }
        self.mode = Mode::Server(page);
    }

    /// A key on the list of servers: Enter connects a row (one at a
    /// time; the one attached is its Server page again), opens the
    /// once-only form, or Add; `d` makes a row the Default or not, `e`
    /// edits it, `x` or Delete removes it.
    fn servers_key(&mut self, mut list: Box<ServersPage>, key: KeyCode) {
        let rows = list.rows();
        let len = rows.len();
        list.cursor = list.cursor.min(len - 1);
        let row = rows[list.cursor];
        let address = match row {
            ServersRow::Server(index) => Some(list.book.servers[index].address.clone()),
            _ => None,
        };
        let edit = match (key, &address) {
            (KeyCode::Char('d'), Some(address)) => {
                let is_default = list.book.default.as_deref() == Some(address.as_str());
                Some(ServerEdit::Default((!is_default).then(|| address.clone())))
            }
            (KeyCode::Char('x') | KeyCode::Delete, Some(address)) => Some(ServerEdit::Remove(address.clone())),
            _ => None,
        };
        if let Some(edit) = edit {
            match self.edit_servers(&edit) {
                Ok(book) => {
                    list.book = book;
                    list.cursor = list.cursor.min(list.rows().len() - 1);
                }
                Err(reason) => self.notice = Some(reason),
            }
            self.mode = Mode::Servers(list);
            return;
        }
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = match list.page {
                    Some(page) => Mode::Server(page),
                    None => Mode::Home { cursor: 1 },
                };
                return;
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => list.cursor = (list.cursor + len - 1) % len,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => list.cursor = (list.cursor + 1) % len,
            KeyCode::Char('e') if address.is_some() => {
                let server = &list.book.servers[list.cursor];
                let form = EditServerForm { original: Some(server.address.clone()), name: server.name.clone(), address: server.address.clone(), cursor: 0, editing: false };
                self.mode = Mode::EditServer { list, form };
                return;
            }
            KeyCode::Enter => match row {
                ServersRow::Server(_) => {
                    let address = address.expect("a server's row");
                    if self.attached_url.as_deref() == Some(address.as_str())
                        && let Some(page) = list.page.take()
                    {
                        self.mode = Mode::Server(page);
                        return;
                    }
                    list.page = None;
                    self.connect(address, Mode::Servers(list));
                    return;
                }
                ServersRow::Once => {
                    self.mode = Mode::Form(self.form(FormKind::Join));
                    return;
                }
                ServersRow::Add => {
                    let form = EditServerForm { original: None, name: String::new(), address: String::new(), cursor: 0, editing: true };
                    self.mode = Mode::EditServer { list, form };
                    return;
                }
            },
            _ => {}
        }
        self.mode = Mode::Servers(list);
    }

    /// A key on the form a server is added or edited on. Save checks the
    /// change against the list first, so a name taken is said here.
    fn edit_server_key(&mut self, mut list: Box<ServersPage>, mut form: EditServerForm, key: KeyCode) {
        if form.editing {
            let text = if form.cursor == 0 { &mut form.name } else { &mut form.address };
            match key {
                KeyCode::Char(c) if !c.is_control() => text.push(c),
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Enter | KeyCode::Esc | KeyCode::Tab | KeyCode::Down => {
                    form.editing = false;
                    if matches!(key, KeyCode::Tab | KeyCode::Down) {
                        form.cursor = (form.cursor + 1) % EditServerForm::ROWS;
                    }
                }
                _ => {}
            }
            self.mode = Mode::EditServer { list, form };
            return;
        }
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Servers(list);
                return;
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => form.cursor = (form.cursor + EditServerForm::ROWS - 1) % EditServerForm::ROWS,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => form.cursor = (form.cursor + 1) % EditServerForm::ROWS,
            KeyCode::Enter if form.cursor < 2 => form.editing = true,
            KeyCode::Enter => {
                let edit = match &form.original {
                    Some(address) => ServerEdit::Edit { address: address.clone(), name: form.name.clone(), new_address: form.address.clone() },
                    None => ServerEdit::Add { name: form.name.clone(), address: form.address.clone() },
                };
                match edit.check(&list.book).and_then(|()| self.edit_servers(&edit)) {
                    Ok(book) => {
                        list.book = book;
                        self.mode = Mode::Servers(list);
                        return;
                    }
                    Err(reason) => self.notice = Some(reason),
                }
            }
            _ => {}
        }
        self.mode = Mode::EditServer { list, form };
    }

    /// A key on the profile page, or on the list of awards open over it:
    /// there ↑ ↓, Page Up/Down, Home and End move, Enter wears the row
    /// at once and Esc closes it.
    fn profile_key(&mut self, mut page: Box<ProfilePage>, key: KeyCode) {
        let Some(attached) = &self.attached else {
            self.disconnect();
            self.mode = Mode::Home { cursor: 1 };
            return;
        };
        if let Some((kind, mut cursor)) = page.choosing {
            let earned = page.earned(kind);
            let last = earned.len();
            match key {
                KeyCode::Up | KeyCode::Char('k') => cursor = cursor.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => cursor = (cursor + 1).min(last),
                KeyCode::PageUp => cursor = cursor.saturating_sub(10),
                KeyCode::PageDown => cursor = (cursor + 10).min(last),
                KeyCode::Home => cursor = 0,
                KeyCode::End => cursor = last,
                KeyCode::Enter => {
                    let picked = cursor.checked_sub(1).map(|index| earned[index].clone());
                    let profile = page.profile().expect("a list opens on a known profile");
                    let (title, badge) = match kind {
                        AwardKind::Title => (picked, profile.badge.clone()),
                        AwardKind::Badge => (profile.title.clone(), picked),
                    };
                    attached.set_profile(title, badge);
                    page.choosing = None;
                    self.mode = Mode::Profile(page);
                    return;
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    page.choosing = None;
                    self.mode = Mode::Profile(page);
                    return;
                }
                _ => {}
            }
            page.choosing = Some((kind, cursor));
            self.mode = Mode::Profile(page);
            return;
        }
        let len = PROFILE_ROWS.len();
        let row = PROFILE_ROWS[page.cursor.min(len - 1)];
        let step_ladder = |page: &mut ProfilePage, back: bool| {
            page.ladder_format = step_format(page.ladder_format, back);
            page.ladder = None;
            attached.ladder(page.ladder_format);
        };
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Server(page.page);
                return;
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => page.cursor = (page.cursor + len - 1) % len,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => page.cursor = (page.cursor + 1) % len,
            KeyCode::Left if row == ProfileRow::Ladder => step_ladder(&mut page, true),
            KeyCode::Right | KeyCode::Char(' ') if row == ProfileRow::Ladder => step_ladder(&mut page, false),
            KeyCode::Enter => match row {
                ProfileRow::Title | ProfileRow::Badge => {
                    let kind = if row == ProfileRow::Title { AwardKind::Title } else { AwardKind::Badge };
                    let earned = page.earned(kind);
                    if earned.is_empty() {
                        self.notice = Some("None earned here yet: events and the server's operator grant them".to_string());
                    } else {
                        let worn = page.profile().and_then(|profile| if kind == AwardKind::Title { profile.title.clone() } else { profile.badge.clone() });
                        let cursor = worn.and_then(|worn| earned.iter().position(|name| *name == worn)).map_or(0, |index| index + 1);
                        page.choosing = Some((kind, cursor));
                    }
                }
                ProfileRow::Ladder => step_ladder(&mut page, false),
                ProfileRow::Back => {
                    self.mode = Mode::Server(page.page);
                    return;
                }
            },
            _ => {}
        }
        self.mode = Mode::Profile(page);
    }

    /// A key on the Make a lobby form.
    fn make_key(&mut self, page: ServerPage, mut form: MakeForm, key: KeyCode) {
        if let Some(field) = form.editing {
            let text = if field == 0 { &mut form.name } else { &mut form.password };
            match key {
                KeyCode::Char(c) if !c.is_control() => text.push(c),
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Enter | KeyCode::Esc | KeyCode::Tab | KeyCode::Down => {
                    form.editing = None;
                    if matches!(key, KeyCode::Tab | KeyCode::Down) {
                        form.cursor = (form.cursor + 1) % MakeForm::ROWS;
                    }
                }
                _ => {}
            }
            self.mode = Mode::MakeLobby { page, form };
            return;
        }
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Server(page);
                return;
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => form.cursor = (form.cursor + MakeForm::ROWS - 1) % MakeForm::ROWS,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => form.cursor = (form.cursor + 1) % MakeForm::ROWS,
            KeyCode::Left if form.cursor == 1 => form.format = step_format(form.format, true),
            KeyCode::Right | KeyCode::Char(' ') if form.cursor == 1 => form.format = step_format(form.format, false),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.cursor == 2 => form.bots = !form.bots,
            KeyCode::Enter => match form.cursor {
                0 => form.editing = Some(0),
                1 => form.format = step_format(form.format, false),
                2 => form.bots = !form.bots,
                MakeForm::PASSWORD => form.editing = Some(MakeForm::PASSWORD),
                _ => {
                    if form.name.trim().is_empty() {
                        self.notice = Some("Give the friend queue a name".to_string());
                    } else if let Some(attached) = &self.attached {
                        let password = Some(form.password.trim().to_string()).filter(|password| !password.is_empty());
                        attached.create_lobby(form.name.trim().to_string(), form.format, password, form.bots);
                    }
                }
            },
            _ => {}
        }
        self.mode = Mode::MakeLobby { page, form };
    }

    /// A key on the tournaments page.
    fn tournaments_key(&mut self, page: ServerPage, mut list: TournamentsPage, key: KeyCode) {
        let rows = list.rows();
        let len = rows.len();
        list.cursor = list.cursor.min(len - 1);
        let Some(attached) = &self.attached else {
            self.disconnect();
            self.mode = Mode::Home { cursor: 0 };
            return;
        };
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Server(page);
                return;
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => list.cursor = (list.cursor + len - 1) % len,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => list.cursor = (list.cursor + 1) % len,
            KeyCode::Char('r') => attached.list_tournaments(),
            KeyCode::Enter => match rows[list.cursor] {
                TournamentsRow::Entry(index) => {
                    let info = list.list[index].clone();
                    let decks = self.deck_choices(info.format);
                    let (corp_deck, runner_deck) = (self.preferred_index(&decks, Side::Corp), self.preferred_index(&decks, Side::Runner));
                    self.mode = Mode::Tournament { page, list, tournament: Box::new(TournamentPage { info, decks, corp_deck, runner_deck, cursor: 0 }) };
                    return;
                }
                TournamentsRow::Refresh => attached.list_tournaments(),
                TournamentsRow::Hold => {
                    if page.key.is_none() {
                        self.notice = Some("A tournament is held by a key, and this client has none: connect with one first".to_string());
                    } else {
                        let format = page.lobby.as_ref().map_or(self.format, |lobby| lobby.format);
                        self.mode = Mode::MakeTournament { page, list, form: MakeTournamentForm { name: String::new(), format, cursor: 0, editing: false } };
                        return;
                    }
                }
            },
            _ => {}
        }
        self.mode = Mode::Tournaments { page, list };
    }

    /// A key on a tournament's page.
    fn tournament_key(&mut self, page: ServerPage, list: TournamentsPage, mut tournament: Box<TournamentPage>, key: KeyCode) {
        let rows = tournament.rows(page.key.as_ref(), page.seeking.is_some());
        let len = rows.len();
        tournament.cursor = tournament.cursor.min(len - 1);
        let row = rows[tournament.cursor];
        let Some(attached) = &self.attached else {
            self.disconnect();
            self.mode = Mode::Home { cursor: 0 };
            return;
        };
        let side_of = |row: TournamentRow| match row {
            TournamentRow::CorpDeck => Some(Side::Corp),
            TournamentRow::RunnerDeck => Some(Side::Runner),
            _ => None,
        };
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Tournaments { page, list };
                return;
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => tournament.cursor = (tournament.cursor + len - 1) % len,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => tournament.cursor = (tournament.cursor + 1) % len,
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if side_of(row).is_some() => tournament.step(side_of(row).expect("a deck row"), key == KeyCode::Left),
            KeyCode::Char('r') => attached.list_tournaments(),
            KeyCode::Enter => match row {
                TournamentRow::CorpDeck | TournamentRow::RunnerDeck => tournament.step(side_of(row).expect("a deck row"), false),
                TournamentRow::Register => match (tournament.chosen(Side::Corp), tournament.chosen(Side::Runner)) {
                    (Some(corp), Some(runner)) => attached.register(tournament.info.id.clone(), corp.clone(), runner.clone()),
                    (None, _) => self.notice = Some(format!("No Corp deck is legal in {}: build one under Decks", format_name(tournament.info.format))),
                    (_, None) => self.notice = Some(format!("No Runner deck is legal in {}: build one under Decks", format_name(tournament.info.format))),
                },
                TournamentRow::Withdraw | TournamentRow::Drop => attached.unregister(tournament.info.id.clone()),
                // The seat is taken with the table's seed commitment as this
                // page read it (Phase 4 §7 stage 6c).
                TournamentRow::Sit => match tournament::my_seed_commitment(&tournament.info, page.key.as_ref()) {
                    Some(commitment) => attached.sit(tournament.info.id.clone(), commitment),
                    None => self.notice = Some("This table has no seed commitment yet: read the tournament again".to_string()),
                },
                TournamentRow::StandUp => attached.cancel_seek(),
                TournamentRow::OfferDraw => attached.offer_draw(tournament.info.id.clone()),
                TournamentRow::BeginRound => attached.begin_round(tournament.info.id.clone()),
                TournamentRow::Finish => attached.finish_tournament(tournament.info.id.clone()),
                TournamentRow::Record(table, outcome) => attached.record_result(tournament.info.id.clone(), table, outcome),
                TournamentRow::Refresh => attached.list_tournaments(),
            },
            _ => {}
        }
        self.mode = Mode::Tournament { page, list, tournament };
    }

    /// A key on the Hold a tournament form.
    fn make_tournament_key(&mut self, page: ServerPage, list: TournamentsPage, mut form: MakeTournamentForm, key: KeyCode) {
        if form.editing {
            match key {
                KeyCode::Char(c) if !c.is_control() => form.name.push(c),
                KeyCode::Backspace => {
                    form.name.pop();
                }
                KeyCode::Enter | KeyCode::Esc | KeyCode::Tab | KeyCode::Down => {
                    form.editing = false;
                    if matches!(key, KeyCode::Tab | KeyCode::Down) {
                        form.cursor = (form.cursor + 1) % MakeTournamentForm::ROWS;
                    }
                }
                _ => {}
            }
            self.mode = Mode::MakeTournament { page, list, form };
            return;
        }
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Tournaments { page, list };
                return;
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => form.cursor = (form.cursor + MakeTournamentForm::ROWS - 1) % MakeTournamentForm::ROWS,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => form.cursor = (form.cursor + 1) % MakeTournamentForm::ROWS,
            KeyCode::Left if form.cursor == 1 => form.format = step_format(form.format, true),
            KeyCode::Right | KeyCode::Char(' ') if form.cursor == 1 => form.format = step_format(form.format, false),
            KeyCode::Enter => match form.cursor {
                0 => form.editing = true,
                1 => form.format = step_format(form.format, false),
                _ => {
                    if form.name.trim().is_empty() {
                        self.notice = Some("Give the tournament a name".to_string());
                    } else if let Some(attached) = &self.attached {
                        attached.create_tournament(form.name.trim().to_string(), form.format);
                    }
                }
            },
            _ => {}
        }
        self.mode = Mode::MakeTournament { page, list, form };
    }

    fn fetch_matches(&mut self, address: String) -> OnlineStep {
        let url = normalize_address(&address);
        self.used(&url);
        let matches = match block_on_bounded(remote::list_matches(&url)) {
            Some(Ok((matches, _, _))) => {
                if matches.is_empty() {
                    self.notice = Some(format!("{url} is hosting no matches right now (r to refresh)"));
                }
                matches
            }
            Some(Err(error)) => {
                self.notice = Some(error.to_string());
                self.mode = Mode::Watch { address, editing: true, matches: Vec::new(), cursor: 0 };
                return OnlineStep::Continue;
            }
            None => {
                self.notice = Some(format!("{url} did not answer within {}s", BLOCKING_TIMEOUT.as_secs()));
                self.mode = Mode::Watch { address, editing: true, matches: Vec::new(), cursor: 0 };
                return OnlineStep::Continue;
            }
        };
        self.mode = Mode::Watch { address, editing: false, matches, cursor: 0 };
        OnlineStep::Continue
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let [body, footer] =
            Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        let help = match &self.mode {
            Mode::Home { cursor } => {
                let items: Vec<ListItem> = HOME
                    .iter()
                    .map(|(label, blurb)| {
                        ListItem::new(vec![
                            Line::from(Span::styled(*label, Style::default().add_modifier(Modifier::BOLD))),
                            Line::from(Span::styled(format!("  {blurb}"), Style::default().fg(Color::Gray))),
                        ])
                    })
                    .collect();
                draw_list(frame, body, &format!("Play Online — as {}", self.player), items, Some(*cursor));
                "Up/Down choose · Enter selects · Esc back"
            }
            Mode::Form(form) => {
                self.draw_form(frame, body, form);
                if form.editing.is_some() { "Type · Enter or Tab finishes" } else { "Up/Down choose · Enter edits or picks · Left/Right steps · Esc back" }
            }
            Mode::Connecting { status, .. } | Mode::Waiting { status, .. } => {
                let mut text: Vec<Line> = status.lines().map(|line| Line::from(line.to_string())).collect();
                text.extend([Line::from(""), Line::from("Esc stops waiting.")]);
                frame.render_widget(
                    Paragraph::new(text).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title("Play Online")),
                    body,
                );
                "Esc stops waiting"
            }
            Mode::Server(page) => {
                self.draw_server(frame, body, page);
                if page.editing.is_some() {
                    "Type · Enter or Tab finishes"
                } else if page.seeking.is_some() {
                    "Up/Down choose · Enter on Looking stops · r lists again · Esc disconnects"
                } else {
                    "Up/Down choose · Enter picks · Left/Right steps the chair · r lists again · Esc disconnects"
                }
            }
            Mode::MakeLobby { form, .. } => {
                let text = |editing: bool, value: &str| if editing { format!("{value}▏") } else { value.to_string() };
                let rows = [
                    format!("Name             {}", text(form.editing == Some(0), &form.name)),
                    format!("Format           ‹ {} ›", capitalised(format_name(form.format))),
                    format!("Bots             ‹ {} ›", if form.bots { "on request — anyone here may ask for one" } else { "people only" }),
                    format!("Password         {}", if form.password.is_empty() && form.editing != Some(MakeForm::PASSWORD) { "(none)".to_string() } else { text(form.editing == Some(MakeForm::PASSWORD), &form.password) }),
                    "[ Make ]".to_string(),
                ];
                let items = rows.into_iter().map(ListItem::new).collect();
                draw_list(frame, body, "Make a friend queue — never listed, never rated, joined by its code", items, Some(form.cursor));
                if form.editing.is_some() { "Type · Enter or Tab finishes" } else { "Up/Down choose · Enter edits, steps or makes · Esc back" }
            }
            Mode::PickDeck { page, side, cursor } => {
                let items = page.side_decks(*side).into_iter().map(|deck| ListItem::new(online::label(deck))).collect();
                draw_list(frame, body, &format!("Your {side:?} deck"), items, Some(*cursor));
                "Up/Down choose · Enter picks · Esc keeps the old choice"
            }
            Mode::Tournaments { list, .. } => {
                let rows: Vec<String> = list
                    .rows()
                    .into_iter()
                    .map(|row| match row {
                        TournamentsRow::Entry(index) => tournament::line(&list.list[index]),
                        TournamentsRow::Refresh => "List the tournaments again".to_string(),
                        TournamentsRow::Hold => "Hold a tournament…".to_string(),
                    })
                    .collect();
                let items = rows.into_iter().map(ListItem::new).collect();
                let title = match (list.listed, list.list.is_empty()) {
                    (false, _) => "Tournaments — asking the server…",
                    (true, true) => "Tournaments — this server holds none",
                    (true, false) => "Tournaments — Enter opens one",
                };
                draw_list(frame, body, title, items, Some(list.cursor.min(list.rows().len() - 1)));
                "Up/Down choose · Enter opens or holds · r lists again · Esc back"
            }
            Mode::Tournament { page, tournament, .. } => {
                self.draw_tournament(frame, body, page, tournament);
                let rows = tournament.rows(page.key.as_ref(), page.seeking.is_some());
                if rows.iter().any(|row| matches!(row, TournamentRow::CorpDeck)) {
                    "Up/Down choose · Left/Right steps a deck · Enter registers or withdraws · r reads again · Esc back"
                } else if rows.len() > 1 {
                    "Up/Down choose · Enter does it · r reads again · Esc back"
                } else {
                    "r reads again · Esc back"
                }
            }
            Mode::MakeTournament { form, .. } => {
                let rows = [
                    format!("Name             {}", if form.editing { format!("{}▏", form.name) } else { form.name.clone() }),
                    format!("Format           ‹ {} ›", capitalised(format_name(form.format))),
                    "[ Hold ]".to_string(),
                ];
                let items = rows.into_iter().map(ListItem::new).collect();
                draw_list(frame, body, "Hold a tournament — run as Null Signal Games run theirs: two decks locked per entrant, Swiss rounds", items, Some(form.cursor));
                if form.editing { "Type · Enter or Tab finishes" } else { "Up/Down choose · Enter edits, steps or holds · Esc back" }
            }
            Mode::Servers(list) => {
                self.draw_servers(frame, body, list);
                "Up/Down choose · Enter connects or opens · d default · e edit · x remove · Esc back"
            }
            Mode::EditServer { form, .. } => {
                let text = |editing: bool, value: &str| if editing { format!("{value}▏") } else { value.to_string() };
                let rows = [
                    format!("Name      {}", text(form.editing && form.cursor == 0, &form.name)),
                    format!("Address   {}", text(form.editing && form.cursor == 1, &form.address)),
                    "[ Save ]".to_string(),
                ];
                let items = rows.into_iter().map(ListItem::new).collect();
                let title = match &form.original {
                    Some(_) => "Edit a server — a new address is another server, and its key is forgotten",
                    None => "Add a server",
                };
                draw_list(frame, body, title, items, Some(form.cursor));
                if form.editing { "Type · Enter or Tab finishes" } else { "Up/Down choose · Enter edits or saves · Esc back" }
            }
            Mode::Profile(page) => {
                self.draw_profile(frame, body, page);
                if page.choosing.is_some() {
                    "Up/Down, Page Up/Down, Home/End move · Enter wears it · Esc closes"
                } else {
                    "Up/Down choose · Enter changes · Left/Right steps the ladder's format · Esc back"
                }
            }
            Mode::Watch { address, editing, matches, cursor } => {
                let [top, list] =
                    Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).areas(body);
                let address = if *editing { format!("{address}▏") } else { address.clone() };
                frame.render_widget(Paragraph::new(address).block(Block::default().borders(Borders::ALL).title("Server")), top);
                let items = matches
                    .iter()
                    .map(|m| {
                        // No decks: a server names none (`MatchSummary`).
                        let lobby = format!("  [{}]", format_name(m.format));
                        ListItem::new(format!("{} (Corp) vs {} (Runner){lobby} — {}s", m.corp, m.runner, m.started_secs_ago))
                    })
                    .collect();
                draw_list(frame, list, "Matches", items, (!matches.is_empty()).then_some(*cursor));
                if *editing { "Type the address · Enter lists its matches · Esc back" } else { "Enter watches · r refreshes · a edits the address · Esc back" }
            }
        };
        let line = match &self.notice {
            Some(notice) => Line::from(Span::styled(notice.clone(), Style::default().fg(Color::Red))),
            None => Line::from(Span::styled(help, Style::default().fg(Color::DarkGray))),
        };
        frame.render_widget(Paragraph::new(line), footer);
    }

    fn draw_form(&self, frame: &mut Frame, area: Rect, form: &Form) {
        let text = |field: Field, value: &str| if form.editing == Some(field) { format!("{value}▏") } else { value.to_string() };
        let rows: Vec<String> = form
            .kind
            .fields()
            .iter()
            .map(|field| match field {
                Field::Address => format!("Address/ticket   {}", text(Field::Address, &form.address)),
                Field::Port => format!("Port             {}", text(Field::Port, &form.port)),
                Field::Reach => format!("Who can join     ‹ {} ›", form.reach.label()),
                Field::Format => format!("Format           ‹ {} ›", capitalised(format_name(form.format))),
                Field::Go => match form.kind {
                    FormKind::Join => "[ Connect ]".to_string(),
                    FormKind::Host => "[ Start hosting ]".to_string(),
                },
            })
            .collect();
        let items = rows.into_iter().map(ListItem::new).collect();
        let title = match form.kind {
            FormKind::Join => "Join a server — its lobbies are listed once you are connected".to_string(),
            FormKind::Host => "Host a game — a human-vs-human server on this machine with one lobby, which you join too".to_string(),
        };
        draw_list(frame, area, &title, items, Some(form.cursor));
    }

    /// The Server page: the host's lines above the rows, when hosting,
    /// and in a lobby whether a game there counts and what this person
    /// stands at.
    fn draw_server(&self, frame: &mut Frame, area: Rect, page: &ServerPage) {
        let mut head: Vec<Line> = Vec::new();
        if let Some(link) = &page.link {
            head.push(Line::from(Span::styled(link.clone(), Style::default().fg(Color::Red))));
        }
        if page.hosting {
            head.extend(self.hosting_lines.lines().map(|line| Line::from(line.to_string())));
        }
        if let Some(lobby) = &page.lobby {
            head.push(Line::from(netrunner_client::identity::lobby_rating_line(lobby.rated, page.hosting, lobby.format, &page.profile)));
        }
        let [top, list] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(if head.is_empty() { 0 } else { head.len() as u16 + 2 }), Constraint::Min(0)])
            .areas(area);
        if !head.is_empty() {
            frame.render_widget(Paragraph::new(head).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title(if page.hosting { "Hosting" } else { "Server" })), top);
        }
        let text = |field: ServerField, value: &str, blank: &str| {
            if page.editing == Some(field) {
                format!("{value}▏")
            } else if value.is_empty() {
                blank.to_string()
            } else {
                value.to_string()
            }
        };
        let rows: Vec<String> = page
            .rows()
            .into_iter()
            .map(|row| match row {
                Row::Lobby(index) => {
                    let lobby = &page.lobbies[index];
                    let here = page.lobby.as_ref().is_some_and(|current| current.id == lobby.id);
                    let mut line = format!("{}  {} — {} here, {} looking", if here { "Here" } else { "Join" }, lobby_title(lobby), lobby.players(), lobby.seeking());
                    if lobby.password {
                        line.push_str(" · password");
                    }
                    line.push_str(if lobby.rated { " · rated" } else { " · unrated" });
                    if !lobby.bots.is_empty() {
                        line.push_str(" · bots on request");
                    }
                    line
                }
                Row::Refresh => "List the lobbies again".to_string(),
                Row::Make => "Make a friend queue…".to_string(),
                Row::Code => format!("A friend queue's code   {}", text(ServerField::Code, &page.code, "(type it)")),
                Row::CodePassword => format!("Its password            {}", text(ServerField::CodePassword, &page.code_password, "(none)")),
                Row::JoinByCode => "[ Join by code ]".to_string(),
                Row::Tournaments => "Tournaments…".to_string(),
                Row::Profile => "Your profile here…".to_string(),
                Row::Servers => "Servers…".to_string(),
                Row::Chair => format!("Chair            ‹ {} ›", page.chair.label()),
                Row::Prefer => format!("Preference       ‹ {} ›", online::preference_label(page.prefer)),
                Row::Opponent => format!("Against          ‹ {} ›", online::opponent_label(page.bot)),
                Row::CorpDeck => format!("Corp deck        {}", page.chosen(Side::Corp).map(online::label).unwrap_or_else(|| "none legal in this lobby's format".to_string())),
                Row::RunnerDeck => format!("Runner deck      {}", page.chosen(Side::Runner).map(online::label).unwrap_or_else(|| "none legal in this lobby's format".to_string())),
                Row::Seek => match page.seeking {
                    Some(position) => format!("Looking for a game — {position} waiting in this queue… (Enter stops)"),
                    None => "[ Find a game ]".to_string(),
                },
                Row::Leave => "[ Leave lobby ]".to_string(),
                Row::Disconnect => "[ Disconnect ]".to_string(),
            })
            .collect();
        let items = rows.into_iter().map(ListItem::new).collect();
        let title = match &page.lobby {
            Some(lobby) => format!("Lobbies — in {}", lobby_title(lobby)),
            None => "Lobbies — join one to look for a game".to_string(),
        };
        draw_list(frame, list, &title, items, Some(page.cursor.min(page.rows().len() - 1)));
    }
}

impl OnlineScreen {
    /// What the person calls the server at `address`: its name in their
    /// list, else the address.
    fn server_name(&self, address: &str) -> String {
        self.book().find(address).map_or_else(|| address.to_string(), |server| server.name.clone())
    }

    /// The list of servers: a row each — its name and tags, its address,
    /// what it keeps of the person — then Connect once and Add; and under
    /// it what this machine knows.
    fn draw_servers(&self, frame: &mut Frame, area: Rect, list: &ServersPage) {
        let local = netrunner_client::identity::key_lines_in(self.identity_dir.as_deref());
        let local: Vec<Line> = local.into_iter().take(2).map(Line::from).collect();
        let [top, bottom] = Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(0), Constraint::Length(local.len() as u16 + 2)]).areas(area);
        let attached = list.page.as_ref().filter(|page| !page.hosting).and(self.attached_url.as_deref());
        let dim = Style::default().fg(Color::Gray);
        let items: Vec<ListItem> = list
            .rows()
            .into_iter()
            .map(|row| match row {
                ServersRow::Server(index) => {
                    let server = &list.book.servers[index];
                    let mut tags = Vec::new();
                    if list.book.default.as_deref() == Some(server.address.as_str()) {
                        tags.push("Default");
                    }
                    if attached == Some(server.address.as_str()) {
                        tags.push("Connected");
                    }
                    let title = if tags.is_empty() { server.name.clone() } else { format!("{} · {}", server.name, tags.join(" · ")) };
                    let mut lines = vec![Line::from(Span::styled(title, Style::default().add_modifier(Modifier::BOLD)))];
                    if server.name != server.address {
                        lines.push(Line::from(Span::styled(format!("  {}", server.address), dim)));
                    }
                    let about: Vec<String> = match (attached == Some(server.address.as_str()), list.page.as_ref(), list.profiles.get(&server.address)) {
                        (true, Some(page), _) => netrunner_client::identity::profile_lines(match &page.profile {
                            ProfileHere::Known(profile) => Some(profile),
                            _ => None,
                        }),
                        (_, _, Some(lines)) => lines.clone(),
                        _ if server.key.is_none() => vec!["Not met yet: what it keeps of you is shown once you have connected".to_string()],
                        _ => Vec::new(),
                    };
                    lines.extend(about.into_iter().map(|line| Line::from(Span::styled(format!("  {line}"), dim))));
                    ListItem::new(lines)
                }
                ServersRow::Once => ListItem::new("Connect once to an address or a host's ticket…"),
                ServersRow::Add => ListItem::new("Add a server…"),
            })
            .collect();
        let title = if list.book.servers.is_empty() { "Servers — none yet; one you connect to is listed here" } else { "Servers" };
        draw_list(frame, top, title, items, Some(list.cursor.min(list.rows().len() - 1)));
        frame.render_widget(Paragraph::new(local).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title("On this machine")), bottom);
    }

    /// The profile page: the standing, the title and badge rows and the
    /// ladder; and over them, while open, the list a title or a badge is
    /// picked from, which scrolls with its cursor.
    fn draw_profile(&self, frame: &mut Frame, area: Rect, page: &ProfilePage) {
        let Some(profile) = page.profile() else { return };
        let mut head: Vec<Line> = netrunner_client::identity::standing_lines(&profile.standings).into_iter().map(Line::from).collect();
        if head.is_empty() {
            head.push(Line::from("No rated game here yet"));
        }
        let ladder: Vec<Line> = match &page.ladder {
            None => vec![Line::from("Asking…")],
            Some(rows) if rows.is_empty() => vec![Line::from(format!("Nobody has {} rated games here yet", netrunner_server::protocol::LADDER_MIN_GAMES))],
            Some(rows) => rows
                .iter()
                .enumerate()
                .map(|(place, row)| {
                    let line = netrunner_client::identity::ladder_line(place + 1, row);
                    if page.page.key.as_ref() == Some(&row.key) { Line::from(Span::styled(format!("{line} · you"), Style::default().add_modifier(Modifier::BOLD))) } else { Line::from(line) }
                })
                .collect(),
        };
        let [top, rows_area, bottom] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(head.len() as u16 + 2), Constraint::Length(PROFILE_ROWS.len() as u16 + 2), Constraint::Min(0)])
            .areas(area);
        let name = self.attached_url.as_deref().map(|url| self.server_name(url)).unwrap_or_else(|| "this server".to_string());
        frame.render_widget(Paragraph::new(head).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title(format!("Your profile at {name}"))), top);
        let worn = |worn: &Option<String>, count: usize, glyph: bool| match worn {
            Some(name) if glyph => format!("[{}] {name}  ({count} earned)", netrunner_client::identity::badge_initials(name)),
            Some(name) => format!("{name}  ({count} earned)"),
            None if count == 0 => "None earned here yet".to_string(),
            None => format!("None worn  ({count} earned)"),
        };
        let items: Vec<ListItem> = PROFILE_ROWS
            .iter()
            .map(|row| match row {
                ProfileRow::Title => ListItem::new(format!("Title    {}", worn(&profile.title, profile.titles.len(), false))),
                ProfileRow::Badge => ListItem::new(format!("Badge    {}", worn(&profile.badge, profile.badges.len(), true))),
                ProfileRow::Ladder => ListItem::new(format!("Ladder   ‹ {} ›", capitalised(format_name(page.ladder_format)))),
                ProfileRow::Back => ListItem::new("[ Back ]"),
            })
            .collect();
        draw_list(frame, rows_area, "Enter changes what you wear", items, Some(page.cursor.min(PROFILE_ROWS.len() - 1)));
        frame.render_widget(Paragraph::new(ladder).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title("Ladder")), bottom);
        if let Some((kind, cursor)) = page.choosing {
            let earned = page.earned(kind);
            let mut items = vec![ListItem::new("None")];
            items.extend(earned.iter().map(|name| ListItem::new(if kind == AwardKind::Badge { format!("[{}] {name}", netrunner_client::identity::badge_initials(name)) } else { name.clone() })));
            let width = area.width.saturating_sub(8).min(60);
            let height = area.height.saturating_sub(4).min(earned.len() as u16 + 3);
            let popup = Rect { x: area.x + (area.width - width) / 2, y: area.y + (area.height - height) / 2, width, height };
            frame.render_widget(ratatui::widgets::Clear, popup);
            let title = format!("{} — {} earned", if kind == AwardKind::Title { "Wear a title" } else { "Wear a badge" }, earned.len());
            draw_list(frame, popup, &title, items, Some(cursor));
        }
    }
}

impl OnlineScreen {
    /// A tournament's page: its code, format and state, who holds it and
    /// where this key stands in it, its entrants, and under them the rows
    /// — the two decks to lock in and Register, Withdraw for an entrant,
    /// and the list read again.
    fn draw_tournament(&self, frame: &mut Frame, area: Rect, page: &ServerPage, tournament: &TournamentPage) {
        let info = &tournament.info;
        let bold = |text: String| Line::from(Span::styled(text, Style::default().add_modifier(Modifier::BOLD)));
        let mut head = vec![
            Line::from(format!("Code {} · {} · {}", info.id, capitalised(format_name(info.format)), tournament::state_label(info.state))),
            Line::from(format!("Held by {}", info.organizer.fingerprint())),
            Line::from(tournament::standing_line(info, page.key.as_ref())),
        ];
        if let Some(line) = tournament::round_line(info, page.key.as_ref()) {
            head.push(Line::from(line));
        }
        if let Some(line) = tournament::clock_line(info, unix_now()) {
            head.push(Line::from(Span::styled(line, Style::default().fg(Color::Yellow))));
        }
        if page.seeking.is_some() && matches!(info.state, TournamentState::Playing { .. }) {
            head.push(Line::from(Span::styled("Seated — waiting for your opponent to sit…", Style::default().fg(Color::Yellow))));
        }
        head.push(Line::from(""));
        match info.state {
            TournamentState::Registering => {
                head.push(bold(if info.entrants.is_empty() { "Nobody has entered yet.".to_string() } else { format!("Entered ({}):", info.entrants.len()) }));
                head.extend(info.entrants.iter().map(|entrant| Line::from(format!("  {}", tournament::entrant_line(entrant)))));
            }
            TournamentState::Playing { .. } | TournamentState::Finished | TournamentState::Cancelled => {
                head.push(bold(format!("{}:", tournament::standings_heading(info.state))));
                head.extend(info.standings().iter().enumerate().map(|(index, standing)| Line::from(format!("  {}", tournament::standing_row(info, index + 1, standing)))));
                if let Some(line) = tournament::seeds_line(info) {
                    head.push(Line::from(line));
                }
                if let Some(current) = info.current_round() {
                    head.push(Line::from(""));
                    head.push(bold(format!("Round {}:", info.rounds.len())));
                    head.extend(current.tables.iter().enumerate().map(|(index, table)| Line::from(format!("  {}", tournament::table_line(info, index, table)))));
                    if let Some(bye) = tournament::bye_line(info, current) {
                        head.push(Line::from(format!("  {bye}")));
                    }
                }
            }
        }
        // The rows keep at least a few lines; a long head is cut, not the
        // rows, because the rows are what a key acts on.
        let head_height = (head.len() as u16 + 2).min(area.height.saturating_sub(6));
        let [top, list] = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(head_height), Constraint::Min(0)]).areas(area);
        frame.render_widget(Paragraph::new(head).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title(info.name.clone())), top);
        let deck = |side: Side| tournament.chosen(side).map(online::label).unwrap_or_else(|| format!("none legal in {}", format_name(info.format)));
        let entered = tournament::entry_of(info, page.key.as_ref()).is_some();
        let page_rows = tournament.rows(page.key.as_ref(), page.seeking.is_some());
        let next_round = info.rounds.len() + 1;
        let rows: Vec<String> = page_rows
            .iter()
            .map(|row| match *row {
                TournamentRow::CorpDeck => format!("Corp deck        ‹ {} ›", deck(Side::Corp)),
                TournamentRow::RunnerDeck => format!("Runner deck      ‹ {} ›", deck(Side::Runner)),
                TournamentRow::Register => if entered { "[ Register again with these decks ]".to_string() } else { "[ Register ]".to_string() },
                TournamentRow::Withdraw => "[ Withdraw ]".to_string(),
                TournamentRow::Sit => "[ Sit at your table ]".to_string(),
                TournamentRow::StandUp => "Seated — waiting for your opponent… (Enter stands up)".to_string(),
                TournamentRow::OfferDraw => "[ Offer a draw — a tie if your opponent offers one too ]".to_string(),
                TournamentRow::Drop => "[ Drop from the tournament — your results stand ]".to_string(),
                TournamentRow::BeginRound => format!("[ Begin round {next_round} ]"),
                TournamentRow::Finish => "[ End the tournament — the standings are final ]".to_string(),
                TournamentRow::Record(table, outcome) => format!(
                    "Record table {}: {}",
                    table + 1,
                    match outcome {
                        TableOutcome::CorpWon => "the Corp won",
                        TableOutcome::RunnerWon => "the Runner won",
                        TableOutcome::Tie => "a tie",
                    }
                ),
                TournamentRow::Refresh => "Read it again".to_string(),
            })
            .collect();
        let items = rows.into_iter().map(ListItem::new).collect();
        let title = match info.state {
            TournamentState::Registering => "Your entry",
            TournamentState::Playing { .. } => "This round",
            TournamentState::Finished => "Over",
            TournamentState::Cancelled => "Called off",
        };
        draw_list(frame, list, title, items, Some(tournament.cursor.min(page_rows.len() - 1)));
    }
}

/// Seconds since the Unix epoch, the client's own clock, for the round's
/// countdown (`tournament::clock_line`).
fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// A lobby as a line names it (`netrunner_client::online::lobby_title`).
fn lobby_title(lobby: &LobbyInfo) -> String {
    online::lobby_title(lobby)
}

fn step_format(format: NsgFormat, back: bool) -> NsgFormat {
    let all = NsgFormat::ALL;
    let at = all.iter().position(|f| *f == format).unwrap_or(0);
    let len = all.len();
    all[if back { (at + len - 1) % len } else { (at + 1) % len }]
}

fn capitalised(words: &str) -> String {
    let mut chars = words.chars();
    chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}

fn draw_list(frame: &mut Frame, area: Rect, title: &str, items: Vec<ListItem>, cursor: Option<usize>) {
    let mut state = ListState::default();
    state.select(cursor);
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title.to_string()))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
        &mut state,
    );
}

/// The host's lines: every address to give out, each with who can use it,
/// and — hosting for the internet — where the router's answer stands. One
/// address per line, because the line is what they read out.
fn hosting_status(hosting: &Hosting) -> String {
    let mut lines = vec!["Give your opponent an address. They connect, join your lobby and look for a game, as you do below.".to_string()];
    lines.extend(hosting.invitation.ways().into_iter().map(|way| match way {
        // The ticket on a line of its own, to copy whole.
        Way::Give { what, who, ticket: true } => format!("  Ticket — {who}:\n{what}"),
        Way::Give { what, who, ticket: false } => format!("  {what} — {who}"),
        Way::Note(note) => format!("  {note}"),
    }));
    lines.join("\n")
}

/// Runs a future to completion from the menu's synchronous loop, giving up
/// after `BLOCKING_TIMEOUT`. `block_in_place` parks this worker so the
/// runtime's others carry on, which needs the binary's multi-threaded
/// runtime.
fn block_on_bounded<F: Future>(future: F) -> Option<F::Output> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async { tokio::time::timeout(BLOCKING_TIMEOUT, future).await.ok() })
    })
}

/// Binds a human-vs-human server on `port` and starts it; returns it and
/// the loopback URL the host attaches by. Port 0 takes any free port
/// (tests). `Reach::Internet` also starts asking the router to forward the
/// port, and offers a ticket (`peer`) whose streams this same server
/// serves; the tick polls both.
///
/// Unrated, by design: a rating is a claim by a server somebody else
/// runs, and this process holds the seed and the unmasked state of the
/// game its own host is playing in (`docs/identity-and-rating.md`).
fn start_hosting(port: u16, reach: Reach, format: NsgFormat, relay: &Result<Relay, String>, key: Option<[u8; 32]>) -> Result<(Hosting, String), String> {
    let relay = match reach {
        Reach::Internet => Some(relay.clone().map_err(|error| format!("the relay setting: {error}"))?),
        _ => None,
    };
    let options = ServeOptions { bots: Vec::new(), formats: vec![format], ..ServeOptions::default() };
    let listener = hosting::bind_listener(reach, port).map_err(|error| error.to_string())?;
    let server = Server::from_listener(listener, options).map_err(|error| error.to_string())?;
    let port = server.local_addr().map_err(|error| error.to_string())?.port();
    let acceptor = server.acceptor();
    let invitation = Invitation::start(reach, port, relay, key, move |stream, who| {
        let acceptor = acceptor.clone();
        tokio::spawn(async move { acceptor.serve(stream, &who).await });
    });
    let task = tokio::spawn(server.run());
    Ok((Hosting { task, invitation }, format!("ws://127.0.0.1:{port}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    use netrunner_client::hosting::{Mapping, PortMapper};
    use netrunner_client::peer::Offer;
    use netrunner_core::rules::{Side, Viewer};

    fn screen(name: &str) -> (OnlineScreen, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("netrunner_online_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let screen = OnlineScreen::open(&dir, &netrunner_client::decks::sample_deck_registry(), NsgFormat::Startup, name.to_string(), "ws://127.0.0.1:8080".into(), Ok(Relay::Off))
            .unwrap()
            .with_identity_dir(Some(dir.join("identity")));
        (screen, dir)
    }

    fn press(screen: &mut OnlineScreen, keys: &[KeyCode]) {
        for key in keys {
            let _ = screen.key(*key);
        }
    }

    fn type_text(screen: &mut OnlineScreen, text: &str) {
        for c in text.chars() {
            screen.key(KeyCode::Char(c));
        }
    }

    /// Ticks until `done`, or fails after ten seconds.
    async fn until(screen: &mut OnlineScreen, what: &str, mut done: impl FnMut(&OnlineScreen) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            screen.tick();
            if done(screen) {
                return;
            }
            assert!(Instant::now() < deadline, "no {what} within 10s; notice: {:?}", screen.notice);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn server_page(screen: &OnlineScreen) -> Option<&ServerPage> {
        match &screen.mode {
            Mode::Server(page) => Some(page),
            _ => None,
        }
    }

    /// Ticks until the Server page is up and in a lobby, with its decks.
    async fn until_in_lobby(screen: &mut OnlineScreen) {
        until(screen, "lobby", |screen| server_page(screen).is_some_and(|page| page.lobby.is_some() && !page.decks.is_empty())).await;
    }

    /// Ticks until the Server page lists the server's lobbies.
    async fn until_listed(screen: &mut OnlineScreen) {
        until(screen, "lobbies", |screen| server_page(screen).is_some_and(|page| !page.lobbies.is_empty())).await;
    }

    /// Ticks until the connection resolves to a place.
    async fn until_play(screen: &mut OnlineScreen) -> Box<Joined> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let OnlineStep::Play { joined } = screen.tick() {
                return joined;
            }
            assert!(Instant::now() < deadline, "no seat within 10s; notice: {:?}", screen.notice);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Hosts for this machine on a free port: Host → port 0 → this machine
    /// → Start hosting, then the Server page in the one lobby.
    async fn host_up(screen: &mut OnlineScreen) -> String {
        press(screen, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace]);
        press(screen, &[KeyCode::Char('0'), KeyCode::Enter, KeyCode::Down, KeyCode::Left, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        assert!(matches!(screen.mode, Mode::Connecting { .. }), "hosting: {:?}", screen.notice);
        until_in_lobby(screen).await;
        screen.hosting.as_ref().unwrap().invitation.shares[0].url.clone()
    }

    /// Join → the address typed → Connect, then the Server page with the
    /// lobbies listed.
    async fn join(screen: &mut OnlineScreen, address: &str) {
        // Join a server is the list of servers, whose first row with none
        // listed is Connect once, the address form.
        press(screen, &[KeyCode::Down, KeyCode::Enter, KeyCode::Enter, KeyCode::Enter]);
        press(screen, &vec![KeyCode::Backspace; 40]);
        type_text(screen, address);
        press(screen, &[KeyCode::Enter, KeyCode::Down, KeyCode::Enter]);
        assert!(matches!(screen.mode, Mode::Connecting { .. }), "{:?}", screen.notice);
        until_listed(screen).await;
    }

    /// Picks the deck with this id for `side` on the Server page, and
    /// looks for a game in `chair`.
    fn seek_as(screen: &mut OnlineScreen, chair: ChairChoice, deck_id: &str) {
        let Mode::Server(page) = &mut screen.mode else { panic!("the Server page") };
        page.chair = chair;
        let side = if chair == ChairChoice::Runner { Side::Runner } else { Side::Corp };
        let index = page.side_decks(side).iter().position(|deck| deck.id == deck_id).expect("a legal deck");
        match side {
            Side::Corp => page.corp_deck = index,
            Side::Runner => page.runner_deck = index,
        }
        page.rest_on(Row::Seek);
        screen.key(KeyCode::Enter);
    }

    #[test]
    fn the_port_field_takes_digits_only_and_a_bad_port_is_refused() {
        let (mut screen, _) = screen("port");
        press(&mut screen, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace]);
        press(&mut screen, &[KeyCode::Char('9'), KeyCode::Char('x'), KeyCode::Char('9'), KeyCode::Enter]);
        let Mode::Form(form) = &screen.mode else { panic!() };
        assert_eq!(form.port, "99", "letters are not typed into a port");
        let Mode::Form(form) = std::mem::replace(&mut screen.mode, Mode::Home { cursor: 0 }) else { panic!() };
        let mut form = form;
        form.port = "99999".into();
        screen.go(form);
        assert!(screen.notice.as_deref().is_some_and(|notice| notice.contains("not a port")), "{:?}", screen.notice);
        assert!(screen.hosting.is_none() && screen.attached.is_none());
    }

    /// The list of servers is edited on its rows (Phase 7 §14): Add on a
    /// form checked against the list, `d` the Default and back, `e` a
    /// rename, `x` a removal — each written to the file beside the key.
    #[test]
    fn the_servers_are_added_made_default_renamed_and_removed_from_the_list() {
        let (mut screen, dir) = screen("servers");
        let book = || ServerBook::load(&server_list::path_in(&dir.join("identity"))).unwrap();
        press(&mut screen, &[KeyCode::Down, KeyCode::Enter, KeyCode::Down]);
        assert!(matches!(&screen.mode, Mode::Servers(list) if list.rows() == vec![ServersRow::Once, ServersRow::Add]));
        press(&mut screen, &[KeyCode::Enter]);
        type_text(&mut screen, "Club");
        press(&mut screen, &[KeyCode::Tab, KeyCode::Enter]);
        type_text(&mut screen, "club.example.net");
        press(&mut screen, &[KeyCode::Enter, KeyCode::Down, KeyCode::Enter]);
        assert!(matches!(&screen.mode, Mode::Servers(_)), "{:?}", screen.notice);
        assert_eq!(book().servers.iter().map(|server| (server.name.as_str(), server.address.as_str())).collect::<Vec<_>>(), [("Club", "ws://club.example.net:8080")]);
        // The same name again is refused on the form, and nothing written.
        let Mode::Servers(list) = &screen.mode else { panic!() };
        let add = list.rows().iter().position(|row| *row == ServersRow::Add).unwrap();
        if let Mode::Servers(list) = &mut screen.mode {
            list.cursor = add;
        }
        press(&mut screen, &[KeyCode::Enter]);
        type_text(&mut screen, "club");
        press(&mut screen, &[KeyCode::Tab, KeyCode::Enter]);
        type_text(&mut screen, "elsewhere");
        press(&mut screen, &[KeyCode::Enter, KeyCode::Down, KeyCode::Enter]);
        assert!(matches!(&screen.mode, Mode::EditServer { .. }) && screen.notice.as_deref().is_some_and(|notice| notice.contains("already called")), "{:?}", screen.notice);
        press(&mut screen, &[KeyCode::Esc]);
        if let Mode::Servers(list) = &mut screen.mode {
            list.cursor = 0;
        }
        press(&mut screen, &[KeyCode::Char('d')]);
        assert_eq!(book().default.as_deref(), Some("ws://club.example.net:8080"));
        press(&mut screen, &[KeyCode::Char('d')]);
        assert_eq!(book().default, None, "d again unmakes it");
        press(&mut screen, &[KeyCode::Char('e'), KeyCode::Enter]);
        press(&mut screen, &[KeyCode::Backspace; 10]);
        type_text(&mut screen, "The Club");
        press(&mut screen, &[KeyCode::Enter, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        assert_eq!(book().servers[0].name, "The Club");
        press(&mut screen, &[KeyCode::Char('x')]);
        assert!(book().servers.is_empty());
        press(&mut screen, &[KeyCode::Esc]);
        assert!(matches!(screen.mode, Mode::Home { cursor: 1 }));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A Default is connected to as the screen opens, and the profile
    /// page there wears a title the store says was earned, picked from a
    /// list that scrolls (Phase 7 §14).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_default_is_connected_to_and_a_title_is_worn_from_the_list() {
        let dir = std::env::temp_dir().join(format!("netrunner_online_profile_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let credentials = netrunner_client::identity::Credentials::in_dir(&dir.join("identity")).unwrap();
        let key = credentials.identity.public_key().rating_id();
        let data = dir.join("daemon");
        std::fs::create_dir_all(&data).unwrap();
        let store = netrunner_store::Store::open(&netrunner_store::sqlite_url(&data.join(netrunner_store::SQLITE_FILE))).await.unwrap();
        store.saw_player(&key, "case", 1).await.unwrap();
        for (index, name) in ["Gauntlet Runner", "Founder", "Glacier Breaker"].into_iter().enumerate() {
            store.grant(&netrunner_store::Award { key: key.clone(), kind: "title".into(), name: name.into(), earned_at: index as i64, source: "event:abc".into() }).await.unwrap();
        }
        store.close().await;
        let options = ServeOptions { seed: Some(1), data_dir: Some(data), ..ServeOptions::default() };
        let server = Server::bind("127.0.0.1:0", options).await.unwrap();
        let url = format!("ws://{}", server.local_addr().unwrap());
        tokio::spawn(server.run());
        ServerBook::update(&server_list::path_in(&dir.join("identity")), |book| {
            ServerEdit::Add { name: "Club".into(), address: url.clone() }.apply(book)?;
            ServerEdit::Default(Some(url.clone())).apply(book)
        })
        .unwrap();

        let mut screen = OnlineScreen::open(&dir, &netrunner_client::decks::sample_deck_registry(), NsgFormat::Startup, "case".to_string(), "ws://127.0.0.1:8080".into(), Ok(Relay::Off))
            .unwrap()
            .with_identity_dir(Some(dir.join("identity")))
            .connect_default();
        assert!(matches!(screen.mode, Mode::Connecting { .. }), "the Default is dialled as the screen opens");
        until(&mut screen, "the profile", |screen| server_page(screen).is_some_and(|page| matches!(page.profile, ProfileHere::Known(_)))).await;
        let Mode::Server(page) = &mut screen.mode else { panic!() };
        page.cursor = page.rows().iter().position(|row| *row == Row::Profile).unwrap();
        press(&mut screen, &[KeyCode::Enter]);
        assert!(matches!(&screen.mode, Mode::Profile(_)), "{:?}", screen.notice);
        until(&mut screen, "the ladder", |screen| matches!(&screen.mode, Mode::Profile(page) if page.ladder.is_some())).await;
        // Enter on Title opens the list; End is the last earned, worn at once.
        press(&mut screen, &[KeyCode::Enter]);
        assert!(matches!(&screen.mode, Mode::Profile(page) if page.choosing == Some((AwardKind::Title, 0))));
        // Drawn, the list over the page, at a small terminal's size and a
        // large one's: what is on screen is the list's own rows.
        for (width, height) in [(80, 24), (160, 48)] {
            let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| screen.draw(frame, frame.area())).unwrap();
            let shown: String = terminal.backend().buffer().content().iter().map(|cell| cell.symbol()).collect();
            assert!(shown.contains("Wear a title — 3 earned") && shown.contains("Glacier Breaker"), "{shown}");
        }
        press(&mut screen, &[KeyCode::End, KeyCode::Enter]);
        until(&mut screen, "the title worn", |screen| matches!(&screen.mode, Mode::Profile(page) if page.profile().is_some_and(|profile| profile.title.as_deref() == Some("Glacier Breaker")))).await;
        // Esc to the Server page, then Servers: the Default is the row
        // connected to.
        press(&mut screen, &[KeyCode::Esc]);
        let Mode::Server(page) = &mut screen.mode else { panic!() };
        page.cursor = page.rows().iter().position(|row| *row == Row::Servers).unwrap();
        press(&mut screen, &[KeyCode::Enter]);
        assert!(matches!(&screen.mode, Mode::Servers(list) if list.page.is_some() && list.book.default.as_deref() == Some(url.as_str())));
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal.draw(|frame| screen.draw(frame, frame.area())).unwrap();
        let shown: String = terminal.backend().buffer().content().iter().map(|cell| cell.symbol()).collect();
        assert!(shown.contains("Club · Default · Connected") && shown.contains("Wearing the title Glacier Breaker"), "{shown}");
        press(&mut screen, &[KeyCode::Enter]);
        assert!(matches!(&screen.mode, Mode::Server(_)), "Enter on the row connected to is its Server page again");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn escape_walks_back_out() {
        let (mut screen, _) = screen("escape");
        press(&mut screen, &[KeyCode::Down, KeyCode::Enter]);
        assert!(matches!(&screen.mode, Mode::Servers(_)), "Join a server is the list of servers");
        press(&mut screen, &[KeyCode::Enter]);
        assert!(matches!(&screen.mode, Mode::Form(form) if form.kind == FormKind::Join), "Connect once is the address form");
        press(&mut screen, &[KeyCode::Esc]);
        assert!(matches!(screen.mode, Mode::Home { cursor: 1 }));
        assert!(matches!(screen.key(KeyCode::Esc), OnlineStep::Back));
    }

    /// The whole flow over real sockets: one screen hosts and looks for a
    /// game as the Runner, another attaches by the host's address, joins
    /// the lobby and looks as the Corp — and both come out with seats, on
    /// the chairs they chose, with the decks they brought. Then the
    /// watcher lists that match. Back from the game the host is on the
    /// Server page still, and Esc disconnects.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_host_and_a_joiner_are_seated_with_the_decks_they_brought() {
        let (mut host, _) = screen("host");
        let address = host_up(&mut host).await;
        assert!(address.starts_with("ws://127.0.0.1:"), "this machine only");
        assert!(host.hosting_lines.contains(&address), "the host's lines keep the address to share: {}", host.hosting_lines);
        seek_as(&mut host, ChairChoice::Runner, "stolen_goods");

        let (mut joiner, _) = screen("joiner");
        join(&mut joiner, address.trim_start_matches("ws://")).await;
        let page = server_page(&joiner).unwrap();
        assert!(page.lobby.is_none() && page.lobbies.len() == 1 && page.lobbies[0].permanent(), "a joiner picks: {:?}", page.lobbies);
        joiner.key(KeyCode::Enter);
        until_in_lobby(&mut joiner).await;
        seek_as(&mut joiner, ChairChoice::Corp, "brick_stack");

        let joined = until_play(&mut joiner).await;
        assert_eq!(joined.viewer, Viewer::Player(Side::Corp));
        assert_eq!(joined.decks, ("brick_stack".to_string(), String::new()), "told its own deck, never the host's");
        let hosted = until_play(&mut host).await;
        assert_eq!(hosted.viewer, Viewer::Player(Side::Runner));
        assert_eq!(hosted.decks, (String::new(), "stolen_goods".to_string()));

        let (mut watcher, _) = screen("watcher");
        press(&mut watcher, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        press(&mut watcher, &vec![KeyCode::Backspace; 40]);
        type_text(&mut watcher, &address);
        watcher.key(KeyCode::Enter);
        let Mode::Watch { matches, .. } = &watcher.mode else { panic!("{:?}", watcher.notice) };
        assert_eq!(matches.len(), 1);
        assert_eq!((matches[0].corp.as_str(), matches[0].runner.as_str()), ("joiner", "host"), "players go by their own names");
        watcher.key(KeyCode::Enter);
        let watching = until_play(&mut watcher).await;
        assert_eq!(watching.viewer, Viewer::Spectator);

        // The game over — the joiner's handle dropped concedes it — the
        // host is back in the lobby, attached still.
        drop(joined);
        host.returned();
        assert!(host.attached.is_some() && host.hosting.is_some(), "the connection and the server outlive the game");
        until(&mut host, "lobby after the game", |host| server_page(host).is_some_and(|page| page.seeking.is_none() && page.lobby.is_some())).await;
        host.key(KeyCode::Esc);
        assert!(host.attached.is_none() && host.hosting.is_none(), "Esc disconnects, and the hosted server stops");
        assert!(matches!(&host.mode, Mode::Form(form) if form.kind == FormKind::Host));
    }

    /// A friend queue of the joiner's own, made on the form and joined as
    /// the server answers; then the host joins it by its code and password.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_friend_queue_is_made_and_joined_by_its_code() {
        let (mut host, _) = screen("maker_host");
        let address = host_up(&mut host).await;
        let (mut joiner, _) = screen("maker");
        join(&mut joiner, &address).await;
        // Make a friend queue…: name, format, bots, password, Make.
        let Mode::Server(page) = &mut joiner.mode else { panic!() };
        page.rest_on(Row::Make);
        joiner.key(KeyCode::Enter);
        assert!(matches!(joiner.mode, Mode::MakeLobby { .. }));
        joiner.key(KeyCode::Enter);
        type_text(&mut joiner, "Friday");
        press(&mut joiner, &[KeyCode::Enter, KeyCode::Down, KeyCode::Down, KeyCode::Right, KeyCode::Down, KeyCode::Enter]);
        assert!(matches!(&joiner.mode, Mode::MakeLobby { form, .. } if form.bots && form.editing == Some(MakeForm::PASSWORD)), "{:?}", joiner.notice);
        type_text(&mut joiner, "swordfish");
        press(&mut joiner, &[KeyCode::Enter, KeyCode::Down, KeyCode::Enter]);
        until_in_lobby(&mut joiner).await;
        let made = server_page(&joiner).unwrap().lobby.clone().unwrap();
        assert!(made.mode == netrunner_server::protocol::QueueMode::Friends && made.password && !made.rated && made.name == "Friday" && made.id.len() == 6, "{made:?}");
        assert!(made.bots.is_empty(), "a hosted server offers no bot, whatever its friend queue asks");

        // The host joins the friend queue by its code and password, typed
        // in lower case.
        let Mode::Server(page) = &mut host.mode else { panic!() };
        page.rest_on(Row::Code);
        host.key(KeyCode::Enter);
        type_text(&mut host, &made.id.to_lowercase());
        press(&mut host, &[KeyCode::Down, KeyCode::Enter]);
        type_text(&mut host, "swordfish");
        press(&mut host, &[KeyCode::Down, KeyCode::Enter]);
        until(&mut host, "friend queue", |host| server_page(host).is_some_and(|page| page.lobby.as_ref().is_some_and(|lobby| lobby.id == made.id))).await;
    }

    /// Hosting for the internet gives out a ticket, and a joiner who
    /// pastes it into Join is attached to the host's server — the same
    /// server the host attached to over loopback. The test screens have
    /// no relay, so the ticket names this machine's own addresses.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_joiner_comes_in_by_the_hosts_ticket() {
        let (mut host, _) = screen("ticket_host");
        press(&mut host, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace]);
        press(&mut host, &[KeyCode::Char('0'), KeyCode::Enter, KeyCode::Down, KeyCode::Right, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        assert!(matches!(host.mode, Mode::Connecting { .. }), "{:?}", host.notice);
        // The router is not what this is about, and a test asks no router.
        host.hosting.as_mut().unwrap().invitation.mapping = None;
        let Offer::Ready { ticket, .. } = host.hosting.as_mut().unwrap().invitation.peer.as_mut().expect("the internet gets a ticket").ready().await else {
            panic!("no ticket")
        };
        let ticket = ticket.to_string();
        until_in_lobby(&mut host).await;
        assert!(host.hosting_lines.lines().any(|line| line == ticket), "the ticket is a line of its own, to copy whole: {}", host.hosting_lines);
        seek_as(&mut host, ChairChoice::Corp, "brick_stack");

        let (mut joiner, _) = screen("ticket_joiner");
        join(&mut joiner, &ticket).await;
        joiner.key(KeyCode::Enter);
        until_in_lobby(&mut joiner).await;
        seek_as(&mut joiner, ChairChoice::Runner, "stolen_goods");
        let joined = until_play(&mut joiner).await;
        let hosted = until_play(&mut host).await;
        assert_ne!(joined.viewer, hosted.viewer, "the two are seated against each other");
    }

    /// A router request whose answer the test sets, and which says when it
    /// has been let go.
    struct FakeRouter {
        answer: std::sync::Arc<std::sync::Mutex<Mapping>>,
        released: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl PortMapper for FakeRouter {
        fn poll(&mut self) -> Mapping {
            self.answer.lock().unwrap().clone()
        }
    }

    impl Drop for FakeRouter {
        fn drop(&mut self) {
            self.released.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Hosting for the internet, the host's lines follow the router:
    /// asking, then the public address to give out — or, behind the
    /// provider's NAT, why no address will do and what to do instead.
    /// Leaving lets the mapping go.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_hosts_line_follows_the_routers_answer() {
        use hosting::MappingFailure;
        use std::net::{Ipv4Addr, SocketAddrV4};

        let (mut host, _) = screen("router");
        press(&mut host, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace]);
        press(&mut host, &[KeyCode::Char('0'), KeyCode::Enter, KeyCode::Down, KeyCode::Left, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        assert!(matches!(host.mode, Mode::Connecting { .. }), "{:?}", host.notice);
        let answer = std::sync::Arc::new(std::sync::Mutex::new(Mapping::Asking));
        let released = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        host.hosting.as_mut().unwrap().invitation.mapping = Some(Box::new(FakeRouter { answer: answer.clone(), released: released.clone() }));
        let status = |host: &mut OnlineScreen| {
            host.tick();
            host.hosting_lines.clone()
        };

        assert!(status(&mut host).contains("asking your router to open port"));
        *answer.lock().unwrap() = Mapping::Open(SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 40123));
        assert!(status(&mut host).contains("ws://203.0.113.7:40123 — from anywhere"), "the router's port, which may not be ours");
        *answer.lock().unwrap() = Mapping::Failed(MappingFailure::SharedAddress(Ipv4Addr::new(100, 70, 1, 1)));
        let shared = status(&mut host);
        assert!(shared.contains("100.70.1.1") && shared.contains("Tailscale"), "{shared}");
        assert!(shared.contains("ws://127.0.0.1:"), "the addresses that do work are still listed: {shared}");

        host.key(KeyCode::Esc);
        host.returned();
        assert!(released.load(std::sync::atomic::Ordering::SeqCst), "leaving releases the mapping");
    }

    /// The Server page drawn, hosting and in a lobby: the host's lines
    /// above the rows, the lobby's row marked Here, the chair, the deck,
    /// Find a game — and, looking, the queue with Enter to stop.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn the_server_page_draws_its_rows() {
        use ratatui::backend::TestBackend;
        let (mut host, _) = screen("drawn");
        let address = host_up(&mut host).await;
        let drawn = |host: &OnlineScreen| {
            let mut terminal = ratatui::Terminal::new(TestBackend::new(120, 30)).unwrap();
            terminal.draw(|frame| host.draw(frame, frame.area())).unwrap();
            let buffer = terminal.backend().buffer().clone();
            (0..buffer.area.height).map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_string()).collect::<String>()).collect::<Vec<_>>().join("\n")
        };
        let text = drawn(&host);
        for wanted in [
            &address[..],
            "Give your opponent an address",
            "Here  Casual · Startup — 1 here, 0 looking · unrated",
            "Unrated: a game hosted from this machine is never rated.",
            "Chair            ‹ Corp ›",
            "Corp deck        Corp ·",
            "[ Find a game ]",
            "[ Leave lobby ]",
            "[ Disconnect ]",
            "Make a friend queue…",
            "[ Join by code ]",
        ] {
            assert!(text.contains(wanted), "{wanted:?} is not drawn:\n{text}");
        }
        seek_as(&mut host, ChairChoice::Either, "brick_stack");
        until(&mut host, "queue", |host| server_page(host).is_some_and(|page| page.seeking == Some(1))).await;
        let text = drawn(&host);
        assert!(text.contains("Looking for a game — 1 waiting in this queue… (Enter stops)"), "{text}");
        assert!(text.contains("Preference       ‹ Any / Balanced ›"), "either chair draws its preference:\n{text}");
        assert!(!text.contains("Against"), "a hosted server offers no bot, so there is no opponent to choose:\n{text}");
        assert!(text.contains("Runner deck      Runner ·") && text.contains("Corp deck        Corp ·"), "either chair draws both decks:\n{text}");
    }

    /// Tournaments, from the pages (Phase 4 §7 stage 6b), over a daemon
    /// that keeps things: one screen holds a tournament from the form and
    /// lands on its page; another opens it from the list, locks in the two
    /// decks shown and registers, is drawn as entered, and withdraws; the
    /// organizer reads the list again and sees both. A screen with no key
    /// is told it cannot hold one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_tournament_is_held_entered_and_left_from_the_pages() {
        use ratatui::backend::TestBackend;
        let data = std::env::temp_dir().join(format!("netrunner_online_tournament_daemon_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let options = ServeOptions { seed: Some(1), data_dir: Some(data.clone()), ..ServeOptions::default() };
        let server = Server::bind("127.0.0.1:0", options).await.expect("an ephemeral port binds");
        let address = format!("ws://{}", server.local_addr().unwrap());
        tokio::spawn(server.run());
        let drawn = |screen: &OnlineScreen| {
            let mut terminal = ratatui::Terminal::new(TestBackend::new(120, 40)).unwrap();
            terminal.draw(|frame| screen.draw(frame, frame.area())).unwrap();
            let buffer = terminal.backend().buffer().clone();
            (0..buffer.area.height).map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_string()).collect::<String>()).collect::<Vec<_>>().join("\n")
        };
        let open_tournaments = |screen: &mut OnlineScreen| {
            let Mode::Server(page) = &mut screen.mode else { panic!("the Server page: {:?}", screen.notice) };
            page.rest_on(Row::Tournaments);
            screen.key(KeyCode::Enter);
            assert!(matches!(screen.mode, Mode::Tournaments { .. }));
        };

        let (mut organizer, _) = screen("organizer");
        join(&mut organizer, &address).await;
        until(&mut organizer, "the key", |screen| server_page(screen).is_some_and(|page| page.key.is_some())).await;
        assert!(drawn(&organizer).contains("Tournaments…"), "the Server page offers the tournaments");
        open_tournaments(&mut organizer);
        until(&mut organizer, "the list", |screen| matches!(&screen.mode, Mode::Tournaments { list, .. } if list.listed)).await;
        assert!(drawn(&organizer).contains("this server holds none"));
        // Hold a tournament…: the last row, one Up from the first; the
        // name typed, the format left, Hold.
        press(&mut organizer, &[KeyCode::Up, KeyCode::Enter]);
        assert!(matches!(organizer.mode, Mode::MakeTournament { .. }), "{:?}", organizer.notice);
        press(&mut organizer, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        assert!(organizer.notice.as_deref().is_some_and(|notice| notice.contains("name")), "a tournament needs a name: {:?}", organizer.notice);
        press(&mut organizer, &[KeyCode::Up, KeyCode::Up, KeyCode::Enter]);
        type_text(&mut organizer, "Friday");
        press(&mut organizer, &[KeyCode::Enter, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        until(&mut organizer, "the tournament's page", |screen| matches!(&screen.mode, Mode::Tournament { tournament, .. } if tournament.info.name == "Friday")).await;
        let (code, organizer_key) = match &organizer.mode {
            Mode::Tournament { tournament, page, .. } => (tournament.info.id.clone(), page.key.unwrap()),
            _ => unreachable!(),
        };
        assert_eq!(code.len(), 6);
        let text = drawn(&organizer);
        for wanted in [&format!("Code {code} · Startup · taking registrations")[..], "You hold this tournament. Not entered yet.", "Nobody has entered yet.", "[ Register ]", "Read it again"] {
            assert!(text.contains(wanted), "{wanted:?} is not drawn:\n{text}");
        }

        // An entrant opens it from the list and registers with the decks
        // shown — the first Corp deck stepped once, the first Runner deck.
        let (mut entrant, _) = screen("entrant");
        join(&mut entrant, &address).await;
        until(&mut entrant, "the key", |screen| server_page(screen).is_some_and(|page| page.key.is_some())).await;
        open_tournaments(&mut entrant);
        until(&mut entrant, "the list with one", |screen| matches!(&screen.mode, Mode::Tournaments { list, .. } if list.listed && list.list.len() == 1)).await;
        assert!(drawn(&entrant).contains("Friday · Startup · 0 entered · taking registrations"));
        entrant.key(KeyCode::Enter);
        let Mode::Tournament { tournament, .. } = &entrant.mode else { panic!("the tournament's page") };
        assert!(!tournament.decks.is_empty(), "the decks legal in its format");
        entrant.key(KeyCode::Right);
        let Mode::Tournament { tournament, .. } = &entrant.mode else { panic!() };
        assert_eq!(tournament.corp_deck, 1, "Right steps the Corp deck");
        let expected = (tournament.chosen(Side::Corp).unwrap().id.clone(), tournament.chosen(Side::Runner).unwrap().id.clone());
        press(&mut entrant, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        until(&mut entrant, "the entry", |screen| matches!(&screen.mode, Mode::Tournament { tournament, .. } if tournament.info.entrants.len() == 1)).await;
        let text = drawn(&entrant);
        for wanted in ["You are entered, with the two decks you committed to.", "Entered (1):", "  entrant · ", "[ Register again with these decks ]", "[ Withdraw ]"] {
            assert!(text.contains(wanted), "{wanted:?} is not drawn:\n{text}");
        }
        // The registration the driver kept names the decks the page chose.
        let kept = std::fs::read_to_string(entrant.identity_dir.as_ref().unwrap().join(netrunner_client::identity::REGISTRATIONS_FILE)).expect("the registration is kept beside the key");
        let registration: netrunner_client::identity::Registration = serde_json::from_str(kept.lines().last().unwrap()).unwrap();
        assert_eq!((registration.corp, registration.runner), expected);

        // The organizer reads it again and sees the entrant.
        organizer.key(KeyCode::Char('r'));
        until(&mut organizer, "the entrant", |screen| matches!(&screen.mode, Mode::Tournament { tournament, .. } if tournament.info.entrants.len() == 1)).await;
        assert!(drawn(&organizer).contains("Entered (1):"));

        // Withdrawn: the row after Register.
        press(&mut entrant, &[KeyCode::Down, KeyCode::Enter]);
        until(&mut entrant, "the withdrawal", |screen| matches!(&screen.mode, Mode::Tournament { tournament, .. } if tournament.info.entrants.is_empty())).await;
        assert!(drawn(&entrant).contains("Not entered yet."));
        // Esc walks back: the list, with the tournament brought level,
        // then the Server page.
        entrant.key(KeyCode::Esc);
        assert!(matches!(&entrant.mode, Mode::Tournaments { list, .. } if list.list[0].entrants.is_empty() && list.list[0].organizer == organizer_key));
        entrant.key(KeyCode::Esc);
        assert!(matches!(entrant.mode, Mode::Server(_)));

        // No key, no tournament to hold.
        let keyless_dir = std::env::temp_dir().join(format!("netrunner_online_keyless_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&keyless_dir);
        let mut keyless = OnlineScreen::open(&keyless_dir, &netrunner_client::decks::sample_deck_registry(), NsgFormat::Startup, "keyless".to_string(), "ws://127.0.0.1:8080".into(), Ok(Relay::Off)).unwrap();
        join(&mut keyless, &address).await;
        until(&mut keyless, "the standing", |screen| server_page(screen).is_some_and(|page| page.profile != ProfileHere::Unasked)).await;
        open_tournaments(&mut keyless);
        until(&mut keyless, "the list", |screen| matches!(&screen.mode, Mode::Tournaments { list, .. } if list.listed)).await;
        press(&mut keyless, &[KeyCode::Up, KeyCode::Enter]);
        assert!(matches!(keyless.mode, Mode::Tournaments { .. }) && keyless.notice.as_deref().is_some_and(|notice| notice.contains("no")), "{:?}", keyless.notice);
        // Down from the last row wraps to the first: the tournament.
        press(&mut keyless, &[KeyCode::Down, KeyCode::Enter]);
        let Mode::Tournament { tournament, page, .. } = &keyless.mode else { panic!() };
        assert_eq!(tournament.rows(page.key.as_ref(), false), vec![TournamentRow::Refresh], "nothing to register with");
        assert!(drawn(&keyless).contains("This client has no key, so it cannot enter."));
        let _ = std::fs::remove_dir_all(&data);
    }

    /// The rounds from the pages (Phase 4 §7 stage 6b): the organizer,
    /// entered too, begins round 1 once a second entrant is in, which the
    /// entrant learns unasked; both sit at their table from its row and
    /// are seated on the pairing's sides; a concession is the table's
    /// result and both come back to the tournament's page, where the
    /// standings are drawn and the next round offered; round 2 is
    /// recorded as a tie and the tournament ended, its standings final.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_round_is_begun_sat_at_played_and_ended_from_the_pages() {
        use ratatui::backend::TestBackend;
        let data = std::env::temp_dir().join(format!("netrunner_online_rounds_daemon_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let options = ServeOptions { seed: Some(1), data_dir: Some(data.clone()), ..ServeOptions::default() };
        let server = Server::bind("127.0.0.1:0", options).await.expect("an ephemeral port binds");
        let address = format!("ws://{}", server.local_addr().unwrap());
        tokio::spawn(server.run());
        let drawn = |screen: &OnlineScreen| {
            let mut terminal = ratatui::Terminal::new(TestBackend::new(140, 40)).unwrap();
            terminal.draw(|frame| screen.draw(frame, frame.area())).unwrap();
            let buffer = terminal.backend().buffer().clone();
            (0..buffer.area.height).map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_string()).collect::<String>()).collect::<Vec<_>>().join("\n")
        };
        let info_of = |screen: &OnlineScreen| match &screen.mode {
            Mode::Tournament { tournament, .. } => Some(tournament.info.clone()),
            _ => None,
        };
        let rows_of = |screen: &OnlineScreen| match &screen.mode {
            Mode::Tournament { tournament, page, .. } => tournament.rows(page.key.as_ref(), page.seeking.is_some()),
            _ => Vec::new(),
        };
        /// Puts the cursor on `row` and presses Enter.
        fn act(screen: &mut OnlineScreen, row: TournamentRow) {
            let Mode::Tournament { tournament, page, .. } = &mut screen.mode else { panic!("the tournament's page") };
            let rows = tournament.rows(page.key.as_ref(), page.seeking.is_some());
            tournament.cursor = rows.iter().position(|r| *r == row).unwrap_or_else(|| panic!("no {row:?} among {rows:?}"));
            screen.key(KeyCode::Enter);
        }

        let (mut organizer, _) = screen("rounds_organizer");
        join(&mut organizer, &address).await;
        until(&mut organizer, "the key", |screen| server_page(screen).is_some_and(|page| page.key.is_some())).await;
        let Mode::Server(page) = &mut organizer.mode else { panic!() };
        page.rest_on(Row::Tournaments);
        organizer.key(KeyCode::Enter);
        until(&mut organizer, "the list", |screen| matches!(&screen.mode, Mode::Tournaments { list, .. } if list.listed)).await;
        press(&mut organizer, &[KeyCode::Up, KeyCode::Enter, KeyCode::Enter]);
        type_text(&mut organizer, "Rounds");
        press(&mut organizer, &[KeyCode::Enter, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        until(&mut organizer, "the tournament's page", |screen| info_of(screen).is_some()).await;
        assert!(!rows_of(&organizer).contains(&TournamentRow::BeginRound), "nobody to pair yet");
        act(&mut organizer, TournamentRow::Register);
        until(&mut organizer, "the organizer's entry", |screen| info_of(screen).is_some_and(|info| info.entrants.len() == 1)).await;

        let (mut entrant, _) = screen("rounds_entrant");
        join(&mut entrant, &address).await;
        until(&mut entrant, "the key", |screen| server_page(screen).is_some_and(|page| page.key.is_some())).await;
        let Mode::Server(page) = &mut entrant.mode else { panic!() };
        page.rest_on(Row::Tournaments);
        entrant.key(KeyCode::Enter);
        until(&mut entrant, "the list with one", |screen| matches!(&screen.mode, Mode::Tournaments { list, .. } if list.listed && list.list.len() == 1)).await;
        entrant.key(KeyCode::Enter);
        act(&mut entrant, TournamentRow::Register);
        until(&mut entrant, "the entrant's entry", |screen| info_of(screen).is_some_and(|info| info.entrants.len() == 2)).await;
        until(&mut organizer, "the second entrant, pushed", |screen| info_of(screen).is_some_and(|info| info.entrants.len() == 2)).await;
        assert!(drawn(&organizer).contains("[ Begin round 1 ]"));
        act(&mut organizer, TournamentRow::BeginRound);
        until(&mut organizer, "round 1", |screen| info_of(screen).is_some_and(|info| info.state == TournamentState::Playing { round: 1 })).await;
        until(&mut entrant, "round 1, pushed", |screen| info_of(screen).is_some_and(|info| info.state == TournamentState::Playing { round: 1 })).await;
        let text = drawn(&organizer);
        for wanted in ["Standings:", "1. ", "Round 1:", "Table 1 · ", "· not played yet", "[ Sit at your table ]", "Round 1: you play"] {
            assert!(text.contains(wanted), "{wanted:?} is not drawn:\n{text}");
        }

        // Both sit: the first waits, the second starts the game.
        act(&mut organizer, TournamentRow::Sit);
        until(&mut organizer, "the organizer seated", |screen| server_page_of(screen).is_some_and(|page| page.seeking.is_some())).await;
        assert!(rows_of(&organizer).contains(&TournamentRow::StandUp));
        assert!(drawn(&organizer).contains("waiting for your opponent"));
        act(&mut entrant, TournamentRow::Sit);
        let mut organizer_seat = until_play(&mut organizer).await;
        let mut entrant_seat = until_play(&mut entrant).await;
        let round = info_of(&organizer).unwrap().rounds[0].clone();
        let organizer_key = server_page_of(&organizer).unwrap().key.unwrap();
        let expected = if round.tables[0].corp == organizer_key { Side::Corp } else { Side::Runner };
        assert_eq!(organizer_seat.viewer, Viewer::Player(expected), "the pairing's side");
        assert_eq!(entrant_seat.viewer, Viewer::Player(expected.other()));
        // Both seats' statements are signed before anyone concedes: the
        // server asks for them ahead of the first board, and takes a
        // signature only while the game runs, so a game conceded at once
        // would keep no seat's word. Each seat waits for its first board —
        // by then its driver has answered the request — and then makes a
        // round trip on the same connection, which the server handles in
        // order: a draw offered mid-game, refused because the game is
        // under way. Without the wait, the entrant's signature lost that race
        // once in CI.
        let id = info_of(&organizer).unwrap().id;
        for (screen, seat) in [(&mut organizer, &mut organizer_seat), (&mut entrant, &mut entrant_seat)] {
            while !matches!(tokio::time::timeout(Duration::from_secs(10), seat.rx.recv()).await.expect("a first board in time"), Some(netrunner_server::protocol::ServerMessage::StateUpdate(_))) {}
            screen.attached.as_ref().expect("attached through the game").offer_draw(id.clone());
            until(screen, "the mid-game draw refused", |screen| screen.notice.as_deref().is_some_and(|notice| notice.contains("under way"))).await;
            screen.notice = None;
        }

        // The organizer concedes; both come back to the tournament's page
        // with the result drawn and the next round offered.
        drop(organizer_seat);
        organizer.returned();
        entrant.returned();
        assert!(matches!(organizer.mode, Mode::Tournament { .. }) && matches!(entrant.mode, Mode::Tournament { .. }), "back from a tournament game, its page");
        let winner = if expected == Side::Corp { TableOutcome::RunnerWon } else { TableOutcome::CorpWon };
        until(&mut organizer, "the table's result", |screen| info_of(screen).is_some_and(|info| info.rounds[0].tables[0].result == Some(winner))).await;
        until(&mut organizer, "the seat let go", |screen| server_page_of(screen).is_some_and(|page| page.seeking.is_none())).await;
        drop(entrant_seat);
        until(&mut entrant, "the result, pushed", |screen| info_of(screen).is_some_and(|info| info.rounds[0].tables[0].result == Some(winner))).await;
        // The seed commit-reveal through the real driver (Phase 4 §7 stage
        // 6c): the table's secret is revealed and checks against its
        // commitment, and the receipt the daemon kept holds both seats'
        // statements, each signed with the nonce the driver made and the
        // registered deck the machine held it to.
        let revealed = info_of(&organizer).unwrap();
        assert!(matches!(revealed.seeds[0].check(), Ok(Some(_))), "{:?}", revealed.seeds);
        let receipts: Vec<std::path::PathBuf> = std::fs::read_dir(data.join("matches"))
            .unwrap()
            .flatten()
            .flat_map(|month| std::fs::read_dir(month.path()).unwrap().flatten().map(|entry| entry.path()))
            .filter(|path| path.to_string_lossy().ends_with(".receipt.json"))
            .collect();
        let signed: netrunner_client::identity::Signed = serde_json::from_str(&std::fs::read_to_string(&receipts[0]).unwrap()).unwrap();
        let receipt: netrunner_server::protocol::statements::Receipt = serde_json::from_str(&signed.payload).unwrap();
        for seat in [&receipt.corp, &receipt.runner] {
            let commitment = seat.commitment.as_ref().unwrap_or_else(|| panic!("{} did not sign their seat", seat.name));
            let said: netrunner_server::protocol::statements::SeatStatement = serde_json::from_str(&commitment.payload).unwrap();
            assert_eq!(said.table.map(|table| table.seed_commitment), Some(revealed.seeds[0].commitment.clone()));
        }
        let text = drawn(&organizer);
        for wanted in ["· 3 pts ·", "· 0 pts ·", " won", "[ Begin round 2 ]", "[ End the tournament"] {
            assert!(text.contains(wanted), "{wanted:?} is not drawn:\n{text}");
        }
        assert!(!rows_of(&entrant).contains(&TournamentRow::BeginRound), "the entrant is offered nothing of the organizer's");

        // Round 2 recorded as a tie, and the end.
        act(&mut organizer, TournamentRow::BeginRound);
        until(&mut organizer, "round 2", |screen| info_of(screen).is_some_and(|info| info.state == TournamentState::Playing { round: 2 })).await;
        assert!(drawn(&organizer).contains("Record table 1: a tie"));
        act(&mut organizer, TournamentRow::Record(0, TableOutcome::Tie));
        until(&mut organizer, "the tie", |screen| info_of(screen).is_some_and(|info| info.rounds[1].tables[0].result == Some(TableOutcome::Tie))).await;
        act(&mut organizer, TournamentRow::Finish);
        until(&mut organizer, "the end", |screen| info_of(screen).is_some_and(|info| info.state == TournamentState::Finished)).await;
        let text = drawn(&organizer);
        assert!(text.contains("Final standings:") && text.contains("· 4 pts ·") && text.contains("· 1 pts ·"), "{text}");
        assert_eq!(rows_of(&organizer), vec![TournamentRow::Refresh]);
        let _ = std::fs::remove_dir_all(&data);
    }

    fn server_page_of(screen: &OnlineScreen) -> Option<&ServerPage> {
        match &screen.mode {
            Mode::Server(page) | Mode::Tournaments { page, .. } | Mode::Tournament { page, .. } | Mode::MakeTournament { page, .. } => Some(page),
            _ => None,
        }
    }

    /// A player who stops looking leaves the lobby: the next to arrive
    /// waits too, instead of being paired with someone who has gone.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn abandoning_the_wait_leaves_the_lobby() {
        let (mut host, _) = screen("abandon");
        let url = host_up(&mut host).await;
        seek_as(&mut host, ChairChoice::Corp, "brick_stack");
        // Keep the server alive past the host's own departure, to see the
        // lobby from outside.
        let hosting = host.hosting.take();
        let deadline = Instant::now() + Duration::from_secs(10);
        while remote::list_matches(&url).await.unwrap().1 != 1 {
            host.tick();
            assert!(Instant::now() < deadline, "the host never reached the lobby");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        host.key(KeyCode::Esc);
        while remote::list_matches(&url).await.unwrap().1 != 0 {
            assert!(Instant::now() < deadline, "the abandoned waiter is still in the lobby");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        drop(hosting);
    }
}
