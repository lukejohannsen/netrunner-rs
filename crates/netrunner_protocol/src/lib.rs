//! Transport-agnostic wire messages between a server's `MatchSession` and
//! one player's client — deliberately just serializable data, no transport
//! assumptions baked in, so an in-process `tokio::sync::mpsc` pair and a
//! WebSocket carry the exact same types unchanged.
//!
//! Lifted out of `netrunner_server` (Phase 4 §6 item 2) so a client can
//! speak the protocol without depending on the server: the server
//! re-exports this crate as its `protocol` module, so every
//! `netrunner_server::{protocol::,}ClientMessage` path still resolves.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use netrunner_bots::{Level, Style};
use netrunner_core::decks::{DeckFile, FactionRule};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::{PlayerAction, Side};
use netrunner_core::view::ClientView;
use netrunner_identity::{Nonce, PublicKey, Signature, Signed};
/// Re-exported so a client can read what `Rated` and `Standing` carry
/// without naming the rating crate: a rating is the server's, and the
/// client only shows it (Phase 3 §2).
pub use netrunner_rating::{Rating, Standing};
/// Re-exported so a client can read a tournament's rounds and recompute
/// its standings without naming the Swiss crate: the pairing is the
/// server's, and the client only checks it (Phase 4 §7 stage 6b).
pub use netrunner_tournament as swiss;

pub mod statements;

