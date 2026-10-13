//! `--serve` mode: a WebSocket daemon that seats remote clients into
//! `MatchSession`s. Library code rather than part of `main.rs` so a test can
//! bind an ephemeral port and drive real sockets through the whole
//! handshake — connect, queue, pair, drop, resume — which is the only
//! honest test of a connection protocol.
//!
//! **One `Registry`, one lock.** Every piece of daemon bookkeeping — the
//! running matches, the seat tokens, the lobby queue and the counter the
//! seed policy reads — sits behind a single `std` mutex. Separate locks
//! (the lobby used to have its own `tokio::sync::Mutex`) were rejected
//! because the operations that matter are compound: a pairing must sweep
//! the lobby, check the cap, claim a match index and register two tickets
//! as one step, or two simultaneous pairings both pass the cap and a
//! resume can find a ticket that is half-registered. A `std` mutex rather
//! than `tokio`'s because nothing awaits while holding it; the longest
//! critical section is a `GameState::setup` (two 45-card shuffles), which
//! is nothing.
//!
//! **Seat tokens.** Every channel seat gets a `Uuid` and a `SeatTicket` in
//! the registry for as long as its match runs. `ClientMessage::Resume {
//! session_token }` looks the ticket up, builds a fresh channel pair and
//! bridge for the new socket, sends `MatchJoined` on it and hands the pair
//! to the session through its `ReattachHandle`. Tickets are keyed by
//! token, not by match, because a token is a *seat's* credential: the
//! Corp's token can never reseat the Runner. They are removed when the
//! session returns, so a resume after the game ended is refused with
//! `ResumeRejected` rather than answered with a `MatchJoined` for a match
//! that no longer exists.
//!
//! **Every connection is attached** (`attached`): a client attaches with
//! a name, browses and joins lobbies, and brings a deck only to look for
//! a game. The one-shot `Connect` that carried a deck and closed with the
//! game was removed before anything was released (26 September 2026).
//!
//! **Two queues per format, and the players' own** (Phase 4 §9). A daemon
//! offers one or more formats (`ServeOptions::formats`), and for each a
//! Ranked queue — rated, so only on a daemon that keeps a book — and a
//! Casual one, which never is; any player may make a friend queue, joined
//! by its code and never listed. A player is paired only within their
//! lobby, their deck checked against its format — so one public server
//! holds a Startup queue and a Standard queue side by side. There is no
//! list of waiters to pick an opponent from (declined, 26 September
//! 2026): a friend queue is how two people who know each other meet.
//! **A bot sits only in a friend lobby that lets bots in, and only when
//! a seek there asks for one** (`Opponent::Bot`; decided 11 October 2026
//! for Casual, narrowed to friend lobbies on 12 October with the
//! dashboard, whose path names no bot); until §9 the daemon seated one
//! against every seek in every lobby.
//!
//! **A key is who a player is** (Phase 4 §5). Before attaching, a
//! connection may prove a key: `Identify`, a `Challenge` naming a nonce
//! fresh for this socket and the daemon's own key, and a `Prove` signing
//! both, checked in the handshake loop before anything else is committed
//! to. A proved connection is rated under `key:<base32>`, and its name is
//! a label (`Players`); an unproved one plays, unrated. A daemon's key
//! lives in its data directory (`ServeOptions::data_dir`), or is made
//! fresh at bind when it has none.
//!
//! **The lobby is a queue, not a slot.** A `Seek` either pairs with a
//! waiter in the same lobby — whom, and in which chairs, is the pure
//! `crate::queue`'s to say: the hard side, the preferences, then the swap
//! of the chairs two keys last took — or joins the queue and is told so
//! (`ServerMessage::Queued`). The token issued there is the one
//! `MatchJoined` carries, and a seek is withdrawn with its socket: a
//! queue place is not a seat with a game in it, so it gets no grace.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

use netrunner_bots::{Knowledge, Level, Style};
use netrunner_core::cards::CardRegistry;
use netrunner_core::decks::{self, DeckCategory, DeckFile};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::{Deck, DeckOrder, GameState, MatchRules, Side};
use netrunner_identity::{Identity, Nonce, PublicKey, Signature, Signed};
use netrunner_rating::Rating;
use netrunner_rating::{Outcome, RatingBook};
use netrunner_store::{ResultRow, Store, TournamentRow};
use netrunner_session::{MatchRecordHeader, RecordedBot};

use crate::match_session::{Finished, MatchSession, PlayerSlot, ReattachHandle, TurnTimeout, DEFAULT_RECONNECT_GRACE};
use crate::protocol::statements::{self, Receipt, ReceiptSeat, RegistrationStatement, SeatStatement, SeedReveal, TableSeat, TableSeed, RECEIPT_TAG, REGISTRATION_TAG, SEAT_TAG};
use crate::protocol::swiss::{self, Outcome as TableOutcome, Role as TableRole};
use crate::protocol::{DrawOffer, RoundClock};
use crate::protocol::{queue_id, Award, Chair, ClientMessage, Opponent, QueueMode, Entrant, FormatStanding, LadderRow, LobbyInfo, MatchSummary, Presence, Profile, ServerMessage, TournamentInfo, TournamentState, LADDER_LIMIT, LADDER_MIN_GAMES};
use crate::fixtures::DealtMatchup;
use crate::queue::{self, Wants};
use crate::{fixtures, net};

#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// The rungs of the difficulty ladder this daemon offers (Phase 4
    /// §9): what a seek in a friend lobby that lets bots in may ask to
    /// play (`Opponent::Bot`), weakest first, each the same bot as a local
    /// game's rung of that name. **Never where a bot sits**: a bot sits
    /// only in such a lobby and only when a seek asks for one (decided 11
    /// October 2026, narrowed from Casual on 12 October) — until §9,
    /// `--bot-runner` seated one against every seek in every lobby, so a
    /// person on a default daemon never met another. Empty offers none.
    /// Every rung by default.
    pub bots: Vec<Level>,
    /// Base seed every match's seed is derived from (`base + match
    /// index`, the headless driver's policy). `None` picks one at random.
    pub seed: Option<u64>,
    /// See `MatchSession::with_reconnect_grace`.
    pub reconnect_grace: Duration,
    /// How many matches may run at once; `None` is no limit. At the cap a
    /// `Connect` is answered with `ConnectRejected` and the socket closed,
    /// in both modes — a human-vs-human `Connect` is refused *before* it
    /// is queued, since a queue entry that cannot be paired is a promise
    /// the daemon cannot keep. Queueing past the cap and pairing as
    /// matches end was rejected: it needs a wake-up from the session-exit
    /// task, and nobody has asked to wait.
    pub max_matches: Option<usize>,
    /// How many spectators each match takes; `None` is no limit. See
    /// `MatchSession::DEFAULT_MAX_SPECTATORS` for why there is one.
    pub max_spectators: Option<usize>,
    /// See `MatchSession::with_turn_timeout`; `None` runs without a clock.
    pub turn_timeout: TurnTimeout,
    /// How long a tournament round runs before time is called
    /// (`ClientMessage::BeginRound` sets each round's clock from it):
    /// forty minutes, single-sided Swiss's (Organized Play Policies
    /// 1.1.5.2). Once called, no seat is given at a table of the round,
    /// and a game under way finishes the turn in progress and one more
    /// (1.1.5.3, `MatchSession::with_round_end`).
    pub round_length: Duration,
    /// Pin the matchup instead of rotating: a published decklist id per
    /// side (`decks::by_id`), resolved once at `bind` so a bad name is a
    /// startup error rather than a per-connection `ConnectRejected`.
    /// `None` on a side leaves that side rotating with the match seed, so
    /// pinning one and rotating the other is a legal — and occasionally
    /// useful — configuration.
    pub corp_deck: Option<String>,
    pub runner_deck: Option<String>,
    /// The lobbies: every format this daemon pairs players in, each a
    /// queue of its own. A `Connect` naming no format joins the first,
    /// which is what a client built before lobbies does. Every deck the
    /// daemon can deal — pinned or rotating — must be legal in all of
    /// them, checked once at `bind`. Every format by default, Startup
    /// first, matching `netrunner_cli --format`'s default; a game hosted
    /// from a client's menu offers only the host's.
    pub formats: Vec<NsgFormat>,
    /// Where the daemon keeps what outlives it (Phase 4 §5, §10): its own
    /// key (`identity.key`, made on first start, mode 0600), the store
    /// (`store.sqlite`, made on first start unless `database` names
    /// another: the players it has seen, a ladder per format, the
    /// results, the tournaments), the results log (`results.jsonl`) and
    /// every match's record (`matches/<yyyy-mm>/<match id>.jsonl`).
    ///
    /// `None` is a stateless daemon, which is what every test and every
    /// game hosted from a client's menu wants: a key made fresh at bind,
    /// which the `Challenge` calls not `lasting` so no client remembers
    /// it, and nothing rated.
    ///
    /// **Only a match between two identified people is rated**, and
    /// between two different keys, in a lobby that rates
    /// (`LobbyInfo::rated`: the server's own, and a player's unless made
    /// casual — Phase 4 §7 stage 5). A game against a seated bot is
    /// practice wherever it is played (`netrunner_rating::Track`), and an
    /// unidentified seat plays unrated: a rating is filed under a key the
    /// player proved, never under the name they typed.
    pub data_dir: Option<PathBuf>,
    /// The store's database, as a URL (`postgres://…`, or a
    /// `sqlite://…` elsewhere than the data directory). `None` with a
    /// data directory is the zero-setup default: an SQLite file in it
    /// (`netrunner_store::SQLITE_FILE`). Ignored with no data directory,
    /// which keeps nothing. Phase 4 §10.
    pub database: Option<String>,
}

impl Default for ServeOptions {
    fn default() -> Self {
        ServeOptions {
            bots: Level::ALL.to_vec(),
            seed: None,
            reconnect_grace: DEFAULT_RECONNECT_GRACE,
            max_matches: None,
            max_spectators: Some(crate::match_session::DEFAULT_MAX_SPECTATORS),
            turn_timeout: None,
            round_length: DEFAULT_ROUND_LENGTH,
            corp_deck: None,
            runner_deck: None,
            formats: DEFAULT_FORMATS.to_vec(),
            data_dir: None,
            database: None,
        }
    }
}

/// Single-sided Swiss's round (Organized Play Policies 1.1.5.2).
pub const DEFAULT_ROUND_LENGTH: Duration = Duration::from_secs(40 * 60);

/// The formats a daemon offers by default, in lobby order: all five
/// (Phase 4 §8, 11 October 2026). Snapshot is a Fantasy Flight Games pool
/// no shipped deck is legal in (Phase 1 §9 Stage 0b), so a daemon can deal
/// a bot nothing there and serves it for people alone, who bring their own
/// decks (`deals_by_format`, `Registry::bots_by_format`). Until today
/// Snapshot was left out for that reason, and the fourth was Unrestricted
/// under its old name, Casual.
pub const DEFAULT_FORMATS: [NsgFormat; 5] = NsgFormat::ALL;

/// A pinned decklist per side, each `None` if that side rotates.
#[derive(Clone, Default)]
struct PinnedDecks {
    corp: Option<(String, Deck)>,
    runner: Option<(String, Deck)>,
}

