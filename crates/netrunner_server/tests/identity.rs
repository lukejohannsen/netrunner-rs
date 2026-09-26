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
use netrunner_identity::{Identity, PublicKey, Signed};
use netrunner_rating::{RatingBook, Track};
use netrunner_server::protocol::statements::{Receipt, SeatStatement, SEAT_TAG};
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

/// The next message `wanted` picks out, skipping the game's traffic.
async fn next_where<T>(socket: &mut Socket, wanted: impl Fn(ServerMessage) -> Option<T>) -> T {
    loop {
        if let Some(found) = wanted(next(socket).await) {
            return found;
        }
    }
}

/// Answers the `SignSeat` that follows `MatchJoined` — or, when `sign` is
/// false, reads it and says nothing. Returns the statement.
async fn answer_sign_seat(socket: &mut Socket, identity: &Identity, sign: bool) -> SeatStatement {
    let (statement, _salt) = next_where(socket, |message| match message {
        ServerMessage::SignSeat { statement, salt } => Some((statement, salt)),
        _ => None,
    })
    .await;
    if sign {
        let signature = identity.sign(SEAT_TAG, statement.clone()).signature;
        send(socket, ClientMessage::SeatSigned { signature }).await;
    }
    serde_json::from_str(&statement).unwrap()
}

/// Two proved players, each seat asked to sign; the Runner concedes. Both
/// seats' receipts come back in `Rated`. `runner_signs` false withholds
/// the Runner's signature.
async fn play_rated(url: &str, ann: &Identity, bo: &Identity, runner_signs: bool) -> [ServerMessage; 2] {
    let mut corp_socket = seek(url, Some(ann), "ann", corp()).await;
    assert!(matches!(next(&mut corp_socket).await, ServerMessage::Queued { .. }));
    let mut runner_socket = seek(url, Some(bo), "bo", runner()).await;
    assert!(matches!(next(&mut runner_socket).await, ServerMessage::MatchJoined { .. }));
    let said = answer_sign_seat(&mut runner_socket, bo, runner_signs).await;
    assert_eq!((said.side, said.key, said.opponent_key), (Side::Runner, bo.public_key(), Some(ann.public_key())));
    assert!(matches!(next(&mut corp_socket).await, ServerMessage::MatchJoined { .. }));
    answer_sign_seat(&mut corp_socket, ann, true).await;
    // The signatures reach the server before the game can end: the next
    // message on each socket is the first view, which the server sent
    // after reading nothing — so wait for a view that follows an action.
    send(&mut corp_socket, ClientMessage::SubmitAction(netrunner_core::rules::PlayerAction::KeepHand)).await;
    next_where(&mut runner_socket, |message| matches!(message, ServerMessage::ActionLog(_)).then_some(())).await;
    send(&mut runner_socket, ClientMessage::Surrender).await;
    let rated = |message: ServerMessage| matches!(message, ServerMessage::Rated { .. }).then_some(message);
    [next_where(&mut corp_socket, rated).await, next_where(&mut runner_socket, rated).await]
}