/// Re-exported, not defined here: both live in `netrunner_session` beside
/// the driver that produces them. `GameEndReason` was never a transport
/// concern — `netrunner_cli` used to depend on this whole crate purely to
/// call `classify_end_reason` on its *offline* local path. Re-exporting
/// keeps every existing `netrunner_server::{protocol::,}GameEndReason` path
/// resolving, and the wire format is unaffected: moving a type does not
/// change its serde representation.
pub use netrunner_core::rules::{ConcealedAction, PublicAction};
pub use netrunner_session::{GameEndReason, HistoryEntry, PublicHistoryEntry, Rewind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// "This is my key": the first step of proving who this connection is
    /// (Phase 4 §5), before `Attach` or `Resume`. Answered with
    /// `Challenge`. Optional: a connection that never identifies plays,
    /// and plays unrated.
    Identify { key: PublicKey },
    /// The answer to `Challenge`: the key's signature over
    /// `netrunner_identity::auth_statement(server_key, nonce)`. Answered
    /// with `Identified`, or `IdentifyRefused` and the socket closed.
    Prove { signature: Signature },
    /// The seat's signature over the statement `SignSeat` sent, under
    /// `statements::SEAT_TAG`, once the client has checked it names this
    /// seat's key, side, match, server and deck.
    SeatSigned { signature: Signature },
    /// This connection's profile at the server (Phase 4 §10): answered
    /// with `Profile`, first thing on a socket (after proving a key) or
    /// from an attached connection — which asks when it attaches and
    /// again when a game puts it back in its lobby, so the Server page
    /// shows the number the game just moved. Never cached by the client
    /// as the truth — the server's store is.
    MyProfile,
    /// Choose what this key wears, among the titles and badges it has
    /// earned here (`Profile::titles`, `badges`); `None` wears nothing.
    /// Answered with the `Profile` as it now stands, or `ProfileRefused`
    /// for a title or badge not earned, or a connection with no key. A
    /// title is never granted this way — an event (§11) or an operator
    /// (§12) grants.
    SetProfile { title: Option<String>, badge: Option<String> },
    /// The server's ladder in `format`: answered with `Ladder`, the keys
    /// with enough rated games there, best first.
    Ladder { format: NsgFormat },
    /// Take a seat back after the socket that held it dropped. The token is
    /// the one `MatchJoined` issued for that seat, and it is the *only*
    /// credential: a seat is worth exactly what a WebSocket connection was
    /// worth before, so a 122-bit random identifier that only ever crossed
    /// this one connection is the same trust the original `Attach` had.
    /// The reply is `MatchJoined` again (same token, same side) followed by
    /// a fresh `StateUpdate`, or `ResumeRejected`; the connection is then
    /// attached as the first one was, and back in its lobby after the
    /// game. A seek is not a seat: it is withdrawn with its socket, and a
    /// client that drops while looking attaches and looks again.
    Resume { session_token: Uuid },
    /// What the daemon is hosting. Answered with `MatchList`, first thing
    /// on a socket or from an attached connection.
    ListMatches,
    /// Watch a running match (an id from `MatchList`) from the
    /// `Viewer::Spectator` perspective: the intersection of what the two
    /// players see, and nothing to submit. Answered with `Spectating` and
    /// then every `StateUpdate`/`ActionLog`/`DecisionClock`/`GameEnded`
    /// the seats get, masked for a spectator; or `ConnectRejected` when no
    /// live match has that id. Anything a spectator sends afterwards is
    /// ignored — it holds no seat.
    Spectate { match_id: Uuid },
    SubmitAction(PlayerAction),
    Surrender,
    /// Take this seat's last move back (Phase 4 §5 stage e): the host
    /// restores the state the move was made from — a restore in the
    /// session, never a `PlayerAction` (AGENTS.md's Session Rule) — and
    /// answers every seat with a fresh `StateUpdate` and a `TakenBack`,
    /// or this seat alone with `ActionRejected` when there is nothing
    /// to take back or the move cannot be. What it would cost was in the
    /// last `Back`: the free kind is always taken, and the undo past it
    /// only where the lobby does not rate its games — in a rated game an
    /// undo would need the other seat's consent, which nothing yet asks
    /// for (`docs/identity-and-rating.md` §5).
    TakeBack,
    /// Attach to the server with nothing but a name — and a key, proved
    /// just before by `Identify` and `Prove`, if the player is to be
    /// rated — and stay attached:
    /// the lobbies are browsed, joined and left, and a game is looked for
    /// and played, all on this one connection, with no deck until a game
    /// is looked for (Phase 4 §7, decided 26 September 2026). Answered
    /// with `Attached`. The only way to play: the one-shot `Connect`, which
    /// carried a deck and closed with the game, was removed before
    /// anything was released.
    ///
    /// **There is no picking an opponent** (decided 26 September 2026): a
    /// lobby pairs whoever is waiting in it, and a friend queue is how two
    /// people who already know each other meet. Listing the waiters and
    /// challenging one was proposed and declined.
    Attach { player_name: String },
    /// The server's own queues, answered with `Lobbies`. A friend queue is
    /// never listed; it is joined by its code.
    ListLobbies,
    /// Make a friend queue and join it (Phase 4 §9): a lobby of a
    /// player's own, joined by the code `LobbyJoined` names and by
    /// `password` when one is set, where the group shows up over time —
    /// it lives while anyone is in it, pairs only among its members, two
    /// members simply play each other, and a member comes back to it after
    /// each game. Answered with `LobbyJoined` or `LobbyRefused`.
    ///
    /// **Casual by definition, and never listed.** A friend queue is how
    /// people who know each other meet, so it counts for nothing and a
    /// stranger never finds it in a list: it was a lobby that could be
    /// rated and could be listed until §9, and both went — rated play is
    /// the server's Ranked queue, and a listed lobby of a player's was a
    /// way to be paired with strangers outside it. `bots` lets its members
    /// ask for a bot (`Opponent::Bot`), the maker's to allow: a bot sits
    /// only in a friend lobby that allows one, never in the server's own
    /// queues (decided 12 October 2026).
    CreateLobby { name: String, format: NsgFormat, password: Option<String>, bots: bool },
    /// Join a lobby by its id — one of the server's queues, or a friend
    /// queue by the code its maker gave — leaving the one this connection
    /// was in. A password is
    /// asked of a lobby made with one. Answered with `LobbyJoined` or
    /// `LobbyRefused`.
    JoinLobby { lobby: String, password: Option<String> },
    LeaveLobby,
    /// Look for a game in the lobby joined, with the deck or decks the
    /// chair needs, against `opponent`. Answered with `Queued`, then
    /// `MatchJoined` when the queue pairs this player (`netrunner_server::
    /// queue`: the chair's hard side, then the preferences, then the swap
    /// of the chairs two keys last took); or `MatchJoined` at once for a
    /// bot; or `SeekRefused`. A second seek while one is open is refused:
    /// `CancelSeek` first, and the new one is a new place in the queue. The
    /// decks go no further than the server, which deals from them and
    /// tells nobody else what they are.
    Seek { chair: Chair, opponent: Opponent },
    /// Stop looking. Answered with `SeekCancelled`.
    CancelSeek,
    /// Hold a tournament on this server (Phase 4 §7 stage 6a), named and
    /// in one of the server's formats, run as Null Signal Games run
    /// theirs: registration first, each entrant's two decks locked for
    /// the whole event behind a signed commitment; the rounds come in
    /// later stages. Only a proved key may call one, and that key is its
    /// organizer — the person the policies give the final say (3.2.1) —
    /// and only a daemon with a data directory holds one, since a
    /// tournament outlives any socket. Answered with `Tournament`, or
    /// `TournamentRefused`.
    CreateTournament { name: String, format: NsgFormat },
    /// Every tournament the server holds, answered with `Tournaments`.
    ListTournaments,
    /// Enter a tournament with the two decks played for the whole event,
    /// one Corp and one Runner: nothing is submitted per round, so there
    /// is nothing to swap mid-event. `statement` is this key's signed
    /// `statements::RegistrationStatement`, naming the two decks by
    /// `statements::deck_hash` under `salt`; the server checks it against
    /// the decks sent and publishes it as the entrant's commitment, and
    /// the decks themselves stay with the server (`TournamentInfo`). The
    /// salt is random, made by the player and kept by both sides, because
    /// without one a well-known list's hash is confirmed by anyone who
    /// guesses the list. Registering again replaces the entry while
    /// registration is open. Answered with `Tournament` or
    /// `TournamentRefused`.
    Register { tournament: String, corp: Box<DeckFile>, runner: Box<DeckFile>, salt: String, statement: Signed },
    /// Withdraw from a tournament while registration is open — or, in
    /// the rounds, drop from it: the entry and every result stay in the
    /// standings (`TournamentInfo::dropped`), the key is paired no more,
    /// and a table of the current round it has not played is forfeit to
    /// the opponent, who is stood up if waiting there. Refused while the
    /// key's game is under way — the board's Surrender writes that loss
    /// — and after the tournament is over. Answered with `Tournament` or
    /// `TournamentRefused`.
    Unregister { tournament: String },
    /// The organizer begins the next round (Phase 4 §7 stage 6b): the
    /// first closes registration — two entrants at least — and fixes the
    /// seeding, a shuffle of the entrants off the match seed; every one
    /// pairs single-sided Swiss over the rounds so far (`swiss::pair`),
    /// and is refused while a table of the current round has no result.
    /// Answered with `Tournament` to the organizer, and pushed to every
    /// entrant attached, or `TournamentRefused`.
    BeginRound { tournament: String },
    /// The organizer ends the tournament after a round is complete: the
    /// standings are final. Answered and pushed as `BeginRound` is.
    FinishTournament { tournament: String },
    /// The organizer, or an operator (`AdminRequest`), calls a
    /// tournament off at any point before it finishes (Phase 4 §12): it
    /// is `Cancelled` from now, its standings as they stood, no seat is
    /// given at a table and a game under way plays on with its result
    /// recorded nowhere. An operator may, because otherwise a
    /// tournament its organizer abandoned could only be ended by editing
    /// the store. Answered and pushed as `BeginRound` is.
    CancelTournament { tournament: String },
    /// Take the seat at this round's table: the game starts when the
    /// opponent sits too, dealt from the two registered lists on the
    /// sides the pairing gave, and shuffled from a seed no one party
    /// chose (Phase 4 §7 stage 6c): `seed_commitment` is the table's, as
    /// the player read it in `TournamentInfo::seeds` — refused if it is
    /// not the server's — and `nonce` is the player's half
    /// (`statements::nonce_is_valid`), told to nobody until the table's
    /// result reveals it with the server's secret. Answered with `Queued` while the opponent
    /// is awaited — `CancelSeek` stands up again — then `MatchJoined`; or
    /// `SeekRefused` for a key with no table this round, a table already
    /// played or playing, or a connection already looking or playing.
    Sit { tournament: String, seed_commitment: String, nonce: String },
    /// The organizer records a result for a table of the current round
    /// that has none and no game under way — a no-show, or a result the
    /// players agreed at the table (2.5.8). The organizer has the final
    /// say (3.2.1). Answered and pushed as `BeginRound` is.
    RecordResult { tournament: String, table: usize, outcome: swiss::Outcome },
    /// Offer the opponent at this round's table an intentional draw
    /// (Organized Play Policies 2.5.8): the offer is published to the
    /// table (`TournamentInfo::draw_offers`), and when both have offered
    /// the table's result is a tie, with a player waiting at it stood
    /// up. Refused for a key with no table this round, a table played or
    /// playing, or an offer already made; an offer lapses when the game
    /// starts, when a result is recorded and with the round. Answered
    /// and pushed as `BeginRound` is.
    OfferDraw { tournament: String },
    /// Every event the server holds, answered with `Events` (Phase 4
    /// §11).
    ListEvents,
    /// Hold an event: open from now, rolling — anyone enters any time and
    /// finds a match in its pool — in one of the server's formats, with
    /// a track of wins and what each milestone grants. **Only an
    /// operator's key** (`--operator`): an event grants titles and
    /// badges the profile wears everywhere on this server, so a key that
    /// could define "Champion · 1 win" would make every award worthless;
    /// a tournament, whose only prize is its own standings, is anyone's
    /// to hold. Only a daemon with a data directory holds one. Answered
    /// with `Event`, or `EventRefused`.
    HoldEvent { event: EventSpec },
    /// The operator closes an event: it takes no entry and no seek from
    /// now, and its page stays a while so the last tracks read. Answered
    /// with `Event` and pushed to every attached entrant, or
    /// `EventRefused`.
    CloseEvent { event: String },
    /// Enter an event: this key's track starts at nothing. Needs a proved
    /// key, since the track and the awards are the key's. Entering again
    /// is refused; quitting and entering starts over. Answered with
    /// `Event` or `EventRefused`.
    EnterEvent { event: String },
    /// Withdraw this key's entry: the track goes, and what it earned
    /// stays earned. A person who only wants to stop looking cancels the
    /// seek. Answered with `Event` or `EventRefused`.
    QuitEvent { event: String },
    /// An operator's request (Phase 4 §12), answered with `Admin` or
    /// `AdminRefused`.
    Admin(AdminRequest),
}