/// Resolves one `--corp-deck`/`--runner-deck` id against the embedded
/// pool, refusing a deck of the wrong side. The error lists what is
/// available, matching the convention the CLI's deck flags already set —
/// a daemon operator naming a deck that does not exist should not have to
/// go and read `data/decks/`.
///
/// **Legality is checked here too**, which the doc comment above this
/// function used to claim while nothing did it: a pinned deck was taken on
/// its name and side alone, so a daemon could serve an illegal matchup for
/// as long as it ran. `DeckFile::validate` is the same gate
/// `netrunner_cli` puts a saved deck through before starting a game, so
/// "this deck is playable" means one thing in both.
fn pin_deck(
    name: Option<&str>,
    side: Side,
    registry: &CardRegistry,
    format: NsgFormat,
) -> std::io::Result<Option<(String, Deck)>> {
    let Some(name) = name else { return Ok(None) };
    let available = || {
        decks::for_side(side)
            .into_iter()
            .filter(|deck| deck.category == DeckCategory::Sample)
            .map(|deck| deck.id)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let Some(deck) = decks::by_id(name) else {
        return Err(std::io::Error::other(format!("no deck named {name:?}; available {side:?} decks: {}", available())));
    };
    if deck.side != side {
        return Err(std::io::Error::other(format!("deck {name:?} is a {:?} deck, not {side:?}", deck.side)));
    }
    if let Err(e) = deck.validate(registry, format) {
        return Err(std::io::Error::other(format!("deck {name:?} is not legal in {format:?}: {e}")));
    }
    Ok(Some((deck.id.clone(), deck.to_deck())))
}

/// The matchups of the rotating pool a daemon may deal in `format`: the
/// indices into `decks::matchups()` whose two decks are both legal there,
/// worked out once at `bind`.
///
/// **A format deals from the part of the pool it allows, and refuses to
/// start only when that part is empty and a bot will need a deal.** It used
/// to refuse outright when any sample deck was illegal, which was sound
/// while every format's tables were a seed that allowed them all. With
/// NetrunnerDB's lists (Phase 1 §9 Stage 0b) thirteen published decks are
/// not Startup-legal, and Snapshot — a Fantasy Flight Games pool — allows
/// none. A Startup lobby now deals among the Startup-legal matchups, and a
/// human-vs-human daemon, whose seats bring their own decks, needs no
/// deal at all.
fn legal_matchups(registry: &CardRegistry, format: NsgFormat) -> Vec<usize> {
    let legal: HashMap<String, bool> = [Side::Corp, Side::Runner]
        .into_iter()
        .flat_map(decks::for_side)
        .filter(|deck| deck.category == DeckCategory::Sample)
        .map(|deck| {
            let ok = deck.validate(registry, format).is_ok();
            (deck.id, ok)
        })
        .collect();
    decks::matchups()
        .iter()
        .enumerate()
        .filter(|(_, (corp, runner))| legal.get(&corp.id).copied().unwrap_or(false) && legal.get(&runner.id).copied().unwrap_or(false))
        .map(|(index, _)| index)
        .collect()
}

/// `legal_matchups` for every format the daemon serves. A format that
/// allows no matchup is served all the same — for people, who bring their
/// own decks — and its lobbies offer no bot (`Registry::bots_by_format`);
/// the daemon refuses to start only when it offers bots and *no* served
/// format can deal one, because then the bots it offers are offered
/// nowhere, which is not what the operator asked for.
fn deals_by_format(registry: &CardRegistry, options: &ServeOptions) -> std::io::Result<HashMap<NsgFormat, Vec<usize>>> {
    let deals: HashMap<NsgFormat, Vec<usize>> = options.formats.iter().map(|&format| (format, legal_matchups(registry, format))).collect();
    if !options.bots.is_empty() && deals.values().all(Vec::is_empty) {
        return Err(std::io::Error::other(format!(
            "no sample matchup is legal in any of {:?}, and the daemon offers bots, which would be dealt one; pin a legal matchup, serve another format or offer no bots",
            options.formats
        )));
    }
    Ok(deals)
}

/// One seat's reattach credentials, held for as long as its match runs.
///
/// Carries the two decklist ids so a `Resume` can send the same
/// `MatchJoined` the seat first received (`SeatTicket::joined`, which
/// tells the seat its own and never the other's) — a reconnecting client must not
/// have to remember what it was dealt, and reading them back off
/// `MatchEntry` would mean holding two lookups under one lock for two
/// short strings.
#[derive(Clone)]
struct SeatTicket {
    match_id: Uuid,
    side: Side,
    corp_deck: String,
    runner_deck: String,
    handle: ReattachHandle,
    /// The lobby the seat's connection returns to after the game, which
    /// a resumed connection returns to too.
    lobby: String,
    /// The seat's current connection, replaced when it is resumed: where
    /// `Rated` goes once the match has let go of the seat.
    tx: mpsc::UnboundedSender<ServerMessage>,
}

impl SeatTicket {
    /// The `MatchJoined` this seat is told: its own deck's id, and the
    /// other side's left empty. The one place the message is built, for
    /// the first seating and every resume, so neither can tell a seat
    /// its opponent's deck — a brought deck's id is a slug of the name
    /// its builder gave it.
    fn joined(&self, session_token: Uuid) -> ServerMessage {
        let (corp_deck, runner_deck) = match self.side {
            Side::Corp => (self.corp_deck.clone(), String::new()),
            Side::Runner => (String::new(), self.runner_deck.clone()),
        };
        ServerMessage::MatchJoined { match_id: self.match_id, assigned_side: self.side, session_token, corp_deck, runner_deck }
    }
}

mod attached;
mod store;

use store::Writer;

/// A running match as `MatchList` reports it. Holds the session's
/// `ReattachHandle` so a `Spectate { match_id }` can reach the pump; the
/// seed is deliberately *not* here — it reproduces R&D's order, so it
/// must never leave the host — and neither are the decks, which are
/// nobody's but their players' (`MatchSummary`).
struct MatchEntry {
    corp: String,
    runner: String,
    format: NsgFormat,
    started_at: Instant,
    handle: ReattachHandle,
    /// What each proved seat was asked to sign, Corp then Runner, and its
    /// signature once it arrives (`Shared::accept_commitment`).
    commitments: [Option<Commitment>; 2],
}

/// A seat statement sent to be signed.
struct Commitment {
    key: PublicKey,
    payload: String,
    signed: Option<Signed>,
}

fn seat_index(side: Side) -> usize {
    match side {
        Side::Corp => 0,
        Side::Runner => 1,
    }
}

/// An attached connection looking for a game against a person (`Seek`),
/// waiting for another in the same lobby.
struct PendingHuman {
    token: Uuid,
    /// The connection's link (`Registry::attached`), which is how the
    /// lobby's listing marks its member as looking (`Presence::seeking`).
    link: Uuid,
    player_name: String,
    /// The key this connection proved, if it identified: what the rating
    /// book files the player under.
    key: Option<PublicKey>,
    /// The lobby's id: a waiter pairs only within its own, and returns to
    /// it after the game.
    lobby: String,
    /// The lobby's format, which every deck here was checked against.
    format: NsgFormat,
    /// A chosen chair's deck, already checked; its side is the seat.
    deck: Option<Box<DeckFile>>,
    /// Either chair's two decks, Corp then Runner: the side is picked at
    /// pairing (`queue::seat`) and the deck for it is played.
    random: Option<(Box<DeckFile>, Box<DeckFile>)>,
    /// Either chair's preferred side, if it named one.
    prefer: Option<Side>,
    /// When it joined the queue, in seconds since the Unix epoch: how
    /// long its preference still counts (`queue::PATIENCE`).
    since: u64,
    tx: mpsc::UnboundedSender<ServerMessage>,
    slot: PlayerSlot,
}

impl PendingHuman {
    /// Seated on `side`: a random chair's deck for that side becomes the
    /// deck played.
    fn seated(self, side: Side) -> SeatedPlayer {
        let deck = match self.random {
            Some((corp, runner)) => Some(match side {
                Side::Corp => corp,
                Side::Runner => runner,
            }),
            None => self.deck,
        };
        SeatedPlayer { key: self.key, name: self.player_name, token: self.token, slot: self.slot, deck, lobby: Some(self.lobby), bot: None }
    }

    /// The seek as the queue weighs it.
    fn seek(&self) -> queue::Seek {
        let wants = match &self.deck {
            Some(deck) if deck.side == Side::Corp => Wants::Corp,
            Some(_) => Wants::Runner,
            None => Wants::Either { prefer: self.prefer },
        };
        queue::Seek { wants, key: self.key.map(|key| key.rating_id()), since: self.since }
    }
}


/// A friend queue a player made (`ClientMessage::CreateLobby`): casual,
/// and joined by its code. The server's own queues are not stored: they
/// are `server_queues`.
struct PlayerLobby {
    name: String,
    format: NsgFormat,
    /// Held in memory only and never sent: a lobby's password is a
    /// door for a few friends, not an account's.
    password: Option<String>,
    /// Its members may ask for a bot (`CreateLobby::bots`).
    bots: bool,
}

/// The server's own queues, in the order they are listed: every format's
/// Ranked queue — only on a daemon that keeps a book, since nothing else
/// could rate it — then every format's Casual one.
fn server_queues(options: &ServeOptions) -> impl Iterator<Item = (QueueMode, NsgFormat)> + '_ {
    let ranked = options.data_dir.is_some();
    let modes = [QueueMode::Ranked, QueueMode::Casual].into_iter().filter(move |&mode| ranked || mode != QueueMode::Ranked);
    modes.flat_map(|mode| options.formats.iter().map(move |&format| (mode, format)))
}

/// A tournament the daemon holds (Phase 4 §7 stage 6a): what
/// `ClientMessage::CreateTournament` made, with every entrant's two decks
/// — which the daemon deals from and never sends — beside the commitment
/// that names them. Kept whole as one document in the store's tournaments
/// table, written on every change (`Shared::save_tournament`): a
/// tournament outlives any socket, and a daemon restarted
/// mid-registration must still hold the lists it promised to deal from.
#[derive(serde::Serialize, serde::Deserialize)]
struct Tournament {
    name: String,
    format: NsgFormat,
    /// The key that made it, which the policies give the final say.
    organizer: PublicKey,
    created_at: u64,
    state: TournamentState,
    /// By the entrant's rating id, in registration order.
    entrants: Vec<Entry>,
    /// The entrants' keys in the order the first round was paired from: a
    /// shuffle off the daemon's seed when registration closed, which is
    /// every "random" the policies ask for (`swiss`). Empty until then.
    seeding: Vec<PublicKey>,
    /// Every round paired so far, each table's result written as its
    /// game ends or the organizer records one (Phase 4 §7 stage 6b).
    rounds: Vec<swiss::Round<PublicKey>>,
    /// Who dropped mid-event: still in `entrants` and the standings,
    /// paired no more. A drop's entry stays because the standings need
    /// its name and its opponents' tiebreakers need its games.
    dropped: Vec<PublicKey>,
    /// Intentional draws offered at the current round's tables and not
    /// yet matched: cleared for a table when its result is written, and
    /// whole when a round begins.
    draw_offers: Vec<DrawOffer>,
    /// The current round's clock, set when it begins.
    clock: Option<RoundClock>,
    /// Every table's seed so far (Phase 4 §7 stage 6c): the server's
    /// secret, made as the round is paired, and the nonces its game was
    /// seeded with once it starts. Only the secret's hash leaves the
    /// server until the table has a result (`info`).
    seeds: Vec<TableSecret>,
}

/// One table's half of the seed the server holds: its secret, never sent
/// until the table has a result, and the two nonces the players sat with
/// (Corp, Runner), once both have and the game has started.
#[derive(serde::Serialize, serde::Deserialize)]
struct TableSecret {
    round: usize,
    table: usize,
    secret: String,
    nonces: Option<[String; 2]>,
}

impl TableSecret {
    /// The table's seed as the wire carries it: the commitment always,
    /// the reveal once `revealed` — the table has a result.
    fn published(&self, revealed: bool) -> TableSeed {
        let reveal = revealed.then(|| SeedReveal {
            secret: self.secret.clone(),
            corp_nonce: self.nonces.as_ref().map(|[corp, _]| corp.clone()),
            runner_nonce: self.nonces.as_ref().map(|[_, runner]| runner.clone()),
        });
        TableSeed { round: self.round, table: self.table, commitment: statements::seed_commitment(&self.secret), reveal }
    }
}

