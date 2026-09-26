//! Proving a key over real WebSockets, and the ratings filed under it
//! (Phase 4 §5 stage b): `Identify` → `Challenge` → `Prove`, a proof that
//! is refused and why, the daemon's own key kept in its data directory,
//! and a rating that follows the key and never the name.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use netrunner_core::decks;
use netrunner_core::rules::Side;
use netrunner_identity::{Identity, PublicKey};
use netrunner_rating::{RatingBook, Track};
use netrunner_server::protocol::Chair;
use netrunner_server::serve::{ServeBotKind, ServeOptions, Server};
use netrunner_server::{ClientMessage, ServerMessage};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A daemon pairing people, and the key it proves itself by.
async fn start(data_dir: Option<PathBuf>) -> (String, PublicKey) {
    let options = ServeOptions { bot_runner: ServeBotKind::None, seed: Some(1), data_dir, ..ServeOptions::default() };
    let server = Server::bind("127.0.0.1:0", options).await.expect("an ephemeral port binds");
    let (addr, key) = (server.local_addr().unwrap(), server.public_key());
    tokio::spawn(server.run());
    (format!("ws://{addr}"), key)
}

/// A scratch directory of the test's own, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("netrunner_identity_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

async fn send(socket: &mut Socket, message: ClientMessage) {
    socket.send(WsMessage::Text(serde_json::to_string(&message).unwrap())).await.unwrap();
}

async fn next(socket: &mut Socket) -> ServerMessage {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(10), socket.next()).await.expect("the server answers within 10s");
        match frame {
            Some(Ok(WsMessage::Text(text))) => return serde_json::from_str(&text).expect("a ServerMessage"),
            Some(Ok(_)) => continue,
            other => panic!("socket ended: {other:?}"),
        }
    }
}

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

fn player(byte: u8) -> Identity {
    Identity::from_secret([byte; 32])
}

/// Identifies as `identity`, answering the challenge honestly, and
/// returns what the server said last.
async fn identify(url: &str, identity: &Identity) -> (Socket, ServerMessage) {
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    send(&mut socket, ClientMessage::Identify { key: identity.public_key() }).await;
    let ServerMessage::Challenge { nonce, server_key, .. } = next(&mut socket).await else { panic!("expected Challenge") };
    send(&mut socket, ClientMessage::Prove { signature: identity.prove(&server_key, &nonce) }).await;
    let answer = next(&mut socket).await;
    (socket, answer)
}

/// Attached as `name` (identified first when `identity` is given), in the
/// Startup lobby, and looking for a game in `chair`.
async fn seek(url: &str, identity: Option<&Identity>, name: &str, chair: Chair) -> Socket {
    let mut socket = match identity {
        Some(identity) => {
            let (socket, answer) = identify(url, identity).await;
            assert!(matches!(answer, ServerMessage::Identified { .. }), "{answer:?}");
            socket
        }
        None => tokio_tungstenite::connect_async(url).await.unwrap().0,
    };
    send(&mut socket, ClientMessage::Attach { player_name: name.into() }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::Attached { .. }));
    send(&mut socket, ClientMessage::JoinLobby { lobby: "startup".into(), password: None }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::LobbyJoined { .. }));
    send(&mut socket, ClientMessage::Seek { chair }).await;
    socket
}

fn corp() -> Chair {
    Chair::Corp(Box::new(decks::by_id("brick_stack").unwrap()))
}

fn runner() -> Chair {
    Chair::Runner(Box::new(decks::by_id("dashing_mad").unwrap()))
}

/// Two players seated, and the Runner concedes: a Corp win. Returns once
/// the Corp has seen it end.
async fn play_one(url: &str, corp_seat: (Option<&Identity>, &str), runner_seat: (Option<&Identity>, &str)) {
    let mut corp_socket = seek(url, corp_seat.0, corp_seat.1, corp()).await;
    assert!(matches!(next(&mut corp_socket).await, ServerMessage::Queued { .. }));
    let mut runner_socket = seek(url, runner_seat.0, runner_seat.1, runner()).await;
    assert!(matches!(next(&mut runner_socket).await, ServerMessage::MatchJoined { .. }));
    assert!(matches!(next(&mut corp_socket).await, ServerMessage::MatchJoined { .. }));
    send(&mut runner_socket, ClientMessage::Surrender).await;
    loop {
        if let ServerMessage::GameEnded { winner, .. } = next(&mut corp_socket).await {
            assert_eq!(winner, Side::Corp);
            break;
        }
    }
}

/// The daemon writes the book after the session task ends, a moment after
/// the players see `GameEnded`; poll for it.
async fn wait_for_book(path: &Path, ready: impl Fn(&RatingBook) -> bool) -> RatingBook {
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

/// A match that should go unrated has ended when the list is empty; a
/// moment later, the book is still not there.
async fn assert_nothing_rated(url: &str, dir: &Path) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
            send(&mut socket, ClientMessage::ListMatches).await;
            if let ServerMessage::MatchList { matches, .. } = next(&mut socket).await
                && matches.is_empty()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the match ends within 10s");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!dir.join("ratings.json").exists(), "an unrated game wrote a rating book");
}

#[tokio::test]
async fn an_honest_proof_is_accepted_and_the_challenge_names_the_server() {
    let (url, server_key) = start(None).await;
    let me = player(1);
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    send(&mut socket, ClientMessage::Identify { key: me.public_key() }).await;
    let ServerMessage::Challenge { nonce, server_key: named, lasting } = next(&mut socket).await else { panic!("expected Challenge") };
    assert_eq!(named, server_key, "the challenge names the key the server proves itself by");
    assert!(!lasting, "a daemon with no data directory makes a new key each run, and says so");
    send(&mut socket, ClientMessage::Prove { signature: me.prove(&named, &nonce) }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::Identified { key } if key == me.public_key()));
    // Proved, the connection goes on as any other.
    send(&mut socket, ClientMessage::Attach { player_name: "me".into() }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::Attached { .. }));
}