/// A rated game ends in a receipt the server signed: it names both keys,
/// carries each seat's signed commitment, the record's hash, and the
/// result, and each seat is told its rating before and after.
#[tokio::test]
async fn a_rated_game_ends_in_a_signed_receipt_both_players_get() {
    let dir = scratch("receipt");
    let (url, server_key) = start(Some(dir.clone())).await;
    let (ann, bo) = (player(1), player(2));
    let [corp_rated, runner_rated] = play_rated(&url, &ann, &bo, true).await;
    let ServerMessage::Rated { receipt, before, after } = corp_rated else { unreachable!() };
    assert!(after.rating > before.rating, "the Corp won: {before:?} -> {after:?}");
    let ServerMessage::Rated { receipt: runner_receipt, before: runner_before, after: runner_after } = runner_rated else { unreachable!() };
    assert!(runner_after.rating < runner_before.rating);
    assert_eq!(receipt, runner_receipt, "one receipt, both players");

    let read = Receipt::read(&receipt, &server_key).expect("signed by the server, under the receipt tag");
    assert!(read.rated);
    assert_eq!(read.winner, Some(Side::Corp));
    assert_eq!((read.corp.key, read.runner.key), (Some(ann.public_key()), Some(bo.public_key())));
    for (seat, key) in [(&read.corp, ann.public_key()), (&read.runner, bo.public_key())] {
        let commitment = seat.commitment.as_ref().expect("each seat signed");
        assert_eq!(commitment.key, key);
        let statement: SeatStatement = serde_json::from_str(commitment.verify(SEAT_TAG).unwrap()).unwrap();
        assert_eq!(statement.match_id, read.match_id);
    }
    assert!(Receipt::read(&receipt, &player(9).public_key()).is_err(), "a receipt is only another server's if that server signed it");

    // The record it names is the one kept, byte for byte, with the
    // receipt beside it; and the results log holds the receipt.
    let months = std::fs::read_dir(dir.join("matches")).unwrap().flatten().map(|month| month.path()).collect::<Vec<_>>();
    let record = months.iter().map(|month| month.join(format!("{}.jsonl", read.match_id))).find(|path| path.exists()).unwrap();
    assert_eq!(netrunner_identity::sha256_hex(&std::fs::read(&record).unwrap()), read.record);
    let beside: Signed = serde_json::from_str(&std::fs::read_to_string(record.with_extension("receipt.json")).unwrap()).unwrap();
    assert_eq!(beside, *receipt);
    let results = std::fs::read_to_string(dir.join("results.jsonl")).unwrap();
    assert_eq!(results.lines().count(), 1);

    // Asked afterwards, the server reports the proved key's standing, and
    // nothing for a connection that proved none.
    let (mut socket, _) = identify(&url, &ann).await;
    send(&mut socket, ClientMessage::MyStanding).await;
    let ServerMessage::Standing { key, standing } = next(&mut socket).await else { panic!("expected Standing") };
    assert_eq!(key, Some(ann.public_key()));
    assert_eq!(standing.expect("a rated game").corp.wins, 1);
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    send(&mut socket, ClientMessage::MyStanding).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::Standing { key: None, standing: None }));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Withholding the signature does not make a lost game count for nothing:
/// it is rated, and its receipt lacks that seat's commitment.
#[tokio::test]
async fn a_withheld_seat_signature_is_still_rated_and_the_receipt_says_so() {
    let dir = scratch("withheld");
    let (url, server_key) = start(Some(dir.clone())).await;
    let [corp_rated, _] = play_rated(&url, &player(1), &player(2), false).await;
    let ServerMessage::Rated { receipt, .. } = corp_rated else { unreachable!() };
    let read = Receipt::read(&receipt, &server_key).unwrap();
    assert!(read.rated);
    assert!(read.corp.commitment.is_some());
    assert!(read.runner.commitment.is_none(), "the Runner never signed");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The book is a cache of the results log: removed, it is rebuilt exactly;
/// and a line nobody signed stops the rebuild rather than count.
#[tokio::test]
async fn the_rating_book_is_rebuilt_exactly_from_the_results_log() {
    let dir = scratch("rebuild");
    let (url, _) = start(Some(dir.clone())).await;
    play_rated(&url, &player(1), &player(2), true).await;
    play_rated(&url, &player(1), &player(2), true).await;
    let ratings = dir.join("ratings.json");
    let book = std::fs::read_to_string(&ratings).unwrap();
    std::fs::remove_file(&ratings).unwrap();
    assert_eq!(netrunner_server::serve::rebuild_ratings(&dir).unwrap(), 2);
    assert_eq!(std::fs::read_to_string(&ratings).unwrap(), book, "the same book, byte for byte");

    let results = dir.join("results.jsonl");
    let forged = std::fs::read_to_string(&results).unwrap().replacen("\"rated\\\":true", "\"rated\\\":false", 1);
    assert_ne!(forged, std::fs::read_to_string(&results).unwrap(), "the test edited a line");
    std::fs::write(&results, forged).unwrap();
    let error = netrunner_server::serve::rebuild_ratings(&dir).unwrap_err().to_string();
    assert!(error.contains("line 1"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}