/// One entrant's registration: the decks the server holds for them, the
/// salt both sides know, and the signed statement that commits to both.
#[derive(serde::Serialize, serde::Deserialize)]
struct Entry {
    name: String,
    key: PublicKey,
    corp: DeckFile,
    runner: DeckFile,
    salt: String,
    commitment: Signed,
}

impl Tournament {
    fn entry(&self, key: &PublicKey) -> Option<&Entry> {
        self.entrants.iter().find(|entry| entry.key == *key)
    }

    /// Whether the current round's time has been called.
    fn time_called(&self) -> bool {
        self.clock.is_some_and(|clock| clock.remaining(unix_now()).is_none())
    }

    /// When the current round's time is called, on the monotonic clock
    /// a match task sleeps against.
    fn round_end(&self) -> Option<tokio::time::Instant> {
        self.clock.map(|clock| tokio::time::Instant::now() + Duration::from_secs(clock.remaining(unix_now()).unwrap_or(0)))
    }

    /// The round being played, by its index into `rounds`.
    fn current(&self) -> Option<(usize, &swiss::Round<PublicKey>)> {
        match self.state {
            TournamentState::Playing { round } => {
                let index = round as usize - 1;
                self.rounds.get(index).map(|current| (index, current))
            }
            TournamentState::Registering | TournamentState::Finished => None,
        }
    }

    /// The tournament as the wire reports it: the entrants by their
    /// commitments, never their lists.
    fn info(&self, id: &str) -> TournamentInfo {
        TournamentInfo {
            id: id.to_string(),
            name: self.name.clone(),
            format: self.format,
            organizer: self.organizer,
            state: self.state,
            seeding: self.seeding.clone(),
            rounds: self.rounds.clone(),
            dropped: self.dropped.clone(),
            draw_offers: self.draw_offers.clone(),
            clock: self.clock,
            seeds: self
                .seeds
                .iter()
                .map(|seed| {
                    let revealed = self.rounds.get(seed.round).and_then(|round| round.tables.get(seed.table)).is_some_and(|table| table.result.is_some());
                    seed.published(revealed)
                })
                .collect(),
            entrants: self
                .entrants
                .iter()
                .map(|entry| {
                    let said: RegistrationStatement = serde_json::from_str(&entry.commitment.payload).expect("the server checked this statement when it took it");
                    Entrant { name: entry.name.clone(), key: entry.key, corp_hash: said.corp_hash, runner_hash: said.runner_hash, commitment: entry.commitment.clone() }
                })
                .collect(),
        }
    }
}

/// Every tournament, by id.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Tournaments(std::collections::BTreeMap<String, Tournament>);

/// One table of one round of one tournament: what a seat waits at and a
/// match is played for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TableRef {
    tournament: String,
    /// Into `Tournament::rounds`.
    round: usize,
    /// Into the round's tables.
    table: usize,
}

/// What a tournament table's game brings to `start_match` beyond any
/// game's: the table, when its round's time is called, and the seed's
/// terms (Phase 4 §7 stage 6c) — the commitment, the server's secret and
/// each seat's nonce and registration salt (Corp, Runner). The salt is
/// the one the seat's registration was hashed under, so the deck hash
/// its seat statement names is the hash it signed at registration.
struct TableGame {
    table: TableRef,
    round_end: Option<tokio::time::Instant>,
    secret: String,
    nonces: [String; 2],
    salts: [String; 2],
}

/// Where a table stands between the pairing and its result: one player
/// seated and waiting for the other, or the game under way.
enum TableState {
    Waiting(Seated),
    Playing(Uuid),
}

/// A player who sat at their table first (`ClientMessage::Sit`), held
/// until the opponent sits: the name `MatchList` will show, their key,
/// the token `Queued` carried, the lobby they go back to, the channels
/// the match will play them through, and their half of the seed. Their
/// deck is not here: it is the one they registered, read when the game
/// starts.
struct Seated {
    token: Uuid,
    key: PublicKey,
    nonce: String,
    player_name: String,
    lobby: String,
    tx: mpsc::UnboundedSender<ServerMessage>,
    slot: PlayerSlot,
}

/// An attached connection, as the daemon can reach it unasked: the key it
/// proved and its outgoing channel. What a tournament's change is pushed
/// through to the entrants and the organizer who are on.
struct AttachedLink {
    key: Option<PublicKey>,
    tx: mpsc::UnboundedSender<ServerMessage>,
}

impl Tournaments {
    /// Oldest first.
    fn list(&self) -> Vec<TournamentInfo> {
        let mut all: Vec<(&String, &Tournament)> = self.0.iter().collect();
        all.sort_by_key(|(id, tournament)| (tournament.created_at, (*id).clone()));
        all.into_iter().map(|(id, tournament)| tournament.info(id)).collect()
    }
}

/// A player about to be seated: the name `MatchList` will show, the key
/// they proved (`None` for one who did not and for a bot, which is what
/// leaves their game unrated), the token `MatchJoined` will carry (already
/// issued if they came through the lobby, so one token spans queue and
/// match), and the slot the session plays them through. A bot seat
/// carries a token too, unused — cheaper than a second type for the one
/// case that never resumes.
struct SeatedPlayer {
    name: String,
    /// The key the player proved; `None` for one who did not, and for a
    /// bot.
    key: Option<PublicKey>,
    token: Uuid,
    slot: PlayerSlot,
    /// The deck the player brought, which replaces whatever the daemon
    /// would have dealt their side. `None` for a bot, which is dealt.
    deck: Option<Box<DeckFile>>,
    /// The lobby a player returns to after the game; `None` for a bot.
    lobby: Option<String>,
    /// A rung of the ladder in this seat, as the match record names it:
    /// for the reader of a record, never for its replay.
    bot: Option<RecordedBot>,
}

impl SeatedPlayer {
    fn channel_tx(&self) -> Option<&mpsc::UnboundedSender<ServerMessage>> {
        match &self.slot {
            PlayerSlot::Channel { tx, .. } => Some(tx),
            PlayerSlot::Bot(_) => None,
        }
    }
}

/// One attached connection in a lobby: its link and its name.
struct Member {
    link: Uuid,
    name: String,
}

/// The daemon's whole bookkeeping — see the module doc for why it is one
/// struct under one lock.
#[derive(Default)]
struct Registry {
    matches: HashMap<Uuid, MatchEntry>,
    seats: HashMap<Uuid, SeatTicket>,
    /// Every rating the daemon holds, loaded from the store at bind and
    /// written back to it row by row (`store`); see
    /// `ServeOptions::data_dir`.
    ratings: RatingBook,
    /// Everyone looking for a person to play, in every lobby, in arrival
    /// order: a lobby's queue is its waiters in this order
    /// (`enqueue_or_pair`).
    waiting: Vec<PendingHuman>,
    /// Which key was the Corp the last time two keys met, by the pair in
    /// sorted order (`ordered_pair`): what `queue::seat` swaps. Read from
    /// the store at bind and kept on every game between two keys, rated
    /// or not, so a daemon that keeps nothing still balances a rematch for
    /// as long as it runs.
    last_corp: HashMap<(String, String), String>,
    /// The rungs a casual lobby in each format offers: the daemon's
    /// `bots`, or none in a format it can deal a bot no deck in.
    bots_by_format: HashMap<NsgFormat, Vec<Level>>,
    /// The lobbies players made, by id.
    player_lobbies: HashMap<String, PlayerLobby>,
    /// The attached connections in each lobby, by id, in the order they
    /// came: each by its link and the name it attached with, which is
    /// what a hosted game's room shows its opponent by (Phase 7 §15).
    members: HashMap<String, Vec<Member>>,
    /// The tournaments the daemon holds; see `ServeOptions::data_dir`.
    tournaments: Tournaments,
    /// Every attached connection, by a link id of its own, for what is
    /// pushed unasked (`Shared::announce`).
    attached: HashMap<Uuid, AttachedLink>,
    /// A tournament table's seat waiting for its opponent, or its game.
    tables: HashMap<TableRef, TableState>,
    /// Claimed by `allocate`, never reused: match `n` plays on
    /// `base_seed + n` whether or not match `n - 1` finished, so a
    /// `--seed` run is reproducible connection for connection.
    next_match_index: u64,
}

impl Registry {
    /// Drops every waiter whose socket has gone. `bridge_websocket` tears
    /// both its halves down together, so a closed socket shows up here as
    /// a closed `tx` without any watcher task.
    fn sweep_lobby(&mut self) {
        self.waiting.retain(|waiter| !waiter.tx.is_closed());
    }

    fn at_cap(&self, options: &ServeOptions) -> bool {
        options.max_matches.is_some_and(|cap| self.matches.len() >= cap)
    }

    /// Claims the next match id and seed. The caller has checked `at_cap`
    /// under the same lock, which is what makes the cap exact.
    fn allocate(&mut self, base_seed: u64) -> (Uuid, u64) {
        let seed = base_seed.wrapping_add(self.next_match_index);
        self.next_match_index += 1;
        (Uuid::new_v4(), seed)
    }

    fn match_list(&self, options: &ServeOptions) -> ServerMessage {
        let now = Instant::now();
        let mut entries: Vec<(&Uuid, &MatchEntry)> = self.matches.iter().collect();
        entries.sort_by_key(|(_, entry)| entry.started_at);
        ServerMessage::MatchList {
            matches: entries
                .into_iter()
                .map(|(match_id, entry)| MatchSummary {
                    match_id: *match_id,
                    corp: entry.corp.clone(),
                    runner: entry.runner.clone(),
                    started_secs_ago: now.saturating_duration_since(entry.started_at).as_secs(),
                    format: entry.format,
                })
                .collect(),
            waiting: self.waiting.len(),
            max_matches: options.max_matches,
        }
    }

    /// Whether a friend lobby in `format` may offer a bot at all: the
    /// server offers some and can deal one a deck there.
    pub(crate) fn bots_in(&self, format: NsgFormat) -> bool {
        self.bots_by_format.get(&format).is_some_and(|rungs| !rungs.is_empty())
    }

    /// Waiters looking for a game in lobby `id`.
    fn seeking(&self, id: &str) -> usize {
        self.waiting.iter().filter(|waiter| waiter.lobby == id).count()
    }

    /// Lobby `id` as a player sees it, or `None` if there is no such lobby.
    fn lobby_info(&self, id: &str, options: &ServeOptions) -> Option<LobbyInfo> {
        let offered = |format: NsgFormat| self.bots_by_format.get(&format).cloned().unwrap_or_default();
        let (name, format, mode, password, bots) = match self.player_lobbies.get(id) {
            Some(lobby) => (lobby.name.clone(), lobby.format, QueueMode::Friends, lobby.password.is_some(), if lobby.bots { offered(lobby.format) } else { Vec::new() }),
            // A bot sits only in a friend lobby that allows one (decided 12
            // October 2026, with the dashboard): the server's own queues,
            // Ranked and Casual alike, are between people.
            None => {
                let (mode, format) = server_queues(options).find(|&(mode, format)| queue_id(mode, format) == id)?;
                (format!("{format:?}"), format, mode, false, Vec::new())
            }
        };
        Some(LobbyInfo {
            id: id.to_string(),
            name,
            format,
            mode,
            password,
            rated: mode == QueueMode::Ranked,
            bots,
            people: self.members.get(id).map(|members| members.iter().map(|member| self.presence(member, id)).collect()).unwrap_or_default(),
        })
    }

    /// A member as the lobby lists them: looking, and for which side,
    /// while their seek waits here.
    fn presence(&self, member: &Member, lobby: &str) -> Presence {
        let waiting = self.waiting.iter().find(|waiter| waiter.link == member.link && waiter.lobby == lobby);
        Presence { name: member.name.clone(), seeking: waiting.is_some(), prefers: waiting.and_then(|waiter| waiter.prefer) }
    }

