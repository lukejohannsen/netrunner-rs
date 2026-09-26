//! The lobby and the match registry over real WebSockets: pairing within
//! a lobby, ghost sweeping, the match cap, the seed policy, the decks a
//! bot is dealt, ratings, spectators and `ListMatches`. Every player comes
//! in attached and looks for a game with the deck its chair needs
//! (`tests/attached.rs` covers the lobbies themselves); `tests/reconnect.rs`
//! covers a seat's lifetime after `MatchJoined`.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use uuid::Uuid;

use netrunner_bots::Personality;
use netrunner_core::decks::{self, DeckFile};
use netrunner_core::dsl::CardId;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::{PlayerAction, Side, Viewer};
use netrunner_core::view::ClientView;
use netrunner_rating::{RatingBook, Track};
use netrunner_server::protocol::Chair;
use netrunner_server::serve::{ServeBotKind, ServeOptions, Server};
use netrunner_server::{ClientMessage, MatchSummary, ServerMessage};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn start_server(options: ServeOptions) -> String {
    let server = Server::bind("127.0.0.1:0", options).await.expect("an ephemeral port binds");
    let addr = server.local_addr().unwrap();
    tokio::spawn(server.run());
    format!("ws://{addr}")
}

async fn human_daemon() -> String {
    start_server(ServeOptions { bot_runner: ServeBotKind::None, seed: Some(1), ..ServeOptions::default() }).await
}

fn bot_daemon() -> ServeOptions {
    ServeOptions { bot_runner: ServeBotKind::Heuristic, seed: Some(1), ..ServeOptions::default() }
}

async fn open(url: &str, hello: ClientMessage) -> Socket {
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.expect("the server accepts");
    send(&mut socket, hello).await;
    socket
}

async fn send(socket: &mut Socket, message: ClientMessage) {
    socket.send(WsMessage::Text(serde_json::to_string(&message).unwrap())).await.unwrap();
}

async fn next(socket: &mut Socket) -> ServerMessage {
    let deadline = Duration::from_secs(10);
    loop {
        let frame = tokio::time::timeout(deadline, socket.next()).await.expect("the server answers within 10s");
        match frame {
            Some(Ok(WsMessage::Text(text))) => return serde_json::from_str(&text).expect("a ServerMessage"),
            Some(Ok(_)) => continue,
            other => panic!("socket ended: {other:?}"),
        }
    }
}

/// Runs until the server closes the socket (or the stream errors), so a
/// test can assert a refusal ends the connection.
async fn closed_by_server(socket: &mut Socket) -> bool {
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(frame) = socket.next().await {
            if matches!(frame, Ok(WsMessage::Close(_)) | Err(_)) {
                return true;
            }
        }
        true
    })
    .await
    .unwrap_or(false)
}

/// A published list under a new id, as a player's saved copy of it would
/// be: legal, and recognisably not what a rotation would have dealt.
fn brought(published: &str, id: &str) -> Box<DeckFile> {
    let mut deck = decks::by_id(published).expect("an embedded deck");
    deck.id = id.to_string();
    deck.name = format!("{id} (brought)");
    Box::new(deck)
}

fn corp(id: &str) -> Chair {
    Chair::Corp(brought("brick_stack", id))
}

fn runner(id: &str) -> Chair {
    Chair::Runner(brought("dashing_mad", id))
}

/// Attaches as `name`, joins `lobby` and looks for a game in `chair`. The
/// next message is `Queued`, `MatchJoined` or `SeekRefused`.
async fn seek_in(url: &str, name: &str, lobby: &str, chair: Chair) -> Socket {
    let mut socket = open(url, ClientMessage::Attach { player_name: name.into() }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::Attached { .. }));
    send(&mut socket, ClientMessage::JoinLobby { lobby: lobby.into(), password: None }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::LobbyJoined { .. }));
    send(&mut socket, ClientMessage::Seek { chair }).await;
    socket
}

async fn seek(url: &str, name: &str, chair: Chair) -> Socket {
    seek_in(url, name, "startup", chair).await
}

fn joined(message: ServerMessage) -> (Uuid, Side, Uuid) {
    let (match_id, assigned_side, session_token, _, _) = joined_with_decks(message);
    (match_id, assigned_side, session_token)
}