/// What an operator asks of a server (Phase 4 §12). **Only a key the
/// daemon's configuration names is answered** — `--operator`, or a line
/// of `operators.txt` in its data directory — over the attached
/// connection it proved; anyone else is refused with the reason, so a
/// client learns from the first `Status` whether its key operates here.
/// The organizer of a tournament keeps the scope it always had, and an
/// operator's call on a tournament is `ClientMessage::CancelTournament`
/// beside the organizer's own, not a request here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdminRequest {
    /// How the server stands. Answered with `AdminAnswer::Status`.
    Status,
    /// Grants `award` to `key`, as an event's milestone would, filed
    /// under the granting operator's key (`Award::source`, its
    /// `key:<base32>`). Refused for a key
    /// the server has never seen attach, which is a key mistyped far
    /// more often than one about to arrive. Answered with
    /// `AdminAnswer::Awards`.
    Grant { key: PublicKey, award: AwardSpec },
    /// Takes `award` back from `key`, whatever granted it; worn, it is
    /// taken off. Answered with `AdminAnswer::Awards`.
    Revoke { key: PublicKey, award: AwardSpec },
    /// Sets what tournament `tournament`'s final standings win, replacing
    /// what was set (`TournamentInfo::prizes`); empty clears them. An
    /// operator's and not the organizer's, as an event is, because a
    /// prize is an award the profile wears everywhere on this server and
    /// any key may organize. Refused once the tournament is over.
    /// Answered with `AdminAnswer::Tournament`, and pushed to its
    /// entrants and organizer as a `Tournament`.
    SetPrizes { tournament: String, prizes: Vec<Prize> },
    /// `key`'s rated games, newest first and at most `RESULTS_LIMIT`, as
    /// the store holds them — the ones that count, so a voided game is
    /// not among them. How an operator finds the game to void. Answered
    /// with `AdminAnswer::Results`.
    Results { key: PublicKey },
    /// Voids rated game `match_id` (Phase 4 §12 stage 3): it counts for
    /// nothing from now, and every rating is folded again from the
    /// results log without it, while the server runs. **The log is never
    /// edited** — every line is a receipt the server signed, and a
    /// player holds the same receipt — so the void is a line of its own
    /// beside it, with who voided it, when and why, which `--rebuild`
    /// honours too. A tournament's table or an event's track the game
    /// moved is not touched: those are their own records. Refused for a
    /// game the log holds no rated receipt of, and one voided already.
    /// Answered with `AdminAnswer::Voided`.
    Void { match_id: Uuid, reason: String },
}