    /// The lobbies listed: the server's own queues. A friend queue is
    /// never listed; it is joined by its code.
    fn open_lobbies(&self, options: &ServeOptions) -> Vec<LobbyInfo> {
        server_queues(options).filter_map(|(mode, format)| self.lobby_info(&queue_id(mode, format), options)).collect()
    }

    /// A player's lobby goes when nobody is in it and nobody waits in it.
    /// The server's own never go.
    fn forget_if_empty(&mut self, id: &str) {
        if self.player_lobbies.contains_key(id) && self.members.get(id).is_none_or(Vec::is_empty) && self.seeking(id) == 0 {
            self.player_lobbies.remove(id);
            self.members.remove(id);
        }
    }
}

/// Everything one accepted connection needs, cloned per connection.
#[derive(Clone)]
struct Shared {
    cards: CardRegistry,
    registry: Arc<StdMutex<Registry>>,
    options: ServeOptions,
    base_seed: u64,
    /// `--corp-deck`/`--runner-deck`, resolved and validated once at
    /// `bind` rather than per match: a misspelled id is a daemon that
    /// refuses to start, not one that refuses every client.
    pinned: PinnedDecks,
    /// Per served format, the matchups a seat nobody brought a deck to may
    /// be dealt (`legal_matchups`).
    deals: HashMap<NsgFormat, Vec<usize>>,
    /// The daemon's own key, which every `Challenge` names.
    identity: Arc<Identity>,
    /// Whether that key was read from the data directory and will be the
    /// same next run, rather than made at bind.
    lasting: bool,
    /// A hash of every card definition the daemon plays with, which a
    /// receipt names: a record replays only against the pool it was
    /// played with.
    card_pool: Arc<str>,
    /// The store behind its one writer; `None` for a daemon that keeps
    /// nothing (no data directory).
    store: Option<Writer>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Registry> {
        self.registry.lock().expect("daemon registry poisoned")
    }

    /// The decklists match `seed` is played with: whatever was pinned,
    /// and `fixtures::sample_decks_for_seed` for each side that was not.
    ///
    /// Rotating is the default because the daemon is the workspace's one
    /// remaining source of *rated* games, and a rating means less if the
    /// games behind it all came from one pairing. `--seed` still makes the
    /// sequence reproducible — it now fixes which matchups are dealt as
    /// well as how each shuffles.
    ///
    /// The rotation runs over the matchups legal in the match's format
    /// (`Shared::deals`); a format with none deals from the whole pool,
    /// which only a match whose seats all brought decks reaches, because
    /// a lobby in such a format offers no bot (`Registry::bots_by_format`).
    fn decks_for(&self, seed: u64, format: NsgFormat) -> DealtMatchup {
        let mut dealt = match self.deals.get(&format).filter(|legal| !legal.is_empty()) {
            Some(legal) => fixtures::sample_decks_at(legal[(seed % legal.len() as u64) as usize]),
            None => fixtures::sample_decks_for_seed(seed),
        };
        if let Some((id, deck)) = self.pinned.corp.clone() {
            dealt.corp_id = id;
            dealt.corp = deck;
        }
        if let Some((id, deck)) = self.pinned.runner.clone() {
            dealt.runner_id = id;
            dealt.runner = deck;
        }
        dealt
    }

    /// Sweeps first so `waiting_in_lobby` counts players who can actually
    /// be paired — a test (and a client) polls this to learn a dropped
    /// waiter is gone, so it must not report ghosts.
    fn match_list(&self) -> ServerMessage {
        let mut registry = self.lock();
        registry.sweep_lobby();
        registry.match_list(&self.options)
    }

    /// Rates one finished match on its format's ladder and queues the
    /// rows for the store. Under the registry lock, so two matches ending
    /// at once serialize their updates, and the rows are queued in that
    /// order (`store::Writer`); the lock is never held across an await.
    /// A failed row write is logged, not fatal: the ratings are already
    /// applied in memory and `rebuild` puts the store right from the log.
    ///
    /// **The receipt is appended to `results.jsonl` under the same lock**,
    /// so the log's order is the order the book was folded in, and
    /// `rebuild` reproduces it exactly. Returns each side's rating before
    /// and after; `None` when the daemon keeps nothing.
    fn rate(&self, corp: &str, runner: &str, format: NsgFormat, outcome: Outcome, receipt: &Signed, ended_at: u64) -> Option<[(Rating, Rating); 2]> {
        let dir = self.options.data_dir.as_ref()?;
        let track = store::ladder(format);
        let mut registry = self.lock();
        let corp_before = registry.ratings.standing(track, corp).unwrap_or_default().corp.rating;
        let runner_before = registry.ratings.standing(track, runner).unwrap_or_default().runner.rating;
        let (corp_after, runner_after) = registry.ratings.record(track, corp, runner, outcome);
        tracing::info!(
            corp, runner, ?format, ?outcome,
            corp_rating = corp_after.corp.rating.rating, runner_rating = runner_after.runner.rating.rating,
            "match rated"
        );
        let results = dir.join(RESULTS_FILE);
        let line = serde_json::to_string(receipt).expect("a signed receipt serializes");
        let appended = std::fs::OpenOptions::new().create(true).append(true).open(&results).and_then(|mut file| {
            use std::io::Write;
            writeln!(file, "{line}")
        });
        if let Err(error) = appended {
            tracing::warn!(path = %results.display(), ?error, "could not append to the results log");
        }
        // The rows that record what the book just took, in the order it
        // took them (`store::Writer`): both keys' standings whole, and the
        // game in the results index with its receipt.
        if let Some(writer) = &self.store {
            let mut rows = store::rows_of(track, corp, &corp_after);
            rows.extend(store::rows_of(track, runner, &runner_after));
            let result = ResultRow {
                match_id: receipt_match_id(receipt),
                ended_at: ended_at as i64,
                format: format_key(format),
                corp: corp.to_string(),
                runner: runner.to_string(),
                winner: Some(match outcome {
                    Outcome::CorpWin => "corp",
                    Outcome::RunnerWin => "runner",
                    Outcome::Draw => "none",
                }
                .to_string()),
                receipt: line,
            };
            writer.write("a rated game", move |store| async move { store.record_game(&rows, &result).await });
        }
        Some([(corp_before, corp_after.corp.rating), (runner_before, runner_after.runner.rating)])
    }

    /// `key`'s standing on every ladder it has a rated game on: one entry
    /// per format, in the formats' order, off the book in memory.
    fn standings_of(&self, id: &str) -> Vec<FormatStanding> {
        let registry = self.lock();
        NsgFormat::ALL
            .into_iter()
            .filter_map(|format| registry.ratings.standing(store::ladder(format), id).map(|standing| FormatStanding { format, standing }))
            .collect()
    }

    /// `key`'s profile, as `Profile` says it: the player row and the
    /// awards from the store, the standings from the book. `profile:
    /// None` for a daemon that keeps nothing and for a key the store has
    /// never seen attach — which a proved key asking has, since
    /// `saw_player` was queued at its attach: the read waits for the
    /// queue to settle (`Writer::settled`), because a read goes to the
    /// store directly and would otherwise race the write.
    async fn profile_of(&self, key: Option<PublicKey>) -> ServerMessage {
        let (Some(key), Some(writer)) = (key, &self.store) else { return ServerMessage::Profile { key, profile: None } };
        let id = key.rating_id();
        writer.settled().await;
        let profile = match self.read_profile(writer.store(), &id).await {
            Ok(profile) => profile,
            Err(error) => {
                tracing::warn!(key = %key.fingerprint(), %error, "could not read a profile");
                None
            }
        };
        ServerMessage::Profile { key: Some(key), profile }
    }

    async fn read_profile(&self, store: &Store, id: &str) -> netrunner_store::Result<Option<Profile>> {
        let Some(player) = store.player(id).await? else { return Ok(None) };
        let awards = store.awards_of(id).await?;
        let award = |row: &netrunner_store::Award| Award { name: row.name.clone(), earned_at: row.earned_at as u64, source: row.source.clone() };
        Ok(Some(Profile {
            name: player.name,
            first_seen: player.first_seen as u64,
            last_seen: player.last_seen as u64,
            standings: self.standings_of(id),
            titles: awards.iter().filter(|row| row.kind == "title").map(award).collect(),
            badges: awards.iter().filter(|row| row.kind == "badge").map(award).collect(),
            title: player.title,
            badge: player.badge,
        }))
    }

    /// Sets what `key` wears, each among what it has earned, and answers
    /// with the profile as it now stands. Written directly rather than
    /// through the writer, so the answer reads back what was written:
    /// `saw_player`'s upsert never touches these two columns, so there is
    /// no order with it to keep.
    async fn set_worn(&self, key: Option<PublicKey>, title: Option<String>, badge: Option<String>) -> ServerMessage {
        let refused = |reason: &str| ServerMessage::ProfileRefused { reason: reason.to_string() };
        let Some(key) = key else { return refused("a profile is a key's: prove one before attaching") };
        let Some(writer) = &self.store else { return refused("this server keeps no profile") };
        let id = key.rating_id();
        writer.settled().await;
        let store = writer.store();
        let written: netrunner_store::Result<Result<(), String>> = async {
            let awards = store.awards_of(&id).await?;
            let earned = |kind: &str, name: &Option<String>| name.as_ref().is_none_or(|name| awards.iter().any(|row| row.kind == kind && row.name == *name));
            if !earned("title", &title) {
                return Ok(Err(format!("no title {:?} has been earned here", title.unwrap_or_default())));
            }
            if !earned("badge", &badge) {
                return Ok(Err(format!("no badge {:?} has been earned here", badge.unwrap_or_default())));
            }
            if store.player(&id).await?.is_none() {
                return Ok(Err("this key has not attached here yet".to_string()));
            }
            store.set_worn(&id, title.as_deref(), badge.as_deref()).await?;
            Ok(Ok(()))
        }
        .await;
        match written {
            Ok(Ok(())) => self.profile_of(Some(key)).await,
            Ok(Err(reason)) => refused(&reason),
            Err(error) => {
                tracing::warn!(key = %key.fingerprint(), %error, "could not write a profile");
                refused("the server could not write the profile")
            }
        }
    }

    /// The ladder in `format`: every key with `LADDER_MIN_GAMES` rated
    /// games there, best overall first (`RatingBook::ladder`'s order), at
    /// most `LADDER_LIMIT`, each with the name and title the store holds.
    async fn ladder_of(&self, format: NsgFormat) -> ServerMessage {
        let Some(writer) = &self.store else { return ServerMessage::Ladder { format, rows: Vec::new() } };
        let listed: Vec<(String, netrunner_rating::Standing)> = {
            let registry = self.lock();
            registry
                .ratings
                .ladder(store::ladder(format))
                .into_iter()
                .filter(|(_, standing)| standing.corp.games() + standing.runner.games() >= LADDER_MIN_GAMES)
                .take(LADDER_LIMIT)
                .map(|(id, standing)| (id.to_string(), standing))
                .collect()
        };
        let mut rows = Vec::with_capacity(listed.len());
        for (id, standing) in listed {
            let Ok(key) = id.parse::<PublicKey>() else { continue };
            let (name, title) = match writer.store().player(&id).await {
                Ok(Some(player)) => (player.name, player.title),
                _ => (String::new(), None),
            };
            rows.push(LadderRow { key, name, standing, title });
        }
        ServerMessage::Ladder { format, rows }
    }

    /// Remembers which chairs two keys took against each other, so the
    /// next pairing of the two can swap them (Phase 4 §9); every game
    /// between two keys, rated or not, as it starts. Under the caller's
    /// registry lock, the one the pairing was made under.
    fn remember_pairing(&self, registry: &mut Registry, corp: &str, runner: &str, played_at: u64) {
        registry.last_corp.insert(ordered_pair(corp, runner), corp.to_string());
        let Some(writer) = &self.store else { return };
        let (corp, runner) = (corp.to_string(), runner.to_string());
        writer.write("a pairing", move |store| async move { store.put_pairing(&corp, &runner, played_at as i64).await });
    }

