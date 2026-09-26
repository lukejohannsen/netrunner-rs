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
//! **A lobby per format, and the players' own.** A daemon offers one or
//! more formats (`ServeOptions::formats`), each a permanent lobby, and
//! any player may make a lobby, open or closed. A player is paired only
//! within their lobby, their deck checked against its format — so one
//! public server holds a Startup queue and a Standard queue side by side.
//! Pairing within a lobby is whoever is waiting: there is no list of
//! waiters to pick an opponent from (declined, 26 September 2026), and a
//! closed lobby is how two people who know each other meet.
//!
//! **The lobby is a queue, not a slot.** A `Seek` either pairs with the
//! first compatible waiter in the same lobby or joins the queue and is
//! told so (`ServerMessage::Queued`). The token issued there is the one
//! `MatchJoined` carries, and a seek is withdrawn with its socket: a
//! queue place is not a seat with a game in it, so it gets no grace.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard};
use std::time::{Duration, Instant};

use clap::ValueEnum;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

use netrunner_bots::{BotAgent, HeuristicAgent, Level, MctsAgent, Personality};
use netrunner_core::cards::CardRegistry;
use netrunner_core::decks::{self, DeckCategory, DeckFile};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::{Deck, GameState, Side};
use netrunner_rating::{Outcome, RatingBook, Track};

use crate::match_session::{MatchSession, PlayerSlot, ReattachHandle, TurnTimeout, DEFAULT_RECONNECT_GRACE};
use crate::protocol::{format_lobby_id, Chair, ClientMessage, LobbyInfo, MatchSummary, ServerMessage};
use crate::fixtures::DealtMatchup;
use crate::{fixtures, net};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServeBotKind {
    Heuristic,
    Mcts,
    /// Queue players looking for a game and pair them into
    /// human-vs-human matches, first come first served within a lobby.
    None,
}

impl ServeBotKind {
    /// What `MatchList` calls a bot seat.
    fn seat_name(self) -> &'static str {
        match self {
            ServeBotKind::Heuristic => "heuristic bot",
            ServeBotKind::Mcts => "mcts bot",
            ServeBotKind::None => unreachable!("a human-vs-human daemon seats no bot"),
        }
    }

}

#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// Bot opponent seated against every connecting client; `None` pairs
    /// humans instead.
    pub bot_runner: ServeBotKind,
    /// Seat a rung of the difficulty ladder instead of `bot_runner`'s
    /// kind — the same override `netrunner_cli --corp-level` makes, so a
    /// daemon's `veteran` and a local `veteran` are the same bot. The
    /// personality still crosses it. Meaningless with `ServeBotKind::None`,
    /// which `bind` refuses.
    pub bot_level: Option<Level>,
    /// The bot's `Personality`, or `None` for the style its dealt deck
    /// names (`DeckFile::style`, balanced when the deck names none).
    pub bot_personality: Option<Personality>,
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
    /// See `MatchSession::with_turn_timeout`; `None` runs without a clock.
    pub turn_timeout: TurnTimeout,
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
    /// Where the daemon keeps its `netrunner_rating::RatingBook`. Loaded
    /// at bind, rewritten after every rated match (temp file plus
    /// rename, like the deck store and the card cache), and the only
    /// thing that makes a rating *persistent*. `None` rates nothing: a
    /// daemon with no file is stateless, which is what every test wants.
    /// **Only a match between two people is rated.** A game against a
    /// seated bot is practice wherever it is played, so a bot daemon given
    /// a file never writes to it (`netrunner_rating::Track`).
    pub ratings_file: Option<PathBuf>,
}

impl Default for ServeOptions {
    fn default() -> Self {
        ServeOptions {
            bot_runner: ServeBotKind::Heuristic,
            bot_level: None,
            bot_personality: None,
            seed: None,
            reconnect_grace: DEFAULT_RECONNECT_GRACE,
            max_matches: None,
            turn_timeout: None,
            corp_deck: None,
            runner_deck: None,
            formats: ALL_FORMATS.to_vec(),
            ratings_file: None,
        }
    }
}

/// Every format, in the order a daemon offers them by default.
pub const ALL_FORMATS: [NsgFormat; 4] = [NsgFormat::Startup, NsgFormat::Standard, NsgFormat::Eternal, NsgFormat::Snapshot];

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