/// The most games `AdminRequest::Results` answers with.
pub const RESULTS_LIMIT: usize = 20;

/// The answer to an `AdminRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdminAnswer {
    Status(ServerStatus),
    /// `key`'s awards as they now stand, after a grant or a revoke.
    Awards { key: PublicKey, titles: Vec<Award>, badges: Vec<Award> },
    /// A tournament as it now stands, after `SetPrizes`.
    Tournament(TournamentInfo),
    /// The answer to `Results`.
    Results { key: PublicKey, games: Vec<GameResult> },
    /// Game `match_id` voided, and the ratings folded again from
    /// `games` rated games that still count.
    Voided { match_id: Uuid, games: usize },
}

/// One rated game, as its receipt says it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameResult {
    pub match_id: Uuid,
    pub ended_at: u64,
    pub format: NsgFormat,
    pub corp: ResultSeat,
    pub runner: ResultSeat,
    /// A rated game always has one: a stall is nobody's and unrated.
    pub winner: Side,
}

/// A rated game's seat: the name it played under and its key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultSeat {
    pub name: String,
    pub key: PublicKey,
}

/// A server as its operator sees it: what it runs, and what it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerStatus {
    /// The build, as a receipt names it.
    pub engine: String,
    /// Seconds since the daemon bound.
    pub up_secs: u64,
    /// Whether it has a data directory: one that keeps nothing rates
    /// nothing and holds no tournament or event.
    pub keeps: bool,
    pub matches: usize,
    /// Seeks waiting in every lobby.
    pub waiting: usize,
    /// Connections attached, in a game or not.
    pub attached: usize,
    /// Tournaments neither finished nor cancelled.
    pub tournaments: usize,
    /// Events open.
    pub events: usize,
    pub operators: usize,
}

/// What an operator holds an event as (Phase 4 §11): the words a player
/// reads on its page, and the rules the server applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSpec {
    pub name: String,
    pub format: NsgFormat,
    /// Which factions an identity may be from, beyond the format; `None`
    /// admits every deck the format does. The first and only variant
    /// rule this stage; the next kind is a second check beside it.
    pub rule: Option<FactionRule>,
    /// Wins, in any order with any number of losses between, to clear the
    /// track. At least one.
    pub wins_to_clear: u32,
    /// What a win grants, by the win it is reached at — at most one
    /// milestone per win, each within `wins_to_clear`.
    pub milestones: Vec<Milestone>,
}

/// A title or a badge granted at the `wins`th win.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Milestone {
    pub wins: u32,
    pub award: AwardSpec,
}

/// A title or a badge by name: what an event grants and a profile wears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwardSpec {
    pub kind: AwardKind,
    pub name: String,
}

/// The two kinds of award (`Profile::titles`, `Profile::badges`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AwardKind {
    Title,
    Badge,
}

impl AwardKind {
    /// The word the store files it under.
    pub fn key(self) -> &'static str {
        match self {
            AwardKind::Title => "title",
            AwardKind::Badge => "badge",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            AwardKind::Title => "Title",
            AwardKind::Badge => "Badge",
        }
    }
}

impl EventSpec {
    /// Why the server would not hold this: a blank name, no win to clear,
    /// a milestone past the track or two at one win, a blank award.
    pub fn problem(&self) -> Option<String> {
        if self.name.trim().is_empty() {
            return Some("an event needs a name".into());
        }
        if self.wins_to_clear == 0 {
            return Some("an event takes at least one win to clear".into());
        }
        let mut seen = Vec::new();
        for milestone in &self.milestones {
            if milestone.wins == 0 || milestone.wins > self.wins_to_clear {
                return Some(format!("a milestone at win {} is off a track of {}", milestone.wins, self.wins_to_clear));
            }
            if seen.contains(&milestone.wins) {
                return Some(format!("two milestones at win {}", milestone.wins));
            }
            if milestone.award.name.trim().is_empty() {
                return Some(format!("the milestone at win {} names no {}", milestone.wins, milestone.award.kind.name().to_lowercase()));
            }
            seen.push(milestone.wins);
        }
        None
    }
}

/// An event as the server reports it to one connection (Phase 4 §11):
/// what it is, how it stands, and this key's own track in it. The
/// roster is not listed — a pool is joined, not watched — only counted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventInfo {
    /// A short code, as a tournament has.
    pub id: String,
    pub name: String,
    pub format: NsgFormat,
    pub rule: Option<FactionRule>,
    pub wins_to_clear: u32,
    /// By the win each is reached at.
    pub milestones: Vec<Milestone>,
    /// Taking entries and seeks; a closed event keeps its page a while.
    pub open: bool,
    pub created_at: u64,
    /// How many keys have entered.
    pub entered: usize,
    /// How many of them are looking for a match in its pool right now.
    pub looking: usize,
    /// This key's track, when it has entered.
    pub mine: Option<EventEntry>,
}

/// One key's track in an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEntry {
    pub wins: u32,
    pub losses: u32,
    pub entered_at: u64,
    /// When the last win landed, once it has.
    pub cleared_at: Option<u64>,
}

impl EventInfo {
    /// The milestone at `wins`, if any.
    pub fn milestone_at(&self, wins: u32) -> Option<&Milestone> {
        self.milestones.iter().find(|milestone| milestone.wins == wins)
    }
}

/// The id of the lobby an event's pool is: a seek there is paired within
/// the event alone (`event:<code>`).
pub fn event_lobby_id(code: &str) -> String {
    format!("event:{code}")
}

/// The event's code off its lobby's id, for a lobby that is one.
pub fn event_code(lobby_id: &str) -> Option<&str> {
    lobby_id.strip_prefix("event:")
}