fn joined_with_decks(message: ServerMessage) -> (Uuid, Side, Uuid, String, String) {
    match message {
        ServerMessage::MatchJoined { match_id, assigned_side, session_token, corp_deck, runner_deck } => {
            (match_id, assigned_side, session_token, corp_deck, runner_deck)
        }
        other => panic!("expected MatchJoined, got {other:?}"),
    }
}

fn queued(message: ServerMessage) -> (Uuid, usize) {
    match message {
        ServerMessage::Queued { session_token, position } => (session_token, position),
        other => panic!("expected Queued, got {other:?}"),
    }
}

fn state_update(message: ServerMessage) -> ClientView {
    match message {
        ServerMessage::StateUpdate(view) => *view,
        other => panic!("expected StateUpdate, got {other:?}"),
    }
}

fn seek_refused(message: ServerMessage) -> String {
    match message {
        ServerMessage::SeekRefused { reason } => reason,
        other => panic!("expected SeekRefused, got {other:?}"),
    }
}

/// One `ListMatches` on a fresh socket, closed afterwards.
async fn list_matches(url: &str) -> (Vec<MatchSummary>, usize) {
    let mut socket = open(url, ClientMessage::ListMatches).await;
    let reply = next(&mut socket).await;
    let _ = socket.close(None).await;
    match reply {
        ServerMessage::MatchList { matches, waiting_in_lobby, .. } => (matches, waiting_in_lobby),
        other => panic!("expected MatchList, got {other:?}"),
    }
}

/// The server notices a closed socket asynchronously; this is how a test
/// waits for that instead of sleeping.
async fn wait_until_lobby_holds(url: &str, expected: usize) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if list_matches(url).await.1 == expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the lobby never came to hold {expected} waiters"));
}

#[tokio::test]
async fn two_humans_are_paired_into_one_match_on_opposite_sides() {
    let url = human_daemon().await;

    let mut first = seek(&url, "first", runner("first_deck")).await;
    let (_, position) = queued(next(&mut first).await);
    assert_eq!(position, 1);

    let mut second = seek(&url, "second", corp("second_deck")).await;
    let (second_match, second_side, _) = joined(next(&mut second).await);
    let (first_match, first_side, _) = joined(next(&mut first).await);

    assert_eq!(first_match, second_match, "one match for both");
    assert_eq!((first_side, second_side), (Side::Runner, Side::Corp), "each in the chair it chose");
    state_update(next(&mut first).await);
    state_update(next(&mut second).await);

    let (matches, waiting) = list_matches(&url).await;
    assert_eq!(waiting, 0);
    assert_eq!(matches.len(), 1);
    assert_eq!((matches[0].corp.as_str(), matches[0].runner.as_str(), matches[0].format), ("second", "first", NsgFormat::Startup));
}

#[tokio::test]
async fn a_waiting_player_whose_socket_closed_is_never_paired() {
    let url = human_daemon().await;

    let mut ghost = seek(&url, "ghost", corp("ghost")).await;
    queued(next(&mut ghost).await);
    ghost.close(None).await.unwrap();
    drop(ghost);
    wait_until_lobby_holds(&url, 0).await;

    let mut second = seek(&url, "second", corp("second")).await;
    let (_, position) = queued(next(&mut second).await);
    assert_eq!(position, 1, "the ghost is gone, not ahead in the queue");

    let mut third = seek(&url, "third", runner("third")).await;
    let (third_match, _, _) = joined(next(&mut third).await);
    let (second_match, _, _) = joined(next(&mut second).await);
    assert_eq!(second_match, third_match, "the two live players pair with each other");
}