/// Each nonce is fresh: two connections are never asked to sign the same
/// thing, so a proof overheard on one is worth nothing on the next.
#[tokio::test]
async fn every_connection_is_asked_to_sign_a_new_nonce() {
    let (url, _) = start(None).await;
    let mut nonces = Vec::new();
    for _ in 0..2 {
        let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        send(&mut socket, ClientMessage::Identify { key: player(1).public_key() }).await;
        let ServerMessage::Challenge { nonce, .. } = next(&mut socket).await else { panic!("expected Challenge") };
        nonces.push(nonce);
    }
    assert_ne!(nonces[0], nonces[1]);
}

/// Three proofs that do not hold, each refused and the socket closed: a
/// key the signer does not hold, a proof made for another server — the
/// relay a hostile server would attempt — and a proof with no challenge.
#[tokio::test]
async fn a_proof_that_does_not_hold_is_refused_and_the_socket_closed() {
    let (url, _) = start(None).await;
    let (me, impostor) = (player(1), player(2));
    let another_server = player(3).public_key();

    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    send(&mut socket, ClientMessage::Identify { key: me.public_key() }).await;
    let ServerMessage::Challenge { nonce, server_key, .. } = next(&mut socket).await else { panic!("expected Challenge") };
    send(&mut socket, ClientMessage::Prove { signature: impostor.prove(&server_key, &nonce) }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::IdentifyRefused { .. }), "someone else's signature");
    assert!(closed_by_server(&mut socket).await);

    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    send(&mut socket, ClientMessage::Identify { key: me.public_key() }).await;
    let ServerMessage::Challenge { nonce, .. } = next(&mut socket).await else { panic!("expected Challenge") };
    send(&mut socket, ClientMessage::Prove { signature: me.prove(&another_server, &nonce) }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::IdentifyRefused { .. }), "a proof made for another server");
    assert!(closed_by_server(&mut socket).await);

    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    send(&mut socket, ClientMessage::Prove { signature: me.prove(&another_server, &netrunner_identity::Nonce([0; 32])) }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::IdentifyRefused { .. }), "a proof with no challenge");
    assert!(closed_by_server(&mut socket).await);
}

/// With a data directory, the daemon's key is made once, written for its
/// owner alone, and the same after a restart — which the challenge calls
/// `lasting`, so a client may remember it.
#[tokio::test]
async fn a_daemon_with_a_data_directory_keeps_its_key() {
    let dir = scratch("keeps");
    let (url, first) = start(Some(dir.clone())).await;
    let (_, second) = start(Some(dir.clone())).await;
    assert_eq!(first, second, "the same key after a restart");
    let file = dir.join("identity.key");
    assert!(file.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600, "the secret is its owner's alone");
    }
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    send(&mut socket, ClientMessage::Identify { key: player(1).public_key() }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::Challenge { lasting: true, .. }));

    // A key file the daemon cannot read is an error, never a reason to
    // make a new key behind every client's back.
    std::fs::write(&file, "not a key").unwrap();
    let options = ServeOptions { data_dir: Some(dir.clone()), ..ServeOptions::default() };
    assert!(Server::bind("127.0.0.1:0", options).await.is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two identified players, twice, the second time under other names on a
/// restarted daemon: the standings are filed under their keys and carry
/// over, and the players file names each key by the name it gave last.
#[tokio::test]
async fn a_rating_follows_the_key_and_never_the_name() {
    let dir = scratch("follows");
    let (ann, bo) = (player(1), player(2));
    let (ann_id, bo_id) = (ann.public_key().rating_id(), bo.public_key().rating_id());
    for (round, (ann_name, bo_name)) in [(1u32, ("ann", "bo")), (2, ("annie", "bo"))] {
        let (url, _) = start(Some(dir.clone())).await;
        play_one(&url, (Some(&ann), ann_name), (Some(&bo), bo_name)).await;
        let book = wait_for_book(&dir.join("ratings.json"), |book| book.standing(Track::HumanVsHuman, &ann_id).is_some_and(|standing| standing.corp.wins == round)).await;
        assert_eq!(book.standing(Track::HumanVsHuman, &bo_id).unwrap().runner.losses, round);
        assert!(book.standing(Track::HumanVsHuman, ann_name).is_none(), "nothing is filed under a name");
    }
    let players: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("players.json")).unwrap()).unwrap();
    assert_eq!(players[&ann_id]["name"], "annie", "the name a key gave last");
    assert_eq!(players[&bo_id]["name"], "bo");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_game_with_an_unidentified_seat_is_rated_by_nobody() {
    let dir = scratch("unidentified");
    let (url, _) = start(Some(dir.clone())).await;
    play_one(&url, (Some(&player(1)), "ann"), (None, "bo")).await;
    assert_nothing_rated(&url, &dir).await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// One key in both chairs is someone playing themselves, which would farm
/// one role's rating off the other's.
#[tokio::test]
async fn a_game_between_one_key_and_itself_is_rated_by_nobody() {
    let dir = scratch("itself");
    let (url, _) = start(Some(dir.clone())).await;
    let me = player(1);
    play_one(&url, (Some(&me), "me"), (Some(&me), "also me")).await;
    assert_nothing_rated(&url, &dir).await;
    let _ = std::fs::remove_dir_all(&dir);
}