/// Which chair a player looks for a game in, and the deck it needs: one
/// deck for a chair chosen, which is the hard constraint a queue honours
/// first, or one for each side, whose side the queue picks at pairing —
/// toward `prefer` where it can (Phase 4 §9). A preference is never a
/// refusal: a queue whose preferences could refuse a pairing can
/// deadlock, so the deck for the other side is always brought.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Chair {
    Corp(Box<DeckFile>),
    Runner(Box<DeckFile>),
    Either { corp: Box<DeckFile>, runner: Box<DeckFile>, prefer: Option<Side> },
}

/// Who a seek is to play (Phase 4 §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Opponent {
    /// Whoever the queue pairs: the only opponent Ranked has.
    Person,
    /// A bot at this rung of the difficulty ladder, in `style` — `None`
    /// for the style of the deck it is dealt. Asked only where the lobby
    /// offers that rung (`LobbyInfo::bots`), and seated at once: **a bot
    /// sits only in Casual, and only when a lobby asks for one** (decided
    /// 11 October 2026).
    Bot { level: Level, style: Option<Style> },
}

/// The three kinds of lobby a server has (Phase 4 §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum QueueMode {
    /// The server's rated queue for a format: only on a server that keeps
    /// a book, and never a bot.
    Ranked,
    /// The server's unrated queue for a format, where a player may ask for
    /// a bot.
    Casual,
    /// A player's own lobby (`ClientMessage::CreateLobby`): casual, never
    /// listed, joined by its code.
    Friends,
    /// An event's pool (Phase 4 §11, `event_lobby_id`): unrated, never a
    /// bot, never listed among the queues — the events page is the way
    /// in — and only an entrant seeks there.
    Event,
}

impl QueueMode {
    /// What a player calls it.
    pub fn name(self) -> &'static str {
        match self {
            QueueMode::Ranked => "Ranked",
            QueueMode::Casual => "Casual",
            QueueMode::Friends => "Friends",
            QueueMode::Event => "Event",
        }
    }
}

/// The id of a server's own queue in `mode` for `format`: the two names
/// in lower case (`ranked-standard`, `casual-startup`, …). A client joins
/// the queue it plays by this id, and a friend queue's code is never one.
pub fn queue_id(mode: QueueMode, format: NsgFormat) -> String {
    format!("{}-{format:?}", mode.name()).to_lowercase()
}

/// A key's standing in one format at a server: the server's ladders are
/// one per format, and a profile lists the ones the key has played in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormatStanding {
    pub format: NsgFormat,
    pub standing: Standing,
}

/// What a server keeps under a key (Phase 4 §10): the name it last
/// attached with, its standing on every ladder it has a rated game on,
/// in the formats' order, the titles and badges it has earned and the
/// ones it wears. The server's word, fetched and never stored by a
/// client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub first_seen: u64,
    pub last_seen: u64,
    pub standings: Vec<FormatStanding>,
    pub titles: Vec<Award>,
    pub badges: Vec<Award>,
    /// The title and badge worn, each one of `titles` / `badges` or none.
    pub title: Option<String>,
    pub badge: Option<String>,
}

impl Profile {
    /// This key's standing in `format` here, if it has one.
    pub fn in_format(&self, format: NsgFormat) -> Option<&Standing> {
        self.standings.iter().find(|entry| entry.format == format).map(|entry| &entry.standing)
    }
}

/// A title or a badge a key has earned at a server: its name, when, and
/// what granted it — `event:<code>` (§11), `tournament:<code>` or an
/// operator's key (§12). A
/// badge's picture is an asset slot named by its name (AGENTS.md §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Award {
    pub name: String,
    pub earned_at: u64,
    pub source: String,
}

/// One row of a server's ladder: a key, the name it last gave, its
/// standing in the ladder's format and what it wears.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LadderRow {
    pub key: PublicKey,
    pub name: String,
    pub standing: Standing,
    pub title: Option<String>,
}

/// How many rated games in a format a key needs before the ladder lists
/// it: keys are free, so a new one is provisional
/// (`docs/identity-and-rating.md` §5), and a deviation after ten games
/// has come down from 350 to under 150.
pub const LADDER_MIN_GAMES: u32 = 10;

/// The most rows a `Ladder` answer carries.
pub const LADDER_LIMIT: usize = 100;

/// A lobby as `Lobbies` and `LobbyJoined` report it: one of the server's
/// two queues for a format, or a friend queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyInfo {
    /// What `JoinLobby` names it by: `queue_id` for the server's own, a
    /// short code for a friend queue.
    pub id: String,
    /// The format's name for the server's own, the maker's for a friend
    /// queue.
    pub name: String,
    pub format: NsgFormat,
    pub mode: QueueMode,
    /// Joining asks for a password.
    pub password: bool,
    /// Whether a game paired here counts: the Ranked queue, which a server
    /// offers only when it keeps a book. A seat still plays unrated there
    /// when it proved no key, or when both chairs are one key; `rated` is
    /// what the lobby offers, and `Standing` is what this connection
    /// stands to gain.
    pub rated: bool,
    /// The rungs a seek here may ask for (`Opponent::Bot`), weakest first:
    /// none in the server's own queues, Ranked or Casual, none in a friend
    /// queue its maker kept bots out of, none in a format the server can
    /// deal a bot no deck in, and none on a server that offers no bots.
    pub bots: Vec<Level>,
    /// Everyone attached in the lobby, in the order they came, each with
    /// whether they are looking for a game and the side they prefer
    /// while they are (Phase 7 §15): a hosted game's room names its
    /// opponent and shows their Ready by it. A count is read off it
    /// (`players`, `seeking`).
    pub people: Vec<Presence>,
}