#[tokio::test]
async fn four_humans_make_two_concurrent_matches() {
    let url = human_daemon().await;
    let mut sockets = Vec::new();
    let mut match_ids = Vec::new();
    for pair in 0..2 {
        let mut a = seek(&url, &format!("a{pair}"), corp("a")).await;
        queued(next(&mut a).await);
        let mut b = seek(&url, &format!("b{pair}"), runner("b")).await;
        let (b_match, _, _) = joined(next(&mut b).await);
        let (a_match, _, _) = joined(next(&mut a).await);
        assert_eq!(a_match, b_match);
        match_ids.push(a_match);
        sockets.push((a, b));
    }
    assert_ne!(match_ids[0], match_ids[1], "two matches, two ids");

    let (matches, waiting) = list_matches(&url).await;
    assert_eq!(waiting, 0);
    let mut names: Vec<(String, String)> = matches.into_iter().map(|m| (m.corp, m.runner)).collect();
    names.sort();
    assert_eq!(names, vec![("a0".to_string(), "b0".to_string()), ("a1".to_string(), "b1".to_string())]);
}

/// Two chairs of the same side wait for the other side rather than pair,
/// and the first compatible waiter is the one taken.
#[tokio::test]
async fn two_decks_for_the_same_side_never_pair() {
    let url = human_daemon().await;
    let mut first = seek(&url, "first", corp("corp_a")).await;
    queued(next(&mut first).await);
    let mut second = seek(&url, "second", Chair::Corp(brought("fine_print", "corp_b"))).await;
    let (_, position) = queued(next(&mut second).await);
    assert_eq!(position, 2, "a second Corp waits rather than being seated as the Runner");

    let mut third = seek(&url, "third", runner("runner_c")).await;
    let (third_match, _, _) = joined(next(&mut third).await);
    let (first_match, _, _, corp_deck, _) = joined_with_decks(next(&mut first).await);
    assert_eq!((first_match, corp_deck.as_str()), (third_match, "corp_a"), "the first compatible waiter");
    assert_eq!(list_matches(&url).await.1, 1, "the second Corp is still waiting");
}

/// A deck the lobby's format does not allow is refused before the queue.
#[tokio::test]
async fn an_illegal_deck_is_refused_before_the_queue() {
    let url = human_daemon().await;
    let mut thin = brought("brick_stack", "thin");
    thin.cards.truncate(2);
    let mut socket = seek(&url, "thin", Chair::Corp(thin)).await;
    let reason = seek_refused(next(&mut socket).await);
    assert!(reason.contains("not legal in Startup"), "{reason}");
    assert_eq!(list_matches(&url).await.1, 0, "never reached the queue");
}

/// A lobby per format: a Startup player and a Standard player each wait
/// in their own, the next Startup player pairs with the first, and the
/// match is listed under its format.
#[tokio::test]
async fn players_are_paired_only_within_their_format() {
    let url = human_daemon().await;
    let mut startup = seek_in(&url, "startup", "startup", corp("s")).await;
    queued(next(&mut startup).await);
    let mut standard = seek_in(&url, "standard", "standard", runner("t")).await;
    let (_, position) = queued(next(&mut standard).await);
    assert_eq!(position, 2, "queued, not paired across formats");

    let mut second = seek_in(&url, "second", "startup", runner("r")).await;
    let (second_match, _, _) = joined(next(&mut second).await);
    let (startup_match, _, _) = joined(next(&mut startup).await);
    assert_eq!(second_match, startup_match);
    let (matches, waiting) = list_matches(&url).await;
    assert_eq!((matches.len(), waiting), (1, 1), "the Standard player still waits");
    assert_eq!(matches[0].format, NsgFormat::Startup);
}

/// Only the formats a daemon offers are lobbies, a player's lobby must be
/// in one of them, and a daemon with none does not start.
#[tokio::test]
async fn a_format_the_daemon_does_not_offer_is_no_lobby() {
    let url = start_server(ServeOptions { bot_runner: ServeBotKind::None, formats: vec![NsgFormat::Standard], ..ServeOptions::default() }).await;
    let mut socket = open(&url, ClientMessage::Attach { player_name: "eternal".into() }).await;
    let ServerMessage::Attached { lobbies } = next(&mut socket).await else { panic!() };
    assert_eq!(lobbies.iter().map(|lobby| lobby.id.as_str()).collect::<Vec<_>>(), ["standard"]);
    send(&mut socket, ClientMessage::JoinLobby { lobby: "eternal".into(), password: None }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::LobbyRefused { .. }));
    send(&mut socket, ClientMessage::CreateLobby { name: "old cards".into(), format: NsgFormat::Eternal, closed: false, password: None }).await;
    let ServerMessage::LobbyRefused { reason } = next(&mut socket).await else { panic!() };
    assert!(reason.contains("no Eternal lobby"), "{reason}");
    assert!(Server::bind("127.0.0.1:0", ServeOptions { formats: Vec::new(), ..ServeOptions::default() }).await.is_err(), "a daemon with no lobby does not start");
}

