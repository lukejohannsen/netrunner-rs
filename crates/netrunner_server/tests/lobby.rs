//! The lobby and the match registry over real WebSockets: queueing,
//! pairing, ghost sweeping, resuming a queue place, rooms, the match cap,
//! the seed policy and `ListMatches`. `tests/reconnect.rs` covers a seat's
//! lifetime after `MatchJoined`; this file covers everything before it,
//! and more than one match at a time.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use uuid::Uuid;

use netrunner_core::rules::Side;
use netrunner_core::view::ClientView;
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

fn connect(name: &str, preferred_side: Option<Side>) -> ClientMessage {
    ClientMessage::Connect { player_name: name.into(), preferred_side, room: None }
}

fn connect_in_room(name: &str, room: &str) -> ClientMessage {
    ClientMessage::Connect { player_name: name.into(), preferred_side: None, room: Some(room.into()) }
}

fn joined(message: ServerMessage) -> (Uuid, Side, Uuid) {
    match message {
        ServerMessage::MatchJoined { match_id, assigned_side, session_token } => (match_id, assigned_side, session_token),
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

    let mut first = open(&url, connect("first", None)).await;
    let (_, position) = queued(next(&mut first).await);
    assert_eq!(position, 1);

    let mut second = open(&url, connect("second", Some(Side::Runner))).await;
    let (second_match, second_side, _) = joined(next(&mut second).await);
    let (first_match, first_side, _) = joined(next(&mut first).await);

    assert_eq!(first_match, second_match, "one match for both");
    assert_eq!((first_side, second_side), (Side::Corp, Side::Runner), "the second player's preference decided");
    state_update(next(&mut first).await);
    state_update(next(&mut second).await);

    let (matches, waiting) = list_matches(&url).await;
    assert_eq!(waiting, 0);
    assert_eq!(matches.len(), 1);
    assert_eq!((matches[0].corp.as_str(), matches[0].runner.as_str()), ("first", "second"));
}

#[tokio::test]
async fn a_waiting_player_whose_socket_closed_is_never_paired() {
    let url = human_daemon().await;

    let mut ghost = open(&url, connect("ghost", None)).await;
    queued(next(&mut ghost).await);
    ghost.close(None).await.unwrap();
    drop(ghost);
    wait_until_lobby_holds(&url, 0).await;

    let mut second = open(&url, connect("second", None)).await;
    let (_, position) = queued(next(&mut second).await);
    assert_eq!(position, 1, "the ghost is gone, not ahead in the queue");

    let mut third = open(&url, connect("third", None)).await;
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
        let mut a = open(&url, connect(&format!("a{pair}"), Some(Side::Corp))).await;
        queued(next(&mut a).await);
        let mut b = open(&url, connect(&format!("b{pair}"), None)).await;
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

#[tokio::test]
async fn a_queued_player_resumes_its_place_with_the_token() {
    let url = human_daemon().await;

    let mut first = open(&url, connect("first", Some(Side::Corp))).await;
    let (token, _) = queued(next(&mut first).await);
    first.close(None).await.unwrap();
    drop(first);

    let mut back = open(&url, ClientMessage::Resume { session_token: token }).await;
    let (resumed_token, position) = queued(next(&mut back).await);
    assert_eq!((resumed_token, position), (token, 1), "the same place, the same credential");

    let mut second = open(&url, connect("second", None)).await;
    joined(next(&mut second).await);
    let (_, side, seat_token) = joined(next(&mut back).await);
    assert_eq!(side, Side::Corp, "the resumed entry kept its preference");
    assert_eq!(seat_token, token, "one token from the queue to the seat");
    state_update(next(&mut back).await);
}

#[tokio::test]
async fn the_seed_policy_is_deterministic_and_per_match() {
    let options = || ServeOptions { bot_runner: ServeBotKind::Heuristic, seed: Some(1), ..ServeOptions::default() };
    let url_a = start_server(options()).await;
    let url_b = start_server(options()).await;

    let mut a1 = open(&url_a, connect("a1", Some(Side::Corp))).await;
    joined(next(&mut a1).await);
    let opening_a1 = state_update(next(&mut a1).await);

    let mut b1 = open(&url_b, connect("b1", Some(Side::Corp))).await;
    joined(next(&mut b1).await);
    let opening_b1 = state_update(next(&mut b1).await);
    assert_eq!(opening_a1, opening_b1, "two daemons on the same --seed deal the same first match");

    let mut a2 = open(&url_a, connect("a2", Some(Side::Corp))).await;
    joined(next(&mut a2).await);
    let opening_a2 = state_update(next(&mut a2).await);
    assert_ne!(opening_a1.corp.hq_cards, opening_a2.corp.hq_cards, "the second match on a daemon is a different deal");
}

#[tokio::test]
async fn connect_is_refused_at_the_match_cap() {
    let url = start_server(ServeOptions {
        bot_runner: ServeBotKind::Heuristic,
        seed: Some(1),
        max_matches: Some(1),
        ..ServeOptions::default()
    })
    .await;

    let mut first = open(&url, connect("first", Some(Side::Corp))).await;
    joined(next(&mut first).await);

    let mut second = open(&url, connect("second", Some(Side::Corp))).await;
    assert!(matches!(next(&mut second).await, ServerMessage::ConnectRejected { .. }));
    assert!(closed_by_server(&mut second).await, "a refused connection is closed, not left waiting");
}

#[tokio::test]
async fn rooms_only_pair_within_themselves() {
    let url = human_daemon().await;

    let mut alice = open(&url, connect_in_room("alice", "friends")).await;
    queued(next(&mut alice).await);
    let mut stranger = open(&url, connect("stranger", None)).await;
    let (_, position) = queued(next(&mut stranger).await);
    assert_eq!(position, 2, "queued behind the room's waiter, not paired with them");

    let mut bob = open(&url, connect_in_room("bob", "friends")).await;
    let (bob_match, _, _) = joined(next(&mut bob).await);
    let (alice_match, _, _) = joined(next(&mut alice).await);
    assert_eq!(alice_match, bob_match);

    let (_, waiting) = list_matches(&url).await;
    assert_eq!(waiting, 1, "the public-queue player is still waiting");
    let _ = stranger.close(None).await;
}