/// Every deck the rotating matchup pool can deal, checked against the
/// daemon's format once at `bind`.
///
/// The pool is `decks::matchups()`, which is the sample decks' cross
/// product, so this is the same set `netrunner_core`'s own
/// `every_sample_deck_is_legal` covers — but that test fixes the format it
/// checks, and an operator picks one. A daemon serving Startup out of a
/// pool that is only Eternal-legal should refuse to start, not deal an
/// illegal game on whichever seed reaches the offending deck.
fn check_rotating_pool(registry: &CardRegistry, format: NsgFormat) -> std::io::Result<()> {
    for side in [Side::Corp, Side::Runner] {
        for deck in decks::for_side(side).into_iter().filter(|deck| deck.category == DeckCategory::Sample) {
            if let Err(e) = deck.validate(registry, format) {
                return Err(std::io::Error::other(format!(
                    "sample deck {:?} is not legal in {format:?}: {e}; pin a legal matchup or serve another format",
                    deck.id
                )));
            }
        }
    }
    Ok(())
}

fn make_serve_agent(kind: ServeBotKind, side: Side, seed: u64, personality: Personality) -> Box<dyn BotAgent> {
    match kind {
        ServeBotKind::Heuristic => Box::new(HeuristicAgent::with_personality(side, seed, personality)),
        ServeBotKind::Mcts => Box::new(MctsAgent::new(side, seed).with_personality(personality)),
        ServeBotKind::None => unreachable!("caller only invokes this for a bot-backed ServeBotKind"),
    }
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
}

/// An attached connection looking for a game (`Seek`), waiting for
/// another in the same lobby (`ServeBotKind::None` only).
struct PendingHuman {
    token: Uuid,
    player_name: String,
    /// The lobby's id: a waiter pairs only within its own, and returns to
    /// it after the game.
    lobby: String,
    /// The lobby's format, which every deck here was checked against.
    format: NsgFormat,
    /// A chosen chair's deck, already checked; its side is the seat.
    deck: Option<Box<DeckFile>>,
    /// A random chair's two decks, Corp then Runner: the side is picked at
    /// pairing (`assign_sides`) and the deck for it is played.
    random: Option<(Box<DeckFile>, Box<DeckFile>)>,
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
        SeatedPlayer { rating_id: Some(self.player_name.clone()), name: self.player_name, token: self.token, slot: self.slot, deck, lobby: Some(self.lobby) }
    }

    /// The side this player must play, if their deck fixes one.
    fn deck_side(&self) -> Option<Side> {
        self.deck.as_ref().map(|deck| deck.side)
    }
}

/// Whether two waiters can be one match: not if both chose the same
/// chair. A random chair sits anywhere.
fn compatible(a: &PendingHuman, b: &PendingHuman) -> bool {
    !matches!((a.deck_side(), b.deck_side()), (Some(x), Some(y)) if x == y)
}


/// A lobby a player made (`ClientMessage::CreateLobby`). The server's own
/// lobbies, one per format, are not stored: they are `options.formats`.
struct PlayerLobby {
    name: String,
    format: NsgFormat,
    closed: bool,
    /// Held in memory only and never sent: a lobby's password is a
    /// door for a few friends, not an account's.
    password: Option<String>,
}

/// A player about to be seated: the name `MatchList` will show, the id
/// the rating book knows them by (the name itself for a human, `None`
/// for a bot, which is what leaves a game against one unrated), the token `MatchJoined` will carry (already
/// issued if they came through the lobby, so one token spans queue and
/// match), and the slot the session plays them through. A bot seat
/// carries a token too, unused — cheaper than a second type for the one
/// case that never resumes.
struct SeatedPlayer {
    name: String,
    rating_id: Option<String>,
    token: Uuid,
    slot: PlayerSlot,
    /// The deck the player brought, which replaces whatever the daemon
    /// would have dealt their side. `None` for a bot, which is dealt.
    deck: Option<Box<DeckFile>>,
    /// The lobby a player returns to after the game; `None` for a bot.
    lobby: Option<String>,
}

impl SeatedPlayer {
    fn channel_tx(&self) -> Option<&mpsc::UnboundedSender<ServerMessage>> {
        match &self.slot {
            PlayerSlot::Channel { tx, .. } => Some(tx),
            PlayerSlot::Bot(_) => None,
        }
    }
}