/// A brought deck is its player's secret (Phase 4 §7 stage 2): each seat
/// is told its own deck's id and never its opponent's — a saved deck's
/// id is a slug of the name its builder gave it — on seating and on a
/// resume alike, and the match list names no decks at all.
#[tokio::test]
async fn a_seat_is_told_its_own_deck_and_never_its_opponents() {
    let url = human_daemon().await;
    let mut corp_seat = seek(&url, "corp", corp("secret_plan")).await;
    queued(next(&mut corp_seat).await);
    let mut runner_seat = seek(&url, "runner", runner("my_heist")).await;
    let (_, _, runner_token, runner_sees_corp, runner_sees_runner) = joined_with_decks(next(&mut runner_seat).await);
    let (_, _, _, corp_sees_corp, corp_sees_runner) = joined_with_decks(next(&mut corp_seat).await);
    assert_eq!((corp_sees_corp.as_str(), corp_sees_runner.as_str()), ("secret_plan", ""));
    assert_eq!((runner_sees_corp.as_str(), runner_sees_runner.as_str()), ("", "my_heist"));

    let listed = serde_json::to_string(&list_matches(&url).await.0).unwrap();
    assert!(!listed.contains("secret_plan") && !listed.contains("my_heist"), "the list names no deck: {listed}");

    // A resume is told the same, and no more.
    let mut again = open(&url, ClientMessage::Resume { session_token: runner_token }).await;
    let (_, _, _, corp_deck, runner_deck) = joined_with_decks(next(&mut again).await);
    assert_eq!((corp_deck.as_str(), runner_deck.as_str()), ("", "my_heist"));
}

#[tokio::test]
async fn the_seed_policy_is_deterministic_and_per_match() {
    let url_a = start_server(bot_daemon()).await;
    let url_b = start_server(bot_daemon()).await;

    let mut a1 = seek(&url_a, "a1", corp("a1")).await;
    joined(next(&mut a1).await);
    let opening_a1 = state_update(next(&mut a1).await);

    let mut b1 = seek(&url_b, "b1", corp("b1")).await;
    joined(next(&mut b1).await);
    let opening_b1 = state_update(next(&mut b1).await);
    assert_eq!(opening_a1, opening_b1, "two daemons on the same --seed deal the same first match");

    let mut a2 = seek(&url_a, "a2", corp("a2")).await;
    joined(next(&mut a2).await);
    let opening_a2 = state_update(next(&mut a2).await);
    assert_ne!(opening_a1.corp.hq_cards, opening_a2.corp.hq_cards, "the second match on a daemon is a different shuffle");
}

/// A bot is dealt a published decklist, a different one per match: the
/// property that puts its games on the same pool every bot in the
/// workspace is measured on. Read off the identity the view shows, since
/// a seat is told only its own deck's id.
#[tokio::test]
async fn a_bot_is_dealt_a_published_deck_and_the_pool_rotates() {
    let url = start_server(bot_daemon()).await;
    let published: Vec<Option<CardId>> = decks::matchups().into_iter().map(|(_, runner)| Some(runner.identity)).collect();

    let mut first = seek(&url, "first", corp("first")).await;
    joined(next(&mut first).await);
    let first_view = state_update(next(&mut first).await);
    let mut second = seek(&url, "second", corp("second")).await;
    joined(next(&mut second).await);
    let second_view = state_update(next(&mut second).await);

    for view in [&first_view, &second_view] {
        assert!(published.contains(&view.runner.identity), "{:?} is not a published Runner deck", view.runner.identity);
    }
    assert_ne!(first_view.runner.identity, second_view.runner.identity, "consecutive matches rotate the pool");
}