/// One attached connection in a lobby, as the lobby's listing shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    pub name: String,
    /// Looking for a game here: a seek submitted and not yet paired.
    pub seeking: bool,
    /// The side the seek prefers, while one is in.
    pub prefers: Option<Side>,
}

impl LobbyInfo {
    /// Attached connections in the lobby.
    pub fn players(&self) -> usize {
        self.people.len()
    }

    /// Of them, how many are looking for a game: the queue's count.
    pub fn seeking(&self) -> usize {
        self.people.iter().filter(|presence| presence.seeking).count()
    }

    /// The server's own queue, which never goes away.
    pub fn permanent(&self) -> bool {
        self.mode != QueueMode::Friends
    }
}

/// One running match as `ListMatches` reports it: the players' names and
/// the lobby. The seed is never on the wire, because it reproduces the
/// order of R&D.
///
/// **No decks** (Phase 4 §7 stage 2, 26 September 2026). The two deck ids
/// used to be here, so that a list could say what each match was while
/// the daemon rotated the sample pool. But a saved deck's id is a slug of
/// the name its builder gave it, so the list told anyone who asked what
/// every player had built and called it — and a spectator is one message
/// from telling a player. The identities are public the moment a game is
/// watched; the lists are nobody's but their players'.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSummary {
    pub match_id: Uuid,
    pub corp: String,
    pub runner: String,
    pub started_secs_ago: u64,
    /// The format of the lobby the match was paired in.
    pub format: NsgFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// The reply to `Identify`: sign this nonce, with this server's key
    /// inside what is signed, so the proof is good on this connection to
    /// this server and nowhere else.
    ///
    /// `lasting` says whether the server keeps its key from one run to the
    /// next (a daemon with a data directory). A client remembers a lasting
    /// key and says so loudly if it changes; a server with no data
    /// directory makes a new key every time it starts, and remembering
    /// that one would only ever cry wolf.
    Challenge { nonce: Nonce, server_key: PublicKey, lasting: bool },
    /// The proof holds: this connection is `key` until it closes.
    Identified { key: PublicKey },
    /// The proof does not hold, or came without a challenge. The socket
    /// is closed after it.
    IdentifyRefused { reason: String },
    /// Sign this: a `statements::SeatStatement` as its exact text, sent to
    /// a proved seat once, after its first `MatchJoined`. `salt` is the
    /// deck hash's, and only this seat is told it
    /// (`statements::deck_hash`).
    SignSeat { statement: String, salt: String },
    /// A rated game has ended: the server's signed `statements::Receipt`,
    /// and this seat's rating for the side it played, before and after.
    /// Sent after `GameEnded`, only to proved seats of a rated game.
    Rated { receipt: Box<Signed>, before: Rating, after: Rating },
    /// The reply to `MyProfile` and `SetProfile`: the key this connection
    /// proved, if it did, and what the server keeps under it. `key: None`
    /// for a connection that proved no key; `profile: None` for a server
    /// that keeps nothing, and for a key it has never seen attach.
    Profile { key: Option<PublicKey>, profile: Option<Profile> },
    /// A `SetProfile` the server would not take, and why.
    ProfileRefused { reason: String },
    /// The reply to `Ladder`: the keys with at least `LADDER_MIN_GAMES`
    /// rated games in `format`, best overall first, at most
    /// `LADDER_LIMIT` of them.
    Ladder { format: NsgFormat, rows: Vec<LadderRow> },
    /// The seat is taken. `session_token` is what `ClientMessage::Resume`
    /// presents to take it back after a dropped connection; it is per
    /// *seat*, not per match, so one player's token never reseats the
    /// other. Sent again, unchanged, on a successful resume.
    ///
    /// `corp_deck`/`runner_deck` name the deck each side plays
    /// (`decks::DeckFile::id`) — **but only the seat's own.** The other
    /// side's is always empty: a deck is its player's secret, and what
    /// the opponent learns of it is what the rules reveal (Phase 4 §7
    /// stage 2). The seat's own is there so a player who looked for a game
    /// in a random chair knows which of their two decks is played.
    MatchJoined {
        match_id: Uuid,
        assigned_side: Side,
        session_token: Uuid,
        corp_deck: String,
        runner_deck: String,
    },
    /// The reply to `Spectate`, before the first spectator `StateUpdate`.
    /// Its own variant rather than a `MatchJoined` with no side: that
    /// message's `assigned_side` and `session_token` are pinned by every
    /// reconnect test and `netrunner_client::connection`, and a spectator has
    /// neither. There is no token because there is nothing to resume —
    /// `Spectate` again is the whole of reconnecting.
    Spectating { match_id: Uuid },
    /// Looking for a game in a lobby, until another player is paired
    /// with this one: `position` is how many are waiting in this lobby's
    /// queue, this player included. The token is the one `MatchJoined`
    /// will carry.
    Queued { session_token: Uuid, position: usize },
    /// A match that cannot be watched — no such match, or it ended — after
    /// which the host closes the socket. Its own variant for the reason
    /// `ResumeRejected` is: the client is waiting to be seated and has no
    /// action to have rejected.
    ConnectRejected { reason: String },
    /// The reply to `ListMatches`. `waiting` is how many are looking for
    /// a game on the whole server, in every queue — a queue's own count is
    /// its `LobbyInfo::seeking` — and counts only waiters whose socket is
    /// still open, so a client (or a test) can poll it to see a dropped
    /// waiter go.
    MatchList { matches: Vec<MatchSummary>, waiting: usize, max_matches: Option<usize> },
    /// `ClientMessage::Resume` named a token the host does not hold: never
    /// issued, or its match already over — including a match that ended
    /// *because* this seat's grace period ran out. A client that missed
    /// the `GameEnded` learns it this way; the host keeps no record of
    /// finished matches, so it cannot say who won. Its own variant rather
    /// than `ActionRejected` because a resuming client is waiting for
    /// `MatchJoined` and nothing else — it has no action to have rejected.
    ResumeRejected { reason: String },
    /// Boxed — `ClientView` is by far the largest variant here, and this
    /// enum is passed around/cloned as a whole regardless of which variant
    /// is active.
    StateUpdate(Box<ClientView>),
    /// One resolved action, sent immediately after the `StateUpdate` it
    /// produced, so a client can render a running game log.
    ///
    /// **Per viewer, like `StateUpdate`.** The Corp's and the Runner's
    /// copies differ: the acting side's action and the engine's raw events
    /// name cards the other seat's view conceals (the Corp's facedown
    /// install, the HQ card the Runner just looked at), so each seat gets
    /// `Session::last_entry_for(side)` — masked by
    /// `netrunner_core::rules::masking` at the same boundary as the view.
    /// The full `HistoryEntry` never leaves the host.
    ///
    /// One message per action rather than the whole log each time: a
    /// `StateUpdate` is already per-action and the client already drains
    /// messages in a loop, so resending a growing log would cost O(n²)
    /// bytes over a match. Boxed for the same reason `StateUpdate` is — an
    /// entry carries the action's `Vec<GameEvent>`.
    ActionLog(Box<PublicHistoryEntry>),
    ActionRejected { reason: String },
    /// `side` has `remaining` to answer the decision it was just offered,
    /// or forfeits with `GameEndReason::TimedOut`. Sent to both seats when
    /// a clock starts, and again to a seat that reattaches mid-decision
    /// with what is left; never sent when the host runs without a clock,
    /// so a clock-less match's message sequence is exactly what it was.
    /// A message rather than a `ClientView` field: the view is the
    /// engine's, and `netrunner_core` knows no wall clock. One message per
    /// decision rather than ticks: the client can count down by itself.
    DecisionClock { side: Side, remaining: Duration },
    /// Time was called on the tournament round this game is played in
    /// (Organized Play Policies 1.1.5.3), during turn `turn`: that turn
    /// is finished, the other side takes one more, and then the game
    /// ends on agenda points (`GameEnded` with `GameEndReason::TimeCalled`,
    /// and no winner when they are even). Sent to both seats and the
    /// spectators once; never in a game outside a round.
    TimeCalled { turn: u32 },
    /// The game is over. `winner` is `None` only for a tie: time called
    /// on a tournament round with the agenda points even.
    GameEnded { winner: Option<Side>, reason: GameEndReason },
    /// What this seat's `TakeBack` would do now: `Free` or `Undo`
    /// (`netrunner_session::Rewind`), or `None` when there is no move of
    /// this seat's to take back — or the one there is would be an undo
    /// the lobby does not allow. Sent only when it changes, as a local
    /// match's `MatchMessage::Back` is, so a seat that ignores it misses
    /// nothing. A message and not a `ClientView` field because a
    /// take-back is the driver's, not the engine's: the view is what a
    /// seat is shown, and the engine knows no take-back.
    Back { rewind: Option<Rewind> },
    /// `by` took its last move back. Sent to every seat and spectator
    /// right after the `StateUpdate` of the state the move was made from,
    /// as `ActionLog` follows the state an action left: `removed` is how
    /// many `ActionLog` entries no longer happened, newest first — the
    /// same count for every viewer, since each got one entry per applied
    /// action — and `kind` what it cost `by`.
    TakenBack { by: Side, removed: usize, kind: Rewind },
    /// The reply to `Attach`: attached, and the open lobbies.
    Attached { lobbies: Vec<LobbyInfo> },
    /// The reply to `ListLobbies`.
    Lobbies { lobbies: Vec<LobbyInfo> },
    /// In the lobby now.
    LobbyJoined { lobby: LobbyInfo },
    /// `CreateLobby` or `JoinLobby` refused: no such lobby, the wrong
    /// password, a format the server does not offer.
    LobbyRefused { reason: String },
    /// Out of every lobby, after `LeaveLobby`.
    LobbyLeft,
    /// `Seek` refused: not in a lobby, a deck for the wrong side, a deck
    /// the lobby's format does not allow, already seeking or playing.
    SeekRefused { reason: String },
    /// No longer looking, after `CancelSeek`.
    SeekCancelled,
    /// The game this attached connection was playing has ended and it is
    /// back in its lobby, free to look for the next one.
    BackInLobby { lobby: Option<LobbyInfo> },
    /// The reply to `ListTournaments`.
    Tournaments { tournaments: Vec<TournamentInfo> },
    /// A tournament as it now stands: the answer to `CreateTournament`,
    /// `Register`, `Unregister`, `BeginRound`, `FinishTournament` and
    /// `RecordResult`, and **pushed unasked** to every attached entrant
    /// and the organizer when a round begins, a table's game ends, a
    /// result is recorded, a draw is offered, a player drops or the
    /// tournament finishes — so a player waiting on the page sees the
    /// pairing when it is posted.
    Tournament { tournament: TournamentInfo },
    /// One of those refused, with the reason: no key proved, a server
    /// that keeps nothing, no such tournament, a statement that does not
    /// hold, a deck the format refuses, not the organizer, a round still
    /// open.
    TournamentRefused { reason: String },
    /// The reply to `ListEvents`: every event the server holds, open ones
    /// first, and whether this connection's key may hold one (an
    /// operator's).
    Events { events: Vec<EventInfo>, operator: bool },
    /// An event as it now stands for this connection: the answer to
    /// `HoldEvent`, `EnterEvent`, `QuitEvent` and `CloseEvent`, and
    /// **pushed unasked** to an attached entrant when a game of theirs
    /// in it ends — their track moved, a milestone perhaps granted — and
    /// to every attached entrant when it closes.
    Event { event: EventInfo },
    /// One of those refused, with the reason: no key proved, not an
    /// operator, a server that keeps nothing, no such event, an event
    /// closed, entered already or not entered.
    EventRefused { reason: String },
    /// The answer to `ClientMessage::Admin`.
    Admin(AdminAnswer),
    /// An `Admin` request refused, with the reason: not an operator's
    /// key, a server that keeps nothing, a key never seen, an award not
    /// held.
    AdminRefused { reason: String },
}