/// The daemon's whole bookkeeping — see the module doc for why it is one
/// struct under one lock.
#[derive(Default)]
struct Registry {
    matches: HashMap<Uuid, MatchEntry>,
    seats: HashMap<Uuid, SeatTicket>,
    /// Every rating the daemon holds; see `ServeOptions::ratings_file`.
    ratings: RatingBook,
    /// Arrival order; pairing takes the first waiter in the newcomer's lobby.
    lobby: Vec<PendingHuman>,
    /// The lobbies players made, by id.
    player_lobbies: HashMap<String, PlayerLobby>,
    /// How many attached connections are in each lobby, by id.
    members: HashMap<String, usize>,
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
        self.lobby.retain(|waiter| !waiter.tx.is_closed());
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
            waiting_in_lobby: self.lobby.len(),
            max_matches: options.max_matches,
        }
    }

    /// Waiters looking for a game in lobby `id`.
    fn seeking(&self, id: &str) -> usize {
        self.lobby.iter().filter(|waiter| waiter.lobby == id).count()
    }

    /// Lobby `id` as a player sees it, or `None` if there is no such lobby.
    fn lobby_info(&self, id: &str, options: &ServeOptions) -> Option<LobbyInfo> {
        let (name, format, permanent, closed, password) = match self.player_lobbies.get(id) {
            Some(lobby) => (lobby.name.clone(), lobby.format, false, lobby.closed, lobby.password.is_some()),
            None => {
                let format = *options.formats.iter().find(|&&format| format_lobby_id(format) == id)?;
                (format!("{format:?}"), format, true, false, false)
            }
        };
        Some(LobbyInfo {
            id: id.to_string(),
            name,
            format,
            permanent,
            closed,
            password,
            players: self.members.get(id).copied().unwrap_or(0),
            seeking: self.seeking(id),
        })
    }

    /// The open lobbies: the server's own, one per format in the order it
    /// offers them, then the players' by name.
    fn open_lobbies(&self, options: &ServeOptions) -> Vec<LobbyInfo> {
        let mut theirs: Vec<&String> = self.player_lobbies.iter().filter(|(_, lobby)| !lobby.closed).map(|(id, _)| id).collect();
        theirs.sort_by_key(|id| (self.player_lobbies[*id].name.to_lowercase(), (*id).clone()));
        options
            .formats
            .iter()
            .map(|&format| format_lobby_id(format))
            .chain(theirs.into_iter().cloned())
            .filter_map(|id| self.lobby_info(&id, options))
            .collect()
    }

    /// A player's lobby goes when nobody is in it and nobody waits in it.
    /// The server's own never go.
    fn forget_if_empty(&mut self, id: &str) {
        if self.player_lobbies.contains_key(id) && self.members.get(id).copied().unwrap_or(0) == 0 && self.seeking(id) == 0 {
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
    fn decks_for(&self, seed: u64) -> DealtMatchup {
        let mut dealt = fixtures::sample_decks_for_seed(seed);
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

    /// Rates one finished match and rewrites the book. Under the registry
    /// lock, so two matches ending at once serialize their updates; the
    /// write is a few kilobytes and the lock is never held across an
    /// await, so that is fine. A failed write is logged, not fatal: the
    /// ratings are already applied in memory and the next match's write
    /// carries them.
    fn rate(&self, corp: &str, runner: &str, outcome: Outcome) {
        let Some(path) = &self.options.ratings_file else { return };
        let mut registry = self.lock();
        let (corp_after, runner_after) = registry.ratings.record(Track::HumanVsHuman, corp, runner, outcome);
        tracing::info!(
            corp, runner, ?outcome,
            corp_rating = corp_after.corp.rating.rating, runner_rating = runner_after.runner.rating.rating,
            "match rated"
        );
        if let Err(error) = save_ratings(path, &registry.ratings) {
            tracing::warn!(path = %path.display(), ?error, "could not save the rating book");
        }
    }
}

fn load_ratings(path: &Path) -> std::io::Result<RatingBook> {
    if !path.exists() {
        return Ok(RatingBook::default());
    }
    let json = std::fs::read_to_string(path)?;
    RatingBook::from_json(&json).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

/// Temp file plus rename, so a crash mid-write leaves the previous book
/// intact rather than half a JSON document.
fn save_ratings(path: &Path, book: &RatingBook) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, book.to_json())?;
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
        Self::with_listener(listener, options)
    }

    /// Serves on a listener somebody else bound. It exists for a host that
    /// needs a socket option `bind` cannot say: `netrunner_client::hosting`
    /// binds `[::]` with `IPV6_V6ONLY` off, so one listener takes IPv4 and
    /// IPv6 on every platform (Windows defaults the option on). The option
    /// is set by the caller rather than here so this crate takes no socket
    /// dependency of its own. Must be called inside a tokio runtime.
    pub fn from_listener(listener: std::net::TcpListener, options: ServeOptions) -> std::io::Result<Self> {
        Self::check_options(&options)?;
        listener.set_nonblocking(true)?;
        Self::with_listener(TcpListener::from_std(listener)?, options)
    }

    fn check_options(options: &ServeOptions) -> std::io::Result<()> {
        if options.bot_level.is_some() && options.bot_runner == ServeBotKind::None {
            return Err(std::io::Error::other("--bot-level seats a bot, but --bot-runner none pairs humans; drop one of them"));
        }
        if options.formats.is_empty() {
            return Err(std::io::Error::other("a daemon needs at least one format to pair players in"));
        }
        Ok(())
    }

    fn with_listener(listener: TcpListener, options: ServeOptions) -> std::io::Result<Self> {
        let base_seed = options.seed.unwrap_or_else(rand::random);
        let ratings = match &options.ratings_file {
            Some(path) => load_ratings(path)?,
            None => RatingBook::default(),
        };
        let cards = fixtures::sample_registry();
        let mut pinned = PinnedDecks::default();
        for &format in &options.formats {
            pinned = PinnedDecks {
                corp: pin_deck(options.corp_deck.as_deref(), Side::Corp, &cards, format)?,
                runner: pin_deck(options.runner_deck.as_deref(), Side::Runner, &cards, format)?,
            };
        }
        // The rotating pool needs the same gate as a pinned deck, and for
        // a better reason: an operator who pins a deck names it and would
        // see it refused, while a rotating daemon deals whatever the seed
        // picks and would only find out mid-match. Checked once here
        // rather than per match — the pool is embedded and cannot change
        // while the process runs.
        for &format in &options.formats {
            check_rotating_pool(&cards, format)?;
        }
        let shared = Shared {
            cards,
            registry: Arc::new(StdMutex::new(Registry { ratings, ..Registry::default() })),
            options,
            base_seed,
            pinned,
        };
        Ok(Server { listener, shared })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// A door into this server for connections it did not accept itself.
    /// Take one before `run`, which consumes the server.
    pub fn acceptor(&self) -> Acceptor {
        Acceptor { shared: self.shared.clone() }
    }

    /// The accept loop. Returns only if `accept` itself fails.
    pub async fn run(self) -> std::io::Result<()> {
        tracing::info!(addr = %self.local_addr()?, bot_runner = ?self.shared.options.bot_runner, "netrunner_server listening");
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
/// it joins; anything else is skipped until one of these arrives.
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

    let handshake = loop {
        match ws_stream.next().await {
            Some(Ok(WsMessage::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Resume { session_token }) => break Handshake::Resume { session_token },
                Ok(ClientMessage::Spectate { match_id }) => break Handshake::Spectate { match_id },
                Ok(ClientMessage::Attach { player_name }) => break Handshake::Attach { player_name },
                Ok(ClientMessage::ListMatches) => {
                    ws_stream.send(WsMessage::Text(serde_json::to_string(&shared.match_list())?)).await?;
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
                if ticket.handle.reattach(ticket.side, out_tx, into_rx).is_err() {
                    // Lost the race with the match ending between the
                    // liveness check and here. The bridge is already up,
                    // so the refusal goes down the same channel.
                    let _ = session_tx.send(ServerMessage::ResumeRejected { reason: "the match ended".into() });
                    return Ok(());
                }
                let playing = attached::Playing { token: session_token, out: out_rx, into: into_tx, lobby: ticket.lobby.clone() };
                tokio::spawn(attached::run(shared, name.unwrap_or_default(), session_tx, session_rx, Some(playing)));
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
            tracing::info!(%player_name, "client attached");
            let (session_tx, bridge_rx) = mpsc::unbounded_channel::<ServerMessage>();
            let (bridge_tx, session_rx) = mpsc::unbounded_channel::<ClientMessage>();
            tokio::spawn(net::bridge_websocket(ws_stream, bridge_tx, bridge_rx));
            tokio::spawn(attached::run(shared, player_name, session_tx, session_rx, None));
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
            // `Spectating` before the control message, so it precedes the
            // `StateUpdate` the session answers with.
            let _ = session_tx.send(ServerMessage::Spectating { match_id });
            if handle.add_spectator(session_tx.clone()).is_err() {
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

#[allow(clippy::too_many_arguments)]
fn seat_vs_bot(
    shared: &Shared,
    kind: ServeBotKind,
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
    let human = SeatedPlayer { rating_id: Some(player_name.clone()), name: player_name, token: Uuid::new_v4(), slot, deck: Some(deck), lobby: Some(lobby) };
    // The same deal `start_match` will make — `decks_for` is a function of
    // the seed — so the bot's style can come off the deck it is about to
    // play. A pinned deck is an embedded id (`pin_deck`), so `by_id`
    // always resolves; the fallback is only for a future pool that is not.
    let personality = shared.options.bot_personality.unwrap_or_else(|| {
        let dealt = shared.decks_for(seed);
        let bot_deck_id = match human_side {
            Side::Corp => &dealt.runner_id,
            Side::Runner => &dealt.corp_id,
        };
        decks::by_id(bot_deck_id).and_then(|deck| Personality::for_deck(&deck).ok()).unwrap_or_default()
    });
    let bot_side = human_side.other();
    let bot_seed = seed.wrapping_add(1);
    let bot = match shared.options.bot_level {
        Some(level) => SeatedPlayer {
            name: styled(format!("{} bot", level.name()), personality),
            rating_id: None,
            token: Uuid::new_v4(),
            slot: PlayerSlot::Bot(level.spec(bot_side).with_personality(personality).agent(bot_seed)),
            deck: None,
            lobby: None,
        },
        None => SeatedPlayer {
            name: styled(kind.seat_name().to_string(), personality),
            rating_id: None,
            token: Uuid::new_v4(),
            slot: PlayerSlot::Bot(make_serve_agent(kind, bot_side, bot_seed, personality)),
            deck: None,
            lobby: None,
        },
    };
    let (corp, runner) = match human_side {
        Side::Corp => (human, bot),
        Side::Runner => (bot, human),
    };
    start_match(shared, &mut registry, match_id, seed, format, corp, runner);
}

/// `ServeBotKind::None`: pair with the first compatible waiter in the same
/// lobby, or join its queue.
fn enqueue_or_pair(shared: &Shared, newcomer: PendingHuman) {
    let mut registry = shared.lock();
    registry.sweep_lobby();
    if registry.at_cap(&shared.options) {
        refuse(&newcomer.tx, AT_CAP);
        return;
    }

    // The first waiter in the lobby who can sit opposite: two Corp decks
    // skip each other and both wait for a Runner.
    let Some(index) = registry.lobby.iter().position(|waiter| waiter.lobby == newcomer.lobby && compatible(waiter, &newcomer)) else {
        let (token, tx) = (newcomer.token, newcomer.tx.clone());
        registry.lobby.push(newcomer);
        let position = registry.lobby.len();
        let _ = tx.send(ServerMessage::Queued { session_token: token, position });
        return;
    };
    let waiter = registry.lobby.remove(index);
    let format = waiter.format;
    let (match_id, seed) = registry.allocate(shared.base_seed);
    let (corp, runner) = assign_sides(waiter, newcomer, seed);
    start_match(shared, &mut registry, match_id, seed, format, corp.seated(Side::Corp), runner.seated(Side::Runner));
}

/// Sets up the state, builds the session, records the match and a ticket
/// per channel seat, tells each seat it has joined, and runs the match;
/// the match and its tickets are dropped when it returns. One place for
/// all of it so the bot and lobby paths cannot disagree on the order —
/// `MatchJoined` must precede the session's first `StateUpdate`, and a
/// ticket must exist before a client can possibly present it. Runs under
/// the caller's registry lock so the cap it was admitted under still
/// holds when the entry lands.
fn start_match(shared: &Shared, registry: &mut Registry, match_id: Uuid, seed: u64, format: NsgFormat, corp: SeatedPlayer, runner: SeatedPlayer) {
    let mut dealt = shared.decks_for(seed);
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
    let (corp_rating_id, runner_rating_id) = (corp.rating_id, runner.rating_id);

    let session = MatchSession::new(state, shared.cards.clone(), corp.slot, runner.slot)
        .with_reconnect_grace(shared.options.reconnect_grace)
        .with_turn_timeout(shared.options.turn_timeout);
    let handle = session.reattach_handle();
    registry.matches.insert(
        match_id,
        MatchEntry {
            corp: corp_name,
            runner: runner_name,
            format,
            started_at: Instant::now(),
            handle: handle.clone(),
        },
    );

    let mut tokens = Vec::with_capacity(seats.len());
    for (side, session_token, tx, lobby) in seats {
        let ticket = SeatTicket {
            match_id,
            side,
            corp_deck: corp_deck_id.clone(),
            runner_deck: runner_deck_id.clone(),
            handle: handle.clone(),
            lobby,
        };
        let _ = tx.send(ticket.joined(session_token));
        registry.seats.insert(session_token, ticket);
        tokens.push(session_token);
    }

    let shared = shared.clone();
    tokio::spawn(async move {
        let (_state, outcome) = session.run_with_outcome().await;
        {
            let mut registry = shared.lock();
            registry.matches.remove(&match_id);
            for token in tokens {
                registry.seats.remove(&token);
            }
        }
        // Two people, or nothing: a seat with no rating id is a bot's.
        let (Some(corp), Some(runner)) = (corp_rating_id, runner_rating_id) else { return };
        // A forfeit — surrender, disconnect, clock — is a loss like any
        // other; a stall (`None`) is nobody's and goes unrated.
        let outcome = match outcome {
            Some((Side::Corp, _)) => Outcome::CorpWin,
            Some((Side::Runner, _)) => Outcome::RunnerWin,
            None => return,
        };
        shared.rate(&corp, &runner, outcome);
    });
}

/// A bot seat's name with the style it plays, when it plays one: "heuristic
/// bot, rush". `MatchList` is the one place a person sees which opponent a
/// daemon seated, and a rush Corp and a glacier Corp are different games.
fn styled(name: String, personality: Personality) -> String {
    match personality {
        Personality::Balanced => name,
        personality => format!("{name}, {personality}"),
    }
}

/// A chosen chair first — `compatible` has already ruled out two of the
/// same, and a random chair sits opposite it — and for two random chairs
/// a coin, tossed off the match's seed so a `--seed` run seats the same
/// way every time. Corp first.
fn assign_sides(a: PendingHuman, b: PendingHuman, seed: u64) -> (PendingHuman, PendingHuman) {
    match (a.deck_side(), b.deck_side()) {
        (Some(Side::Corp), _) | (_, Some(Side::Runner)) => (a, b),
        (Some(Side::Runner), _) | (_, Some(Side::Corp)) => (b, a),
        (None, None) if seed & 1 == 0 => (a, b),
        (None, None) => (b, a),
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
    /// it.
    #[test]
    fn an_unplayable_rotating_pool_is_refused_at_bind() {
        let empty = CardRegistry::new();
        let error = check_rotating_pool(&empty, NsgFormat::Startup).expect_err("the pool cannot be legal");
        let error = error.to_string();
        assert!(error.contains("sample deck"), "{error}");
        assert!(error.contains("Startup"), "{error}");
    }

    /// The real pool against the real registry, in every format the daemon
    /// can be asked to serve. This is what stops the gate from being a
    /// startup failure the day someone runs `--format standard`.
    #[test]
    fn every_shipped_format_can_actually_serve_the_sample_pool() {
        let registry = fixtures::sample_registry();
        for format in [NsgFormat::Startup, NsgFormat::Standard, NsgFormat::Eternal, NsgFormat::Snapshot] {
            check_rotating_pool(&registry, format)
                .unwrap_or_else(|e| panic!("a daemon serving {format:?} must be able to start: {e}"));
        }
    }
}