/// Pinning the bot's deck deals it to every match, and a name that is not
/// a deck refuses to start rather than refusing every client.
#[tokio::test]
async fn a_pinned_bot_deck_is_dealt_to_every_match_and_a_bad_id_fails_to_bind() {
    let url = start_server(ServeOptions { corp_deck: Some("discretion_advised".into()), ..bot_daemon() }).await;
    let pinned = Some(decks::by_id("discretion_advised").unwrap().identity);
    let mut first = seek(&url, "first", runner("first")).await;
    let (_, _, _, corp_deck, _) = joined_with_decks(next(&mut first).await);
    let first_view = state_update(next(&mut first).await);
    let mut second = seek(&url, "second", runner("second")).await;
    joined(next(&mut second).await);
    let second_view = state_update(next(&mut second).await);
    assert_eq!(corp_deck, "", "the bot's deck is not named either");
    assert_eq!((&first_view.corp.identity, &second_view.corp.identity), (&pinned, &pinned));

    let refuses = |corp_deck: &str| {
        let corp_deck = corp_deck.to_string();
        async move {
            match Server::bind("127.0.0.1:0", ServeOptions { corp_deck: Some(corp_deck), ..ServeOptions::default() }).await {
                Ok(_) => panic!("binding should have been refused"),
                Err(error) => error.to_string(),
            }
        }
    };
    let refused = refuses("not_a_deck").await;
    assert!(refused.contains("not_a_deck"), "{refused}");
    let wrong_side = refuses("stolen_goods").await;
    assert!(wrong_side.contains("not Corp"), "{wrong_side}");
}

#[tokio::test]
async fn a_bot_daemon_plays_the_deck_the_human_brought() {
    let url = start_server(bot_daemon()).await;
    let mut socket = seek(&url, "solo", Chair::Corp(brought("fine_print", "my_corp"))).await;
    let (_, side, _, corp_deck, _) = joined_with_decks(next(&mut socket).await);
    assert_eq!((side, corp_deck.as_str()), (Side::Corp, "my_corp"));
}

/// At the match cap a seek is refused, and the connection stays attached.
#[tokio::test]
async fn a_seek_is_refused_at_the_match_cap() {
    let url = start_server(ServeOptions { max_matches: Some(1), ..bot_daemon() }).await;
    let mut first = seek(&url, "first", corp("first")).await;
    joined(next(&mut first).await);

    let mut second = seek(&url, "second", corp("second")).await;
    let reason = seek_refused(next(&mut second).await);
    assert!(reason.contains("match limit"), "{reason}");
    send(&mut second, ClientMessage::ListLobbies).await;
    assert!(matches!(next(&mut second).await, ServerMessage::Lobbies { .. }), "still attached");
}

#[tokio::test]
async fn a_spectator_joins_a_running_match_by_id() {
    let url = start_server(bot_daemon()).await;

    let mut corp_seat = seek(&url, "corp", corp("corp")).await;
    let (match_id, _, _) = joined(next(&mut corp_seat).await);
    state_update(next(&mut corp_seat).await);
    let (matches, _) = list_matches(&url).await;
    assert_eq!(matches[0].match_id, match_id);

    let mut spectator = open(&url, ClientMessage::Spectate { match_id }).await;
    assert!(matches!(next(&mut spectator).await, ServerMessage::Spectating { match_id: seen } if seen == match_id));
    let view = state_update(next(&mut spectator).await);
    assert_eq!(view.viewer, Viewer::Spectator);
    assert_eq!(view.corp.hq_cards, None);
    assert!(view.legal_actions.is_empty());

    // A spectator holds no seat: what it sends is ignored, and does not
    // cost it the socket.
    send(&mut spectator, ClientMessage::Surrender).await;
    send(&mut corp_seat, ClientMessage::SubmitAction(PlayerAction::KeepHand)).await;
    state_update(next(&mut spectator).await);
    assert!(matches!(next(&mut spectator).await, ServerMessage::ActionLog(_)));
}

#[tokio::test]
async fn spectating_an_unknown_match_is_refused() {
    let url = human_daemon().await;
    let mut socket = open(&url, ClientMessage::Spectate { match_id: Uuid::new_v4() }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::ConnectRejected { .. }));
    assert!(closed_by_server(&mut socket).await);
}