    /// A seat's signature over the statement it was sent, kept if it is
    /// that seat's key's over that statement. Anything else — a seat with
    /// no statement, a second signature, a signature that does not hold —
    /// is ignored: the game goes on, and its receipt lacks the
    /// commitment.
    fn accept_commitment(&self, token: Uuid, signature: Signature) {
        let mut registry = self.lock();
        let Some(ticket) = registry.seats.get(&token) else { return };
        let (match_id, side) = (ticket.match_id, ticket.side);
        let Some(commitment) = registry.matches.get_mut(&match_id).and_then(|entry| entry.commitments[seat_index(side)].as_mut()) else { return };
        if commitment.signed.is_some() {
            return;
        }
        match commitment.key.verify(SEAT_TAG, commitment.payload.as_bytes(), &signature) {
            Ok(()) => commitment.signed = Some(Signed { key: commitment.key, payload: commitment.payload.clone(), signature }),
            Err(_) => tracing::info!(%match_id, ?side, "a seat commitment did not verify"),
        }
    }

    /// Writes a finished match's record — its header and every action,
    /// the JSON-Lines `netrunner_cli replay` reads — to
    /// `matches/<yyyy-mm>/<match id>.jsonl` (Phase 4 §5 stage a). Every
    /// match, a bot's and a stall included: a record is evidence for a
    /// dispute or a bug report as much as for a rating.
    ///
    /// **Written when the match ends, not appended as it goes.** A crash
    /// mid-match loses that match's record, and the players' seats with
    /// it; appending would buy the prefix of a game nobody finished, at a
    /// write per action on the hottest path the daemon has. A failed write
    /// is logged, as a failed rating write is.
    ///
    /// **The receipt sits beside it** (`<match id>.receipt.json`), not in
    /// a footer line as the design first sketched: the record stays
    /// exactly what `MatchHistory::read_jsonl` and replay read, and the
    /// hash the receipt signs is of exactly that file.
    fn keep_record(&self, match_id: Uuid, record: &[u8], receipt: &Signed, ended_at: u64) {
        let Some(dir) = &self.options.data_dir else { return };
        let path = record_path(dir, match_id, ended_at);
        let written = (|| {
            std::fs::create_dir_all(path.parent().expect("a record sits in a month's directory"))?;
            let tmp = path.with_extension("jsonl.tmp");
            std::fs::write(&tmp, record)?;
            std::fs::rename(tmp, &path)?;
            let receipt_json = serde_json::to_string_pretty(receipt).expect("a signed receipt serializes");
            write_atomically(&path.with_extension("receipt.json"), &receipt_json)
        })();
        match written {
            Ok(()) => tracing::info!(%match_id, path = %path.display(), "match record kept"),
            Err(error) => tracing::warn!(%match_id, path = %path.display(), ?error, "could not keep the match record"),
        }
    }

    /// A tournament as it now stands, pushed to every attached entrant and
    /// its organizer but the connection that asked (`except`, which gets
    /// the same message as its answer): a player waiting on the page sees
    /// the pairing when it is posted, and the organizer sees the entrants
    /// arrive and the tables finish. Under the caller's registry lock.
    fn announce(&self, registry: &Registry, id: &str, except: Option<Uuid>) {
        let Some(tournament) = registry.tournaments.0.get(id) else { return };
        let info = tournament.info(id);
        for (link_id, link) in &registry.attached {
            if Some(*link_id) == except {
                continue;
            }
            let Some(key) = link.key else { continue };
            if key == tournament.organizer || tournament.entry(&key).is_some() {
                let _ = link.tx.send(ServerMessage::Tournament { tournament: info.clone() });
            }
        }
    }

    /// A table's game ended: its result is written into the round, the
    /// table let go, the file rewritten and the change pushed. A result
    /// the organizer already recorded (a game that ran past a recorded
    /// no-show cannot happen, since recording refuses a playing table)
    /// is kept; a table whose tournament has gone is nobody's.
    fn table_ended(&self, table: TableRef, outcome: TableOutcome) {
        let mut registry = self.lock();
        registry.tables.remove(&table);
        let Some(tournament) = registry.tournaments.0.get_mut(&table.tournament) else { return };
        let Some(slot) = tournament.rounds.get_mut(table.round).and_then(|round| round.tables.get_mut(table.table)) else { return };
        if slot.result.is_none() {
            slot.result = Some(outcome);
        }
        tournament.draw_offers.retain(|offer| offer.table != table.table);
        self.save_tournament(&registry, &table.tournament);
        self.announce(&registry, &table.tournament, None);
    }

    /// Writes the tournament `id` to the store after a change, under the
    /// caller's lock — the document as it now stands, one row, so the
    /// store holds it in the order the registry changed it. A daemon
    /// with no data directory holds no tournament (`attached` refuses to
    /// make one), so there is nothing to write.
    fn save_tournament(&self, registry: &Registry, id: &str) {
        let (Some(writer), Some(tournament)) = (&self.store, registry.tournaments.0.get(id)) else { return };
        let row = TournamentRow { id: id.to_string(), document: serde_json::to_string(tournament).expect("a tournament serializes"), updated_at: unix_now() as i64 };
        writer.write("a tournament", move |store| async move { store.put_tournament(&row).await });
    }

    /// Notes that `key` has attached as `name`, in the store's players
    /// table: a first visit makes the row, a later one updates the name
    /// and the time (`Store::saw_player`).
    fn saw_player(&self, key: PublicKey, name: &str) {
        let Some(writer) = &self.store else { return };
        let (key, name) = (key.rating_id(), name.to_string());
        writer.write("a player seen", move |store| async move { store.saw_player(&key, &name, unix_now() as i64).await });
    }
}

/// The match id a signed receipt names, for the results index; the
/// receipt was made by this daemon a moment ago, so it parses.
fn receipt_match_id(receipt: &Signed) -> String {
    serde_json::from_str::<Receipt>(&receipt.payload).map(|receipt| receipt.match_id.to_string()).unwrap_or_default()
}

const IDENTITY_FILE: &str = "identity.key";
const MATCHES_DIR: &str = "matches";
const RESULTS_FILE: &str = "results.jsonl";

/// What a receipt names the server's build by.
const ENGINE: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));

/// SHA-256 over every card definition's JSON, in card order: a registry
/// is a `HashMap`, and its own serialization would hash differently from
/// one run to the next.
fn card_pool_hash(cards: &CardRegistry) -> String {
    let mut definitions: Vec<_> = cards.iter().collect();
    definitions.sort_by(|a, b| a.id.cmp(&b.id));
    let mut text = String::new();
    for definition in definitions {
        text.push_str(&serde_json::to_string(definition).expect("a card definition serializes"));
        text.push('\n');
    }
    netrunner_identity::sha256_hex(text.as_bytes())
}

/// Rebuilds the store's ratings and results from `results.jsonl` in
/// `data_dir`, folding every rated receipt through `RatingBook::record` in
/// the order it was appended — the store is an index of the log, and this
/// is how a corrupt store, a change to the rating system's parameters, or
/// a voided game (its line deleted) is put right. Every receipt must be
/// this daemon's key's; one that is not, or that does not verify, stops
/// the rebuild and names its line, rather than fold a result nobody
/// signed. The players, the awards, the pairings and the tournaments are
/// not the log's and are left as they are. Returns how many games were
/// folded. `database` is `ServeOptions::database`.
pub async fn rebuild(data_dir: &Path, database: Option<&str>) -> std::io::Result<usize> {
    let invalid = |line: usize, error: String| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{RESULTS_FILE} line {line}: {error}"));
    let identity_path = data_dir.join(IDENTITY_FILE);
    let identity = Identity::from_file_text(&std::fs::read_to_string(&identity_path)?)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{}: {error}", identity_path.display())))?;
    let server_key = identity.public_key();
    let results = data_dir.join(RESULTS_FILE);
    let text = if results.exists() { std::fs::read_to_string(&results)? } else { String::new() };
    let mut book = RatingBook::default();
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate().filter(|(_, line)| !line.trim().is_empty()) {
        let signed: Signed = serde_json::from_str(line).map_err(|error| invalid(index + 1, error.to_string()))?;
        let receipt = Receipt::read(&signed, &server_key).map_err(|error| invalid(index + 1, error))?;
        let (Some(corp), Some(runner), Some(winner)) = (receipt.corp.key, receipt.runner.key, receipt.winner) else { continue };
        if !receipt.rated {
            continue;
        }
        let outcome = match winner {
            Side::Corp => Outcome::CorpWin,
            Side::Runner => Outcome::RunnerWin,
        };
        book.record(store::ladder(receipt.format), &corp.rating_id(), &runner.rating_id(), outcome);
        rows.push(ResultRow {
            match_id: receipt.match_id.to_string(),
            ended_at: receipt.ended_at as i64,
            format: format_key(receipt.format),
            corp: corp.rating_id(),
            runner: runner.rating_id(),
            winner: Some(match winner {
                Side::Corp => "corp",
                Side::Runner => "runner",
            }
            .to_string()),
            receipt: line.to_string(),
        });
    }
    let store = open_store(data_dir, database).await?;
    let ratings: Vec<netrunner_store::RatingRow> = book.entries().flat_map(|(track, key, standing)| store::rows_of(track, key, standing)).collect();
    let written = store.replace_rebuilt(&ratings, &rows).await;
    store.close().await;
    written.map_err(|error| std::io::Error::other(format!("rebuilding the store: {error}")))?;
    Ok(rows.len())
}

/// Opens the daemon's store: the database `database` names, or the
/// SQLite file in `data_dir` (`netrunner_store::SQLITE_FILE`), made on
/// first start.
async fn open_store(data_dir: &Path, database: Option<&str>) -> std::io::Result<Store> {
    let url = match database {
        Some(url) => url.to_string(),
        None => netrunner_store::sqlite_url(&data_dir.join(netrunner_store::SQLITE_FILE)),
    };
    Store::open(&url).await.map_err(|error| std::io::Error::other(format!("opening the store at {url}: {error}")))
}

/// Where a match's record lives: a directory per month, so a daemon that
/// runs for years never holds one directory of every match it played.
pub fn record_path(data_dir: &Path, match_id: Uuid, ended_at: u64) -> PathBuf {
    let (year, month) = year_month(ended_at);
    data_dir.join(MATCHES_DIR).join(format!("{year:04}-{month:02}")).join(format!("{match_id}.jsonl"))
}

