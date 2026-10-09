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

use netrunner_core::decks::DeckFile;
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
    /// This connection's standing at the server: answered with
    /// `Standing`, first thing on a socket (after proving a key) or from
    /// an attached connection — which asks when it attaches and again
    /// when a game puts it back in its lobby, so the Server page shows
    /// the number the game just moved. Never cached by the client as the
    /// truth — the server's book is.
    MyStanding,
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
    /// lobby pairs whoever is waiting in it, and a closed lobby is how two
    /// people who already know each other meet. Listing the waiters and
    /// challenging one was proposed and declined.
    Attach { player_name: String },
    /// The open lobbies, answered with `Lobbies`. A closed lobby is never
    /// listed; it is joined by its id.
    ListLobbies,
    /// Make a lobby and join it: open (listed) or closed (joined by its
    /// id, and by `password` when one is set). Answered with
    /// `LobbyJoined`, whose id is the one to share, or `LobbyRefused`.
    ///
    /// `casual` makes a lobby whose games count for nothing, on a server
    /// that would otherwise rate them (Phase 4 §7 stage 5): two friends
    /// trying a deck, or a game nobody wants on their record. It is the
    /// lobby's property, not a seat's, so both players know before they
    /// look. A rated lobby is what a server's own lobbies are when it
    /// keeps a book, and `casual: false` on a server that keeps none is
    /// still unrated — `LobbyInfo::rated` says what the server will do.
    CreateLobby { name: String, format: NsgFormat, closed: bool, password: Option<String>, casual: bool },
    /// Join a lobby by its id — a listed one, or a closed one given by its
    /// maker — leaving the one this connection was in. A password is
    /// asked of a lobby made with one. Answered with `LobbyJoined` or
    /// `LobbyRefused`.
    JoinLobby { lobby: String, password: Option<String> },
    LeaveLobby,
    /// Look for a game in the lobby joined, with the deck or decks the
    /// chair needs. Answered with `Queued`, then `MatchJoined` when the
    /// lobby pairs this player; or `SeekRefused`. A second seek while one
    /// is open is refused: `CancelSeek` first, and the new one is a new
    /// place in the queue. The decks go no further than the server, which
    /// deals from them and tells nobody else what they are.
    Seek { chair: Chair },
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
    /// Take the seat at this round's table: the game starts when the
    /// opponent sits too, dealt from the two registered lists on the
    /// sides the pairing gave. Answered with `Queued` while the opponent
    /// is awaited — `CancelSeek` stands up again — then `MatchJoined`; or
    /// `SeekRefused` for a key with no table this round, a table already
    /// played or playing, or a connection already looking or playing.
    Sit { tournament: String },
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
}

/// Which chair a player looks for a game in, and the deck it needs: one
/// deck for a chair chosen, one for each side for a random one, whose
/// side the server picks at pairing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Chair {
    Corp(Box<DeckFile>),
    Runner(Box<DeckFile>),
    Random { corp: Box<DeckFile>, runner: Box<DeckFile> },
}

/// The id of a server's own lobby for `format`: its name in lower case
/// (`startup`, `standard`, …). A client joins the lobby for the format it
/// plays by this id, and a player's lobby never has one of these ids.
pub fn format_lobby_id(format: NsgFormat) -> String {
    format!("{format:?}").to_lowercase()
}

/// A lobby as `Lobbies` and `LobbyJoined` report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyInfo {
    /// What `JoinLobby` names it by: a format's name for the server's own
    /// lobby, a short code for a player's.
    pub id: String,
    pub name: String,
    pub format: NsgFormat,
    /// The server's own lobby for its format, which never goes away.
    pub permanent: bool,
    /// Not listed; joined by its id.
    pub closed: bool,
    /// Joining asks for a password.
    pub password: bool,
    /// Whether a game paired here counts: the server keeps a book and
    /// the lobby was not made casual. A seat still plays unrated in a
    /// rated lobby when it proved no key, or when both chairs are one
    /// key; `rated` is what the lobby offers, and `Standing` is what
    /// this connection stands to gain.
    pub rated: bool,
    /// Attached connections in the lobby.
    pub players: usize,
    /// Of them, how many are looking for a game.
    pub seeking: usize,
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
    /// The reply to `MyStanding`: the key this connection proved, if it
    /// did, and its standing on the server's book between people. `None`
    /// for a connection that proved no key, on a server that keeps no
    /// book, or for a key with no rated game there yet.
    Standing { key: Option<PublicKey>, standing: Option<Standing> },
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
    /// with this one: `position` is how many are waiting on the server,
    /// this player included. The token is the one `MatchJoined` will
    /// carry.
    Queued { session_token: Uuid, position: usize },
    /// A match that cannot be watched — no such match, or it ended — after
    /// which the host closes the socket. Its own variant for the reason
    /// `ResumeRejected` is: the client is waiting to be seated and has no
    /// action to have rejected.
    ConnectRejected { reason: String },
    /// The reply to `ListMatches`. `waiting_in_lobby` counts only waiters
    /// whose socket is still open, so a client (or a test) can poll it to
    /// see a dropped waiter go.
    MatchList { matches: Vec<MatchSummary>, waiting_in_lobby: usize, max_matches: Option<usize> },
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
    /// The current round's clock (Organized Play Policies 1.1.5.2: forty
    /// minutes for single-sided Swiss, the daemon's `--round-minutes`),
    /// set when the round begins. `None` while registering and after
    /// the end. A client counts down from it by its own clock; the
    /// server is what refuses a seat once time is called and runs the
    /// end-of-round rule inside each game.
    pub clock: Option<RoundClock>,
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
            TournamentState::Registering | TournamentState::Finished => None,
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