/// The daemon writes the book after the session task ends, which is a
/// moment after the client saw `GameEnded`; poll for it.
async fn wait_for_book(path: &std::path::Path, ready: impl Fn(&RatingBook) -> bool) -> RatingBook {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(json) = std::fs::read_to_string(path)
                && let Ok(book) = RatingBook::from_json(&json)
                && ready(&book)
            {
                return book;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the rating book is written within 10s")
}

/// A game against a seated bot is practice wherever it is played: a bot
/// daemon given a rating file never writes to it. The match leaving
/// `MatchList` is the session task's exit, which is where a human match
/// is rated, so the file's absence after it is the claim.
#[tokio::test]
async fn a_game_against_a_seated_bot_is_rated_by_nobody() {
    let dir = std::env::temp_dir().join(format!("netrunner_ratings_bot_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ratings.json");
    let url = start_server(ServeOptions {
        bot_level: Some(netrunner_bots::Level::Operator),
        bot_personality: Some(Personality::Balanced),
        ratings_file: Some(path.clone()),
        ..bot_daemon()
    })
    .await;

    let mut quitter = seek(&url, "quitter", corp("quitter")).await;
    joined(next(&mut quitter).await);
    state_update(next(&mut quitter).await);
    assert_eq!(list_matches(&url).await.0[0].runner, "operator bot", "a rung is seated under its own name");
    send(&mut quitter, ClientMessage::Surrender).await;
    assert!(matches!(next(&mut quitter).await, ServerMessage::GameEnded { winner: Side::Runner, .. }));

    tokio::time::timeout(Duration::from_secs(10), async {
        while !list_matches(&url).await.0.is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the match ends within 10s");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!path.exists(), "a bot game wrote a rating book");
    let _ = std::fs::remove_dir_all(&dir);
}

/// With no personality pinned, the bot plays the style its dealt deck
/// names, and its seat says so — a rush Corp and a glacier Corp are
/// different opponents.
#[tokio::test]
async fn an_unpinned_bot_plays_its_dealt_decks_style_and_its_seat_says_so() {
    // The first match of a daemon seeded at 1 is match seed 1, so the deal
    // is `sample_decks_for_seed(1)` and the Runner deck's style is known.
    let dealt = netrunner_server::fixtures::sample_decks_for_seed(1);
    let runner_deck = decks::by_id(&dealt.runner_id).expect("the dealt deck is embedded");
    let style = runner_deck.style.clone().expect("every sample deck names a style");
    let url = start_server(bot_daemon()).await;

    let mut human = seek(&url, "human", corp("human")).await;
    joined(next(&mut human).await);
    assert_eq!(list_matches(&url).await.0[0].runner, format!("heuristic bot, {style}"));
}

/// Two humans, one surrenders; then the daemon is restarted on the same
/// file and a second match adds to the same standings.
#[tokio::test]
async fn human_matches_are_rated_on_their_own_track_and_the_book_survives_a_restart() {
    let dir = std::env::temp_dir().join(format!("netrunner_ratings_human_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ratings.json");
    let options = || ServeOptions { bot_runner: ServeBotKind::None, seed: Some(1), ratings_file: Some(path.clone()), ..ServeOptions::default() };

    for round in 1..=2u32 {
        let url = start_server(options()).await;
        let mut ann = seek(&url, "ann", corp("ann")).await;
        queued(next(&mut ann).await);
        let mut bo = seek(&url, "bo", runner("bo")).await;
        joined(next(&mut bo).await);
        joined(next(&mut ann).await);
        state_update(next(&mut ann).await);
        state_update(next(&mut bo).await);
        // The Corp's mulligan is awaited; the Runner concedes anyway — a
        // player may surrender at any moment, not only when asked.
        send(&mut bo, ClientMessage::Surrender).await;
        assert!(matches!(next(&mut ann).await, ServerMessage::GameEnded { winner: Side::Corp, .. }));
        let book = wait_for_book(&path, |book| {
            book.standing(Track::HumanVsHuman, "ann").is_some_and(|standing| standing.corp.wins == round)
        })
        .await;
        let bo_standing = book.standing(Track::HumanVsHuman, "bo").unwrap();
        assert_eq!(bo_standing.runner.losses, round);
        assert!(bo_standing.runner.rating.rating < 1500.0);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