/// The UTC year and month of a Unix time — Howard Hinnant's
/// `civil_from_days`, because a month's directory name is the only date
/// the daemon needs and a calendar crate for it is not worth a dependency.
fn year_month(unix_secs: u64) -> (i64, u32) {
    let days = (unix_secs / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// What a daemon reads from its data directory at bind, making the
/// directory, the key and the store on first start.
struct Kept {
    identity: Identity,
    lasting: bool,
    ratings: RatingBook,
    tournaments: Tournaments,
    last_corp: HashMap<(String, String), String>,
    store: Option<Store>,
}

impl Kept {
    /// A daemon that keeps nothing: a key made at bind, no book, no store.
    fn nothing() -> Kept {
        Kept { identity: Identity::from_secret(rand::random()), lasting: false, ratings: RatingBook::default(), tournaments: Tournaments::default(), last_corp: HashMap::new(), store: None }
    }
}

async fn load_kept(data_dir: Option<&Path>, database: Option<&str>) -> std::io::Result<Kept> {
    let Some(dir) = data_dir else { return Ok(Kept::nothing()) };
    std::fs::create_dir_all(dir)?;
    let identity = load_or_make_identity(&dir.join(IDENTITY_FILE))?;
    let store = open_store(dir, database).await?;
    let invalid = |what: &str, error: String| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("reading the store's {what}: {error}"));
    let ratings = store::book_from(&store.ratings().await.map_err(|error| invalid("ratings", error.to_string()))?);
    let mut tournaments = Tournaments::default();
    for row in store.tournaments().await.map_err(|error| invalid("tournaments", error.to_string()))? {
        let tournament: Tournament = serde_json::from_str(&row.document).map_err(|error| invalid("tournaments", format!("{}: {error}", row.id)))?;
        tournaments.0.insert(row.id, tournament);
    }
    let last_corp = store
        .pairings()
        .await
        .map_err(|error| invalid("pairings", error.to_string()))?
        .into_iter()
        .map(|pairing| (ordered_pair(&pairing.corp, &pairing.runner), pairing.corp))
        .collect();
    Ok(Kept { identity, lasting: true, ratings, tournaments, last_corp, store: Some(store) })
}

/// The daemon's key: read, or made and written with only its owner able
/// to read it. A file that is there and unreadable is an error, never a
/// reason to make a new key — that would silently make every client that
/// remembered the old one cry wolf.
fn load_or_make_identity(path: &Path) -> std::io::Result<Identity> {
    if path.exists() {
        let text = std::fs::read_to_string(path)?;
        return Identity::from_file_text(&text).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{}: {error}", path.display())));
    }
    let identity = Identity::from_secret(rand::random());
    write_secret(path, &identity.to_file_text())?;
    tracing::info!(path = %path.display(), key = %identity.public_key(), "made the server's key");
    Ok(identity)
}

/// Written readable by its owner alone, created that way rather than
/// narrowed after, so the secret is never on disk with wider access.
fn write_secret(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(text.as_bytes())
}

fn write_atomically(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

pub struct Server {
    listener: TcpListener,
    shared: Shared,
}

impl Server {
    /// Binds `addr` (`host:port`; port 0 for an ephemeral one — see
    /// `local_addr`). Accepting starts in `run`.
    pub async fn bind(addr: &str, options: ServeOptions) -> std::io::Result<Self> {
        Self::check_options(&options)?;
        let listener = TcpListener::bind(addr).await?;
        let kept = load_kept(options.data_dir.as_deref(), options.database.as_deref()).await?;
        Self::with_listener(listener, options, kept)
    }

    /// Serves on a listener somebody else bound. It exists for a host that
    /// needs a socket option `bind` cannot say: `netrunner_client::hosting`
    /// binds `[::]` with `IPV6_V6ONLY` off, so one listener takes IPv4 and
    /// IPv6 on every platform (Windows defaults the option on). The option
    /// is set by the caller rather than here so this crate takes no socket
    /// dependency of its own. Must be called inside a tokio runtime.
    ///
    /// **A host keeps nothing**, so this refuses a data directory: opening
    /// a store is an await, and both clients call this from a function
    /// that is not one — and a game hosted from a player's own machine is
    /// never rated (`docs/identity-and-rating.md` §5), so there is
    /// nothing a store would hold for it.
    pub fn from_listener(listener: std::net::TcpListener, options: ServeOptions) -> std::io::Result<Self> {
        Self::check_options(&options)?;
        if options.data_dir.is_some() {
            return Err(std::io::Error::other("a server on a listener somebody else bound keeps no data directory; use `bind`"));
        }
        listener.set_nonblocking(true)?;
        Self::with_listener(TcpListener::from_std(listener)?, options, Kept::nothing())
    }

    fn check_options(options: &ServeOptions) -> std::io::Result<()> {
        if options.formats.is_empty() {
            return Err(std::io::Error::other("a daemon needs at least one format to pair players in"));
        }
        Ok(())
    }

    fn with_listener(listener: TcpListener, options: ServeOptions, kept: Kept) -> std::io::Result<Self> {
        let base_seed = options.seed.unwrap_or_else(rand::random);
        let Kept { identity, lasting, ratings, tournaments, last_corp, store } = kept;
        let cards = fixtures::sample_registry();
        // The rotating pool is gated per format, and for a better reason
        // than a pinned deck: an operator who pins a deck names it and
        // would see it refused, while a rotating daemon deals whatever the
        // seed picks and would only find out mid-match. Worked out once
        // here rather than per match — the pool is embedded and cannot
        // change while the process runs.
        let deals = deals_by_format(&cards, &options)?;
        // A pinned deck is dealt only where a bot is seated, which is only
        // a format the rotating pool can deal in (`bots_by_format`), so it
        // is held to those formats and not to one served for people alone
        // — Snapshot, which no shipped deck is legal in, would otherwise
        // refuse every pin on the default daemon.
        let mut pinned = PinnedDecks::default();
        for (&format, legal) in &deals {
            if legal.is_empty() {
                continue;
            }
            pinned = PinnedDecks {
                corp: pin_deck(options.corp_deck.as_deref(), Side::Corp, &cards, format)?,
                runner: pin_deck(options.runner_deck.as_deref(), Side::Runner, &cards, format)?,
            };
        }
        let bots_by_format = deals.iter().map(|(&format, legal)| (format, if legal.is_empty() { Vec::new() } else { options.bots.clone() })).collect();
        let cards_for_hash = cards.clone();
        let shared = Shared {
            cards,
            registry: Arc::new(StdMutex::new(Registry { ratings, tournaments, last_corp, bots_by_format, ..Registry::default() })),
            options,
            base_seed,
            pinned,
            deals,
            card_pool: card_pool_hash(&cards_for_hash).into(),
            identity: Arc::new(identity),
            lasting,
            store: store.map(Writer::start),
        };
        Ok(Server { listener, shared })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// The key this server proves itself by: the one every `Challenge`
    /// names.
    pub fn public_key(&self) -> PublicKey {
        self.shared.identity.public_key()
    }

    /// A door into this server for connections it did not accept itself.
    /// Take one before `run`, which consumes the server.
    pub fn acceptor(&self) -> Acceptor {
        Acceptor { shared: self.shared.clone() }
    }

    /// The accept loop. Returns only if `accept` itself fails.
    pub async fn run(self) -> std::io::Result<()> {
        tracing::info!(addr = %self.local_addr()?, bots = ?self.shared.options.bots, "netrunner_server listening");
        loop {
            let (stream, peer_addr) = self.listener.accept().await?;
            let shared = self.shared.clone();
            tokio::spawn(async move {
                if let Err(error) = handle_connection(stream, shared).await {
                    tracing::warn!(%peer_addr, ?error, "connection ended with an error");
                }
            });
        }
    }
}

/// Serves a byte stream that arrived some other way than this server's
/// TCP listener, as if the listener had accepted it: the WebSocket
/// handshake, the lobby, a seat, a resume — one registry, so a player who
/// came in by TCP and one who came in another way are paired with each
/// other (Phase 4 §6 item 3).
///
/// **A stream, not a transport.** `netrunner_client::peer` accepts a QUIC
/// stream over iroh and hands it here; the WebSocket framing rides inside
/// it unchanged. Taking a stream keeps this crate free of the peer-to-peer
/// dependency, as `from_listener` keeps it free of `socket2`, and it means
/// the handshake has one implementation whatever carried it.
#[derive(Clone)]
pub struct Acceptor {
    shared: Shared,
}

impl Acceptor {
    /// Serves `stream` to the end of its handshake; the match it joins
    /// runs on in tasks of its own. `peer` names it in the log.
    pub async fn serve<S>(&self, stream: S, peer: &str)
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        if let Err(error) = handle_connection(stream, self.shared.clone()).await {
            tracing::warn!(%peer, ?error, "connection ended with an error");
        }
    }
}

/// The first message that commits a socket to something. `ListMatches` is
/// answered inline without leaving this loop, so a client can look before
/// it joins, and so is proving a key (`Identify`, `Prove`), which comes
/// before the commitment; anything else is skipped until one of these
/// arrives.
enum Handshake {
    Resume { session_token: Uuid },
    Spectate { match_id: Uuid },
    Attach { player_name: String },
}

async fn handle_connection<S>(stream: S, shared: Shared) -> Result<(), Box<dyn std::error::Error>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let mut ws_stream = tokio_tungstenite::accept_async(stream).await?;

    // The key this connection claimed and the nonce it was asked to sign,
    // between `Identify` and `Prove`; then the key it proved.
    let mut claimed: Option<(PublicKey, Nonce)> = None;
    let mut identified: Option<PublicKey> = None;
    let handshake = loop {
        match ws_stream.next().await {
            Some(Ok(WsMessage::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Resume { session_token }) => break Handshake::Resume { session_token },
                Ok(ClientMessage::Spectate { match_id }) => break Handshake::Spectate { match_id },
                Ok(ClientMessage::Attach { player_name }) => break Handshake::Attach { player_name },
                Ok(ClientMessage::ListMatches) => {
                    ws_stream.send(WsMessage::Text(serde_json::to_string(&shared.match_list())?)).await?;
                }
                Ok(ClientMessage::MyProfile) => {
                    ws_stream.send(WsMessage::Text(serde_json::to_string(&shared.profile_of(identified).await)?)).await?;
                }
                // A ladder is anyone's to read, as the match list is.
                Ok(ClientMessage::Ladder { format }) => {
                    ws_stream.send(WsMessage::Text(serde_json::to_string(&shared.ladder_of(format).await)?)).await?;
                }
                // A nonce fresh for this connection: a proof is good on
                // the socket it was made for and no other. A second
                // `Identify` starts again with a new nonce.
                Ok(ClientMessage::Identify { key }) => {
                    let nonce = Nonce(rand::random());
                    claimed = Some((key, nonce));
                    let challenge = ServerMessage::Challenge { nonce, server_key: shared.identity.public_key(), lasting: shared.lasting };
                    ws_stream.send(WsMessage::Text(serde_json::to_string(&challenge)?)).await?;
                }
                Ok(ClientMessage::Prove { signature }) => {
                    let proved = claimed.take().ok_or("a proof with no challenge").and_then(|(key, nonce)| {
                        key.verify_proof(&shared.identity.public_key(), &nonce, &signature).map(|()| key).map_err(|_| "that signature does not prove the key")
                    });
                    match proved {
                        Ok(key) => {
                            tracing::info!(key = %key.fingerprint(), "client proved its key");
                            identified = Some(key);
                            ws_stream.send(WsMessage::Text(serde_json::to_string(&ServerMessage::Identified { key })?)).await?;
                        }
                        // A connection that claims a key it cannot prove is
                        // not let on as anyone, and not as nobody either: a
                        // client that meant to be rated should learn now,
                        // not after a game that counted for nothing.
                        Err(reason) => {
                            tracing::info!(reason, "identification refused");
                            let refusal = ServerMessage::IdentifyRefused { reason: reason.to_string() };
                            let _ = ws_stream.send(WsMessage::Text(serde_json::to_string(&refusal)?)).await;
                            let _ = ws_stream.close(None).await;
                            return Ok(());
                        }
                    }
                }
                _ => continue,
            },
            Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => return Err("connection closed before handshake".into()),
            Some(Ok(_)) => continue,
        }
    };

    match handshake {
        Handshake::Resume { session_token } => {
            let ticket = shared.lock().seats.get(&session_token).cloned();
            if let Some(ticket) = ticket.filter(|ticket| ticket.handle.is_live()) {
                tracing::info!(%session_token, match_id = %ticket.match_id, side = ?ticket.side, "client resumed");
                let (session_tx, bridge_rx) = mpsc::unbounded_channel::<ServerMessage>();
                let (bridge_tx, session_rx) = mpsc::unbounded_channel::<ClientMessage>();
                tokio::spawn(net::bridge_websocket(ws_stream, bridge_tx, bridge_rx));
                // `MatchJoined` before the reattach, so it precedes the
                // `StateUpdate` the session answers with: the client is
                // waiting for its seat back before it renders anything.
                let _ = session_tx.send(ticket.joined(session_token));
                // The seat is attached again: the match talks to a
                // connection task, which takes the socket back to the
                // lobby when the game is over.
                let name = shared.lock().matches.get(&ticket.match_id).map(|entry| match ticket.side {
                    Side::Corp => entry.corp.clone(),
                    Side::Runner => entry.runner.clone(),
                });
                let (out_tx, out_rx) = mpsc::unbounded_channel::<ServerMessage>();
                let (into_tx, into_rx) = mpsc::unbounded_channel::<ClientMessage>();
                if let Some(held) = shared.lock().seats.get_mut(&session_token) {
                    held.tx = out_tx.clone();
                }
                if ticket.handle.reattach(ticket.side, out_tx, into_rx).is_err() {
                    // Lost the race with the match ending between the
                    // liveness check and here. The bridge is already up,
                    // so the refusal goes down the same channel.
                    let _ = session_tx.send(ServerMessage::ResumeRejected { reason: "the match ended".into() });
                    return Ok(());
                }
                let playing = attached::Playing { token: session_token, out: out_rx, into: into_tx, lobby: ticket.lobby.clone() };
                tokio::spawn(attached::run(shared, name.unwrap_or_default(), identified, session_tx, session_rx, Some(playing)));
                return Ok(());
            }

            // A seek is withdrawn with its socket, so a token is only ever
            // a seat's.
            tracing::info!(%session_token, "resume refused: no such seat");
            let refusal = ServerMessage::ResumeRejected { reason: "no live match holds that session token".into() };
            let _ = ws_stream.send(WsMessage::Text(serde_json::to_string(&refusal)?)).await;
            let _ = ws_stream.close(None).await;
        }
        Handshake::Attach { player_name } => {
            tracing::info!(%player_name, key = identified.map(|key| key.fingerprint()), "client attached");
            if let Some(key) = identified {
                shared.saw_player(key, &player_name);
            }
            let (session_tx, bridge_rx) = mpsc::unbounded_channel::<ServerMessage>();
            let (bridge_tx, session_rx) = mpsc::unbounded_channel::<ClientMessage>();
            tokio::spawn(net::bridge_websocket(ws_stream, bridge_tx, bridge_rx));
            tokio::spawn(attached::run(shared, player_name, identified, session_tx, session_rx, None));
        }
        Handshake::Spectate { match_id } => {
            let handle = shared.lock().matches.get(&match_id).map(|entry| entry.handle.clone());
            let Some(handle) = handle.filter(ReattachHandle::is_live) else {
                tracing::info!(%match_id, "spectate refused: no such match");
                let refusal = ServerMessage::ConnectRejected { reason: "no live match has that id".into() };
                let _ = ws_stream.send(WsMessage::Text(serde_json::to_string(&refusal)?)).await;
                let _ = ws_stream.close(None).await;
                return Ok(());
            };
            tracing::info!(%match_id, "spectator joined");
            let (session_tx, bridge_rx) = mpsc::unbounded_channel::<ServerMessage>();
            let (bridge_tx, mut session_rx) = mpsc::unbounded_channel::<ClientMessage>();
            tokio::spawn(net::bridge_websocket(ws_stream, bridge_tx, bridge_rx));
            // The session never reads a spectator's messages, but the
            // receiving half must stay open: the bridge's recv task ends
            // when its send into a dropped receiver fails, and the select
            // in `bridge_websocket` then closes the socket — so a dropped
            // `rx` would kick the spectator on its first keypress. Drain
            // and discard instead.
            tokio::spawn(async move { while session_rx.recv().await.is_some() {} });
            // The session says `Spectating` itself, ahead of the
            // `StateUpdate` it answers with — or refuses the sink at its
            // spectator cap, with `ConnectRejected` and nothing before it.
            if handle.add_spectator(session_tx.clone(), match_id).is_err() {
                let _ = session_tx.send(ServerMessage::ConnectRejected { reason: "the match ended".into() });
            }
        }
    }
    Ok(())
}

const AT_CAP: &str = "the host is at its match limit";

/// A seat the daemon cannot give: at its match cap, or a match that failed
/// to set up. The attached connection that asked turns it into
/// `SeekRefused` (`attached::Attached::on_match`).
fn refuse(tx: &mpsc::UnboundedSender<ServerMessage>, reason: &str) {
    let _ = tx.send(ServerMessage::ConnectRejected { reason: reason.to_string() });
}

/// A seek against a bot (`Opponent::Bot`), already checked against what
/// its lobby offers: seated at once, in the chair its deck is for,
/// against `level` in `style` — or the style of the deck the bot is
/// dealt. Never rated.
#[allow(clippy::too_many_arguments)]
fn seat_vs_bot(
    shared: &Shared,
    level: Level,
    style: Option<Style>,
    player_name: String,
    format: NsgFormat,
    deck: Box<DeckFile>,
    lobby: String,
    tx: mpsc::UnboundedSender<ServerMessage>,
    slot: PlayerSlot,
) {
    let mut registry = shared.lock();
    if registry.at_cap(&shared.options) {
        refuse(&tx, AT_CAP);
        return;
    }
    let (match_id, seed) = registry.allocate(shared.base_seed);

    let human_side = deck.side;
    // Nothing against a bot is rated, so the person's key is never asked
    // for; the receipt names the seat by its name alone.
    let human = SeatedPlayer { key: None, name: player_name, token: Uuid::new_v4(), slot, deck: Some(deck), lobby: Some(lobby), bot: None };
    // The same deal `start_match` will make — `decks_for` is a function of
    // the seed — so the bot's style can come off the deck it is about to
    // play. A pinned deck is an embedded id (`pin_deck`), so `by_id`
    // always resolves; the fallback is only for a future pool that is not.
    let dealt = shared.decks_for(seed, format);
    let (bot_deck_id, bot_deck) = match human_side {
        Side::Corp => (&dealt.runner_id, &dealt.runner),
        Side::Runner => (&dealt.corp_id, &dealt.corp),
    };
    let style = style.unwrap_or_else(|| decks::by_id(bot_deck_id).and_then(|deck| Style::for_deck(&deck).ok()).unwrap_or_default());
    // The bot knows the lobby's format and the deck it is dealt; the
    // person's deck it knows by its identity alone, like any opponent.
    let knowledge = Knowledge::new(format, Some(bot_deck.clone()));
    let bot_side = human_side.other();
    let bot = SeatedPlayer {
        name: styled(format!("{} bot", level.name()), style),
        key: None,
        token: Uuid::new_v4(),
        slot: PlayerSlot::Bot(level.spec(bot_side).with_style(style).agent(seed.wrapping_add(1), knowledge)),
        deck: None,
        lobby: None,
        bot: Some(RecordedBot::rung(bot_side, level, style)),
    };
    let (corp, runner) = match human_side {
        Side::Corp => (human, bot),
        Side::Runner => (bot, human),
    };
    start_match(shared, &mut registry, match_id, seed, format, corp, runner, false, None);
}

/// Pair `newcomer` with a waiter in its lobby, or put it in the lobby's
/// queue: whom, and who takes which chair, is `queue`'s to say.
fn enqueue_or_pair(shared: &Shared, newcomer: PendingHuman) {
    let mut registry = shared.lock();
    registry.sweep_lobby();
    if registry.at_cap(&shared.options) {
        refuse(&newcomer.tx, AT_CAP);
        return;
    }

    let queue: Vec<usize> = registry.waiting.iter().enumerate().filter(|(_, waiter)| waiter.lobby == newcomer.lobby).map(|(index, _)| index).collect();
    let seeks: Vec<queue::Seek> = queue.iter().map(|&index| registry.waiting[index].seek()).collect();
    let Some(found) = queue::partner(&seeks, &newcomer.seek(), unix_now()) else {
        let (token, tx) = (newcomer.token, newcomer.tx.clone());
        registry.waiting.push(newcomer);
        let _ = tx.send(ServerMessage::Queued { session_token: token, position: queue.len() + 1 });
        return;
    };
    let waiter = registry.waiting.remove(queue[found]);
    let format = waiter.format;
    // Read at pairing, off the lobby both are in.
    let rated_lobby = registry.lobby_info(&waiter.lobby, &shared.options).is_some_and(|lobby| lobby.rated);
    let (match_id, seed) = registry.allocate(shared.base_seed);
    let (first, second) = (waiter.seek(), newcomer.seek());
    let last = queue::last_corp(&first, &second, |a, b| registry.last_corp.get(&ordered_pair(a, b)).cloned());
    // The coin, when nothing else decides, is tossed off the match's seed
    // so a `--seed` run seats the same way every time.
    let (corp, runner) = match queue::seat(&first, &second, last, seed & 1 == 0) {
        queue::Corp::First => (waiter, newcomer),
        queue::Corp::Second => (newcomer, waiter),
    };
    start_match(shared, &mut registry, match_id, seed, format, corp.seated(Side::Corp), runner.seated(Side::Runner), rated_lobby, None);
}

/// A format as the results index names it: its name in lower case.
fn format_key(format: NsgFormat) -> String {
    format!("{format:?}").to_lowercase()
}

/// Two keys in the order the pairing memory files them under, whichever
/// order they are named in.
fn ordered_pair(a: &str, b: &str) -> (String, String) {
    if a <= b { (a.to_string(), b.to_string()) } else { (b.to_string(), a.to_string()) }
}

/// Sets up the state, builds the session, records the match and a ticket
/// per channel seat, tells each seat it has joined, and runs the match;
/// the match and its tickets are dropped when it returns. One place for
/// all of it so the bot and lobby paths cannot disagree on the order —
/// `MatchJoined` must precede the session's first `StateUpdate`, and a
/// ticket must exist before a client can possibly present it. Runs under
/// the caller's registry lock so the cap it was admitted under still
/// holds when the entry lands. `rated_lobby` is whether the lobby the
/// players met in rates its games (`LobbyInfo::rated`); a bot's seat
/// never is.
#[allow(clippy::too_many_arguments)]
fn start_match(shared: &Shared, registry: &mut Registry, match_id: Uuid, seed: u64, format: NsgFormat, corp: SeatedPlayer, runner: SeatedPlayer, rated_lobby: bool, table: Option<TableGame>) {
    let round_end = table.as_ref().and_then(|game| game.round_end);
    let mut dealt = shared.decks_for(seed, format);
    // A brought deck replaces the deal for its side, pinned or rotating:
    // the player chose it, and the operator's pin is the default for a
    // seat nobody brought a deck to.
    if let Some(deck) = &corp.deck {
        dealt.corp_id = deck.id.clone();
        dealt.corp = deck.to_deck();
    }
    if let Some(deck) = &runner.deck {
        dealt.runner_id = deck.id.clone();
        dealt.runner = deck.to_deck();
    }
    let (corp_deck_id, runner_deck_id) = (dealt.corp_id, dealt.runner_id);
    // The inputs `GameState::setup` is about to be given, whole: with the
    // history the session hands back, a record that replays the match.
    let header = MatchRecordHeader {
        seed,
        corp_deck: dealt.corp.clone(),
        runner_deck: dealt.runner.clone(),
        rules: MatchRules::default(),
        bot: corp.bot.clone().or_else(|| runner.bot.clone()),
        order: DeckOrder::Shuffled,
    };
    let state = match GameState::setup(&dealt.corp, &dealt.runner, &shared.cards, seed) {
        Ok((state, _events)) => state,
        Err(error) => {
            let reason = format!("match setup failed: {error}");
            for player in [&corp, &runner] {
                if let Some(tx) = player.channel_tx() {
                    refuse(tx, &reason);
                }
            }
            return;
        }
    };

    let seats: Vec<(Side, Uuid, mpsc::UnboundedSender<ServerMessage>, String)> = [(&corp, Side::Corp), (&runner, Side::Runner)]
        .into_iter()
        .filter_map(|(player, side)| player.channel_tx().map(|tx| (side, player.token, tx.clone(), player.lobby.clone().unwrap_or_default())))
        .collect();
    let (corp_name, runner_name) = (corp.name, runner.name);
    let keys = [corp.key, runner.key];
    let started_at = unix_now();
    // Which chairs the two keys took, remembered as the game starts rather
    // than as it ends: the seats are back in their lobby the moment the
    // session lets go of them, before anything after the game runs, so a
    // rematch looked for at once would otherwise be seated off the game
    // before last.
    if let [Some(corp_key), Some(runner_key)] = keys
        && corp_key != runner_key
    {
        shared.remember_pairing(registry, &corp_key.rating_id(), &runner_key.rating_id(), started_at);
    }
    // What each proved seat is asked to sign, with a salt for its deck's
    // hash that only it is told (`statements::deck_hash`).
    let decks = [&dealt.corp, &dealt.runner];
    let mut commitments: [Option<Commitment>; 2] = [None, None];
    let mut sign_requests: Vec<(Side, ServerMessage)> = Vec::new();
    for side in [Side::Corp, Side::Runner] {
        let index = seat_index(side);
        let Some(key) = keys[index] else { continue };
        // A table seat's deck is the one it registered, under the salt it
        // registered with: the hash it signs now is the hash it signed then.
        let salt = match &table {
            Some(game) => game.salts[index].clone(),
            None => netrunner_identity::sha256_hex(&rand::random::<[u8; 16]>())[..32].to_string(),
        };
        let statement = SeatStatement {
            match_id,
            server_key: shared.identity.public_key(),
            side,
            key,
            opponent_key: keys[1 - index],
            deck_hash: statements::deck_hash(&salt, decks[index]),
            started_at,
            table: table.as_ref().map(|game| TableSeat {
                tournament: game.table.tournament.clone(),
                round: game.table.round,
                table: game.table.table,
                seed_commitment: statements::seed_commitment(&game.secret),
                nonce: game.nonces[index].clone(),
            }),
        };
        let payload = serde_json::to_string(&statement).expect("a seat statement serializes");
        sign_requests.push((side, ServerMessage::SignSeat { statement: payload.clone(), salt }));
        commitments[index] = Some(Commitment { key, payload, signed: None });
    }

    // A move is taken back at a host as in a local game: the free kind
    // in every match, and the undo past it only where the lobby does not
    // rate — a bot's lobby never does (Phase 4 §5 stage e).
    let session = MatchSession::new(state, shared.cards.clone(), corp.slot, runner.slot)
        .with_undo(!rated_lobby)
        .with_reconnect_grace(shared.options.reconnect_grace)
        .with_max_spectators(shared.options.max_spectators)
        .with_turn_timeout(shared.options.turn_timeout)
        .with_round_end(round_end);
    let handle = session.reattach_handle();
    registry.matches.insert(
        match_id,
        MatchEntry {
            corp: corp_name,
            runner: runner_name,
            format,
            started_at: Instant::now(),
            handle: handle.clone(),
            commitments,
        },
    );
    // A tournament table's game: the table is playing until it ends, so
    // the organizer cannot record over it and nobody sits at it again.
    if let Some(game) = &table {
        registry.tables.insert(game.table.clone(), TableState::Playing(match_id));
    }

    let mut tokens = Vec::with_capacity(seats.len());
    for (side, session_token, tx, lobby) in seats {
        let ticket = SeatTicket {
            match_id,
            side,
            corp_deck: corp_deck_id.clone(),
            runner_deck: runner_deck_id.clone(),
            handle: handle.clone(),
            lobby,
            tx: tx.clone(),
        };
        let _ = tx.send(ticket.joined(session_token));
        // After `MatchJoined`, before the session's first view.
        for (_, request) in sign_requests.iter().filter(|(requested, _)| *requested == side) {
            let _ = tx.send(request.clone());
        }
        registry.seats.insert(session_token, ticket);
        tokens.push(session_token);
    }

    let shared = shared.clone();
    tokio::spawn(async move {
        let Finished { history, outcome, .. } = session.run_with_outcome().await;
        let (entry, tickets) = {
            let mut registry = shared.lock();
            let entry = registry.matches.remove(&match_id);
            let tickets: Vec<SeatTicket> = tokens.iter().filter_map(|token| registry.seats.remove(token)).collect();
            (entry, tickets)
        };
        let Some(entry) = entry else { return };
        let mut record = Vec::new();
        history.write_jsonl(&header, &mut record).expect("writing to memory cannot fail");
        let ended_at = unix_now();

        // Two proved people with different keys, a winner, in a lobby
        // that rates (which a daemon with no book never has): one key in
        // both chairs is someone playing themselves, which would farm one
        // role's rating off the other's; a forfeit — surrender,
        // disconnect, clock — is a loss like any other; a stall is
        // nobody's.
        let rated = matches!(keys, [Some(corp), Some(runner)] if corp != runner) && outcome.is_some_and(|(winner, _)| winner.is_some()) && rated_lobby;
        let [corp_commitment, runner_commitment] = entry.commitments.map(|commitment| commitment.and_then(|commitment| commitment.signed));
        let receipt = Receipt {
            match_id,
            server_key: shared.identity.public_key(),
            corp: ReceiptSeat { name: entry.corp, key: keys[0], commitment: corp_commitment },
            runner: ReceiptSeat { name: entry.runner, key: keys[1], commitment: runner_commitment },
            winner: outcome.and_then(|(winner, _)| winner),
            reason: outcome.map(|(_, reason)| reason),
            rated,
            format,
            engine: ENGINE.to_string(),
            card_pool: shared.card_pool.to_string(),
            record: netrunner_identity::sha256_hex(&record),
            started_at,
            ended_at,
            action_chain: None,
            table: table.as_ref().map(|game| TableSeed {
                round: game.table.round,
                table: game.table.table,
                commitment: statements::seed_commitment(&game.secret),
                reveal: Some(SeedReveal { secret: game.secret.clone(), corp_nonce: Some(game.nonces[0].clone()), runner_nonce: Some(game.nonces[1].clone()) }),
            }),
        };
        let signed = shared.identity.sign(RECEIPT_TAG, serde_json::to_string(&receipt).expect("a receipt serializes"));
        shared.keep_record(match_id, &record, &signed, ended_at);
        // A table's result is the game's: a win either way, and a stall
        // nobody won or time called with the points even is a tie
        // (1.1.4, 1.1.5.3). Its standings are the tournament's own; the
        // game is not on the ladder.
        if let Some(TableGame { table, .. }) = table {
            let result = match outcome {
                Some((Some(Side::Corp), _)) => TableOutcome::CorpWon,
                Some((Some(Side::Runner), _)) => TableOutcome::RunnerWon,
                Some((None, _)) | None => TableOutcome::Tie,
            };
            shared.table_ended(table, result);
        }
        let [Some(corp), Some(runner)] = keys else { return };
        let Some((Some(winner), _)) = outcome else { return };
        if !rated {
            return;
        }
        let outcome = match winner {
            Side::Corp => Outcome::CorpWin,
            Side::Runner => Outcome::RunnerWin,
        };
        let Some(changes) = shared.rate(&corp.rating_id(), &runner.rating_id(), format, outcome, &signed, ended_at) else { return };
        // To each seat's current connection, which a resume may have
        // replaced; the match has already let go of it.
        for ticket in tickets {
            let (before, after) = changes[seat_index(ticket.side)];
            let _ = ticket.tx.send(ServerMessage::Rated { receipt: Box::new(signed.clone()), before, after });
        }
    });
}

/// A bot seat's name with the style it plays, when it plays one:
/// "planner bot, fast-advance". `MatchList` is the one place a person
/// sees which opponent a daemon seated, and a fast-advance Corp and a
/// glacier Corp are different games.
fn styled(name: String, style: Style) -> String {
    if style.is_balanced() {
        name
    } else {
        format!("{name}, {style}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The daemon's legality gate is wired, and a failure names the deck
    /// and the format rather than surfacing as a mid-match surprise.
    ///
    /// Checked against an empty registry rather than a genuinely illegal
    /// deck because **no shipped format rejects any shipped deck**: every
    /// published decklist is built from `sg` and `elev`, which is exactly
    /// Startup's pool, and no format bans or restricts anything yet (the
    /// tables are a documented seed). So what is testable here is the
    /// wiring — that `DeckFile::validate` is consulted at all and its
    /// verdict reaches the caller — while the rules themselves are covered
    /// where they live, in `netrunner_core::deck::validator`. Before this,
    /// `pin_deck` checked a deck's name and side and nothing else, while
    /// its own doc comment claimed it validated.
    #[test]
    fn an_unplayable_pinned_deck_is_refused_at_bind() {
        let empty = CardRegistry::new();
        let error = pin_deck(Some("discretion_advised"), Side::Corp, &empty, NsgFormat::Startup)
            .expect_err("a deck whose cards are unknown cannot be legal");
        let error = error.to_string();
        assert!(error.contains("discretion_advised"), "{error}");
        assert!(error.contains("Startup"), "the format is named: {error}");
        assert!(error.contains("not legal"), "{error}");
    }

    /// The rotating pool gets the same gate, and for a stronger reason: an
    /// operator who pins a deck sees it refused by name, while a rotating
    /// daemon would deal the offending deck only on whichever seed reached
    /// it. A daemon with a bot and no legal matchup in any format does not
    /// start.
    #[test]
    fn a_bot_daemon_with_no_legal_matchup_anywhere_is_refused_at_bind() {
        let empty = CardRegistry::new();
        let options = ServeOptions { formats: vec![NsgFormat::Startup], ..ServeOptions::default() };
        assert!(!options.bots.is_empty(), "the default daemon offers bots");
        let error = deals_by_format(&empty, &options).expect_err("no deck is legal without cards").to_string();
        assert!(error.contains("no sample matchup"), "{error}");
        assert!(error.contains("Startup"), "{error}");
    }

    #[test]
    fn a_record_is_filed_under_the_month_it_ended_in() {
        let id = Uuid::nil();
        // 2026-09-26T12:00:00Z, the last second of 2025, and the leap day.
        assert!(record_path(Path::new("/d"), id, 1_790_424_000).ends_with(format!("matches/2026-09/{id}.jsonl")));
        assert_eq!(year_month(1_767_225_599), (2025, 12));
        assert_eq!(year_month(1_709_164_800), (2024, 2));
        assert_eq!(year_month(0), (1970, 1));
    }

    /// The real pool against the real registry, in every format: each
    /// deals from the matchups its lists allow. Startup's balance update
    /// and Standard's ban list each refuse some of the published decks, so
    /// each deals from part of the pool; Eternal and Unrestricted deal all of it; Snapshot
    /// — a Fantasy Flight Games pool — allows none, so a daemon seats no
    /// bot there and serves it for people: a daemon serving Snapshot alone
    /// and offering bots has nowhere to seat one and does not start, while
    /// the default daemon, all five formats, starts and offers its bots in
    /// the four that can deal them.
    #[test]
    fn each_format_deals_from_the_part_of_the_pool_it_allows() {
        let registry = fixtures::sample_registry();
        let all = decks::matchups().len();
        for format in [NsgFormat::Startup, NsgFormat::Standard] {
            let legal = legal_matchups(&registry, format).len();
            assert!(legal > 0 && legal < all, "{format:?} deals {legal} of {all}");
        }
        assert_eq!(legal_matchups(&registry, NsgFormat::Eternal).len(), all);
        assert_eq!(legal_matchups(&registry, NsgFormat::Unrestricted).len(), all);
        assert!(legal_matchups(&registry, NsgFormat::Snapshot).is_empty());
        let snapshot = |bots: &[Level]| ServeOptions { formats: vec![NsgFormat::Snapshot], bots: bots.to_vec(), ..ServeOptions::default() };
        assert!(deals_by_format(&registry, &snapshot(&Level::ALL)).is_err());
        assert!(deals_by_format(&registry, &snapshot(&[])).is_ok());
        let deals = deals_by_format(&registry, &ServeOptions::default()).expect("the default daemon starts");
        assert_eq!(deals.len(), 5, "all five formats are served");
        assert!(deals[&NsgFormat::Snapshot].is_empty() && !deals[&NsgFormat::Eternal].is_empty());
    }
}