/// A tournament as the server reports it (Phase 4 §7 stage 6). The
/// entrants are public — a pairing names them — and their lists are not:
/// each is named by its commitment alone until the event reveals it, as
/// decklists stay private through Swiss at a table (Organized Play
/// Policies 1.1.8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentInfo {
    /// A short code, as a player's lobby has.
    pub id: String,
    pub name: String,
    pub format: NsgFormat,
    /// The key that made it, which has the final say over it.
    pub organizer: PublicKey,
    pub state: TournamentState,
    /// In registration order.
    pub entrants: Vec<Entrant>,
    /// The entrants in the order the first round was paired from: a
    /// shuffle fixed when registration closed, which is every "random"
    /// the policies ask for (`swiss`). Empty while registering.
    pub seeding: Vec<PublicKey>,
    /// Every round paired so far, the current one last, each table's
    /// result filled in as its game ends. The standings are
    /// `swiss::standings(&seeding, &rounds)`, on either end.
    pub rounds: Vec<swiss::Round<PublicKey>>,
    /// Entrants who dropped mid-event (`ClientMessage::Unregister` in the
    /// rounds): still entrants, still in the standings with every result
    /// they have, paired no more. In the order they left.
    pub dropped: Vec<PublicKey>,
    /// Intentional draws offered at the current round's tables and not
    /// yet answered (`ClientMessage::OfferDraw`); the second offer at a
    /// table is the tie, and empties it. Nothing from an earlier round.
    pub draw_offers: Vec<DrawOffer>,
    /// Every table's seed so far (Phase 4 §7 stage 6c): the commitment to
    /// the server's secret, published as the round is paired, and the
    /// secret and both nonces once the table has a result. In round
    /// order, then table order.
    pub seeds: Vec<statements::TableSeed>,
    /// The current round's clock (Organized Play Policies 1.1.5.2: forty
    /// minutes for single-sided Swiss, the daemon's `--round-minutes`),
    /// set when the round begins. `None` while registering and after
    /// the end. A client counts down from it by its own clock; the
    /// server is what refuses a seat once time is called and runs the
    /// end-of-round rule inside each game.
    pub clock: Option<RoundClock>,
    /// What the final standings win (Phase 4 §12): a title or a badge
    /// per place, set by an operator and granted when the organizer
    /// finishes the tournament. By place, best first; empty, nothing
    /// but the standings.
    pub prizes: Vec<Prize>,
}

/// A title or a badge for the entrant who finishes `place`th (from 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prize {
    pub place: u32,
    pub award: AwardSpec,
}

impl Prize {
    /// Why the server would not take these: a place 0, two at one place,
    /// an award with no name. A place past the entrants is no problem —
    /// they are not known until registration closes — and goes to nobody.
    pub fn problem(prizes: &[Prize]) -> Option<String> {
        let mut seen = Vec::new();
        for prize in prizes {
            if prize.place == 0 {
                return Some("places count from 1".into());
            }
            if seen.contains(&prize.place) {
                return Some(format!("two prizes for place {}", prize.place));
            }
            if prize.award.name.trim().is_empty() {
                return Some(format!("the prize for place {} names no {}", prize.place, prize.award.kind.name().to_lowercase()));
            }
            seen.push(prize.place);
        }
        None
    }
}

/// When a round began and how long it runs, in seconds since the Unix
/// epoch and in seconds, so a client can say what is left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundClock {
    pub began_at: u64,
    pub seconds: u64,
}

impl RoundClock {
    /// Seconds left at `now`, `None` once time is called.
    pub fn remaining(self, now: u64) -> Option<u64> {
        let ends = self.began_at + self.seconds;
        (now < ends).then(|| ends - now)
    }
}

/// One player's standing offer of an intentional draw at a table of the
/// current round (Organized Play Policies 2.5.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawOffer {
    /// Into the current round's tables.
    pub table: usize,
    pub by: PublicKey,
}

impl TournamentInfo {
    /// The standings as they stand, best first.
    pub fn standings(&self) -> Vec<swiss::Standing<PublicKey>> {
        swiss::standings(&self.seeding, &self.rounds)
    }

    /// The round being played, if one is.
    pub fn current_round(&self) -> Option<&swiss::Round<PublicKey>> {
        match self.state {
            TournamentState::Playing { round } => self.rounds.get(round as usize - 1),
            TournamentState::Registering | TournamentState::Finished | TournamentState::Cancelled => None,
        }
    }
}

/// Where a tournament is. An exhaustive match on this is what makes a
/// later stage (the cut) add its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TournamentState {
    /// Taking registrations.
    Registering,
    /// Round `round` (from 1) is paired and being played; the next is
    /// begun by the organizer once every table has a result.
    Playing { round: u32 },
    /// The organizer ended it: the standings are final.
    Finished,
    /// Called off before it finished, by the organizer or an operator
    /// (Phase 4 §12): the standings are as they stood, and final.
    Cancelled,
}

/// One entrant: who, and the commitment to the two decks the server holds
/// for them — the `statements::RegistrationStatement` they signed, whose
/// two hashes are repeated here for a reader that does not parse it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrant {
    /// The name the key attached with when it registered. A label.
    pub name: String,
    pub key: PublicKey,
    pub corp_hash: String,
    pub runner_hash: String,
    pub commitment: Signed,
}
