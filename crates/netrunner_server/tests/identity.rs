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
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;
use netrunner_identity::{Identity, PublicKey, Signed};
use netrunner_store::scratch::PostgresSchema;
use netrunner_store::{RatingRow, Store};
use netrunner_server::protocol::statements::{Receipt, SeatStatement, SEAT_TAG};
use netrunner_server::protocol::{Chair, Opponent, QueueMode};
use netrunner_server::serve::{ServeOptions, Server};
use netrunner_server::{ClientMessage, ServerMessage};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A daemon pairing people, and the key it proves itself by.
async fn start(data_dir: Option<PathBuf>) -> (String, PublicKey) {
    start_keeping(data_dir, None).await
}

/// A daemon pairing people over `--database` when it is given one.
async fn start_keeping(data_dir: Option<PathBuf>, database: Option<String>) -> (String, PublicKey) {
    let options = ServeOptions { seed: Some(1), data_dir, database, ..ServeOptions::default() };
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
/// Ranked Startup queue, and looking for a game in `chair`.
async fn seek(url: &str, identity: Option<&Identity>, name: &str, chair: Chair) -> Socket {
    seek_in(url, identity, name, "ranked-startup", chair).await
}

/// `seek`, in lobby `lobby` — one of the server's queues or a code.
async fn seek_in(url: &str, identity: Option<&Identity>, name: &str, lobby: &str, chair: Chair) -> Socket {
    let mut socket = attach(url, identity, name).await;
    send(&mut socket, ClientMessage::JoinLobby { lobby: lobby.into(), password: None }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::LobbyJoined { .. }));
    send(&mut socket, ClientMessage::Seek { chair, opponent: Opponent::Person }).await;
    socket
}

/// Attached as `name`, proving `identity` first when there is one.
async fn attach(url: &str, identity: Option<&Identity>, name: &str) -> Socket {
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
            assert_eq!(winner, Some(Side::Corp));
            break;
        }
    }
}

/// The store the daemon in `dir` writes, opened beside it (SQLite takes
/// a second reader; the daemon's writer is the only writer).
async fn store_in(dir: &Path) -> Store {
    Store::open(&netrunner_store::sqlite_url(&dir.join(netrunner_store::SQLITE_FILE))).await.expect("the daemon's store opens")
}

/// Where a daemon in a test keeps what it keeps: its data directory —
/// its key and the results log — and its store, the SQLite file in that
/// directory or a scratch Postgres schema given as `--database` (Phase 4
/// §10 stage 3).
struct Keeping {
    dir: PathBuf,
    postgres: Option<PostgresSchema>,
}

impl Keeping {
    fn database(&self) -> Option<&str> {
        self.postgres.as_ref().map(|schema| schema.url.as_str())
    }

    async fn start(&self) -> (String, PublicKey) {
        start_keeping(Some(self.dir.clone()), self.database().map(str::to_string)).await
    }

    /// The daemon's store, opened beside it (the daemon's writer is the
    /// only writer).
    async fn store(&self) -> Store {
        match self.database() {
            Some(url) => Store::open(url).await.expect("the daemon's store opens"),
            None => store_in(&self.dir).await,
        }
    }

    async fn remove(self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        if let Some(schema) = self.postgres {
            schema.remove().await;
        }
    }
}

/// One keeping per database a test can reach: SQLite always, and
/// Postgres when `NETRUNNER_TEST_POSTGRES` names a server.
async fn every_keeping(name: &str) -> Vec<Keeping> {
    let mut keepings = vec![Keeping { dir: scratch(name), postgres: None }];
    if let Some(schema) = netrunner_store::scratch::postgres(&format!("server_{name}")).await {
        keepings.push(Keeping { dir: scratch(&format!("{name}_postgres")), postgres: Some(schema) });
    }
    keepings
}

/// The daemon's writer lands the rows after the session task ends, a
/// moment after the players see `GameEnded`; poll for them.
async fn wait_for_rows(store: &Store, ready: impl Fn(&[RatingRow]) -> bool) -> Vec<RatingRow> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(rows) = store.ratings().await
                && ready(&rows)
            {
                return rows;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the rating rows are written within 10s")
}

/// `key`'s row in `rows` for `format` and `role`.
fn row<'a>(rows: &'a [RatingRow], key: &str, format: &str, role: &str) -> Option<&'a RatingRow> {
    rows.iter().find(|row| row.key == key && row.format == format && row.role == role)
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
    let store = store_in(dir).await;
    assert!(store.ratings().await.unwrap().is_empty(), "an unrated game wrote a rating");
    assert_eq!(store.result_count().await.unwrap(), 0, "an unrated game is not in the results index");
    store.close().await;
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
    let (ann, bo) = (player(1), player(2));
    let (ann_id, bo_id) = (ann.public_key().rating_id(), bo.public_key().rating_id());
    for keeping in every_keeping("follows").await {
        for (round, (ann_name, bo_name)) in [(1u32, ("ann", "bo")), (2, ("annie", "bo"))] {
            let (url, _) = keeping.start().await;
            play_one(&url, (Some(&ann), ann_name), (Some(&bo), bo_name)).await;
            // The games were in the Startup lobby, so the Startup ladder is
            // the one that moved (Phase 4 §10: a ladder per format).
            let store = keeping.store().await;
            let rows = wait_for_rows(&store, |rows| row(rows, &ann_id, "startup", "corp").is_some_and(|row| row.wins == i64::from(round))).await;
            assert_eq!(row(&rows, &bo_id, "startup", "runner").unwrap().losses, i64::from(round));
            assert!(rows.iter().all(|row| row.format == "startup"), "no other ladder moved: {rows:?}");
            assert!(rows.iter().all(|row| row.key != ann_name), "nothing is filed under a name");
            store.close().await;
        }
        let store = keeping.store().await;
        assert_eq!(store.player(&ann_id).await.unwrap().unwrap().name, "annie", "the name a key gave last");
        assert_eq!(store.player(&bo_id).await.unwrap().unwrap().name, "bo");
        let pairing = store.last_pairing(&ann_id, &bo_id).await.unwrap().expect("the pair is remembered");
        assert_eq!((pairing.corp.as_str(), pairing.runner.as_str()), (ann_id.as_str(), bo_id.as_str()), "which chairs they took");
        store.close().await;
        keeping.remove().await;
    }
}

#[tokio::test]
async fn a_game_with_an_unidentified_seat_is_rated_by_nobody() {
    let dir = scratch("unidentified");
    let (url, _) = start(Some(dir.clone())).await;
    play_one(&url, (Some(&player(1)), "ann"), (None, "bo")).await;
    assert_nothing_rated(&url, &dir).await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A lobby says whether its games count, and a friend queue on a daemon
/// that keeps a book rates nobody, two proved keys or not — while the
/// daemon's Ranked queues beside it still do (Phase 4 §7 stage 5, §9).
#[tokio::test]
async fn a_friend_queue_between_two_keys_is_rated_by_nobody() {
    let dir = scratch("casual");
    let (url, _) = start(Some(dir.clone())).await;
    let (ann, bo) = (player(1), player(2));
    let mut maker = attach(&url, Some(&ann), "ann").await;
    send(&mut maker, ClientMessage::ListLobbies).await;
    let ServerMessage::Lobbies { lobbies } = next(&mut maker).await else { panic!() };
    assert!(lobbies.iter().any(|lobby| lobby.mode == QueueMode::Ranked), "a daemon with a book has Ranked queues: {lobbies:?}");
    assert!(lobbies.iter().all(|lobby| lobby.rated == (lobby.mode == QueueMode::Ranked) && (lobby.mode != QueueMode::Ranked || lobby.bots.is_empty())), "only Ranked rates, and it offers no bot: {lobbies:?}");
    send(&mut maker, ClientMessage::CreateLobby { name: "just trying".into(), format: NsgFormat::Startup, password: None, bots: false }).await;
    let ServerMessage::LobbyJoined { lobby } = next(&mut maker).await else { panic!() };
    assert!(!lobby.rated, "{lobby:?}");
    // The maker is in the lobby already (and leaving would empty it, which
    // is the end of a friend queue): seek from that socket.
    let mut corp_socket = maker;
    send(&mut corp_socket, ClientMessage::Seek { chair: corp(), opponent: Opponent::Person }).await;
    assert!(matches!(next(&mut corp_socket).await, ServerMessage::Queued { .. }));
    let mut runner_socket = seek_in(&url, Some(&bo), "bo", &lobby.id, runner()).await;
    assert!(matches!(next(&mut runner_socket).await, ServerMessage::MatchJoined { .. }));
    assert!(matches!(next(&mut corp_socket).await, ServerMessage::MatchJoined { .. }));
    // An unrated lobby takes an undo back at one press (Phase 4 §5 stage
    // e): the Corp's click for a credit is over the moment it is taken,
    // so going back is past the free line — and the host offers it,
    // takes it, and tells both seats.
    use netrunner_core::rules::PlayerAction;
    send(&mut corp_socket, ClientMessage::SubmitAction(PlayerAction::KeepHand)).await;
    next_where(&mut runner_socket, |message| matches!(message, ServerMessage::ActionLog(_)).then_some(())).await;
    send(&mut runner_socket, ClientMessage::SubmitAction(PlayerAction::KeepHand)).await;
    // Through the turn's start — each seat's one legal action at a time,
    // the window's passes — to the Corp's action phase.
    let click = PlayerAction::GainCreditClick { side: Side::Corp };
    loop {
        let (seat, message) = tokio::select! {
            message = next(&mut corp_socket) => (Side::Corp, message),
            message = next(&mut runner_socket) => (Side::Runner, message),
        };
        let ServerMessage::StateUpdate(view) = message else { continue };
        if seat == Side::Corp && view.legal_actions.contains(&click) {
            break;
        }
        if let [only] = &view.legal_actions[..] {
            let socket = if seat == Side::Corp { &mut corp_socket } else { &mut runner_socket };
            send(socket, ClientMessage::SubmitAction(only.clone())).await;
        }
    }
    send(&mut corp_socket, ClientMessage::SubmitAction(click)).await;
    let offered = next_where(&mut corp_socket, |message| match message {
        ServerMessage::Back { rewind } => Some(rewind),
        _ => None,
    })
    .await;
    assert_eq!(offered, Some(netrunner_protocol::Rewind::Undo), "a lobby that does not rate offers the undo too");
    send(&mut corp_socket, ClientMessage::TakeBack).await;
    for socket in [&mut corp_socket, &mut runner_socket] {
        let (by, removed) = next_where(socket, |message| match message {
            ServerMessage::TakenBack { by, removed, .. } => Some((by, removed)),
            _ => None,
        })
        .await;
        assert_eq!((by, removed), (Side::Corp, 1));
    }
    send(&mut runner_socket, ClientMessage::Surrender).await;
    // Back in the lobby with no `Rated` between the end and the return.
    let after_end = next_where(&mut corp_socket, |message| match message {
        ServerMessage::GameEnded { .. } => Some(None),
        ServerMessage::Rated { .. } => Some(Some("rated")),
        _ => None,
    })
    .await;
    assert_eq!(after_end, None);
    let next_message = next(&mut corp_socket).await;
    assert!(matches!(next_message, ServerMessage::BackInLobby { .. }), "a casual game ends in no receipt for the players: {next_message:?}");
    assert_nothing_rated(&url, &dir).await;
    let _ = std::fs::remove_dir_all(&dir);
}

fn either() -> Chair {
    Chair::Either { corp: Box::new(decks::by_id("brick_stack").unwrap()), runner: Box::new(decks::by_id("dashing_mad").unwrap()), prefer: None }
}

/// Both sockets, already in one lobby, look for a game in either chair —
/// `first` before `second` — and the Runner concedes it. Returns the side
/// `first` played, once both are back in the lobby.
async fn play_either(first: &mut Socket, second: &mut Socket) -> Side {
    send(first, ClientMessage::Seek { chair: either(), opponent: Opponent::Person }).await;
    assert!(matches!(next(first).await, ServerMessage::Queued { position: 1, .. }));
    send(second, ClientMessage::Seek { chair: either(), opponent: Opponent::Person }).await;
    let ServerMessage::MatchJoined { assigned_side: second_side, .. } = next(second).await else { panic!("expected MatchJoined") };
    let ServerMessage::MatchJoined { assigned_side: first_side, .. } = next(first).await else { panic!("expected MatchJoined") };
    assert_ne!(first_side, second_side);
    let runner = if first_side == Side::Runner { &mut *first } else { &mut *second };
    send(runner, ClientMessage::Surrender).await;
    for socket in [first, second] {
        next_where(socket, |message| matches!(message, ServerMessage::BackInLobby { .. }).then_some(())).await;
    }
    first_side
}

/// A friend queue `ann` makes on `url`, which `bo` joins.
async fn friend_queue(url: &str, ann: &Identity, bo: &Identity) -> (Socket, Socket) {
    let mut ann_socket = attach(url, Some(ann), "ann").await;
    send(&mut ann_socket, ClientMessage::CreateLobby { name: "rematch".into(), format: NsgFormat::Startup, password: None, bots: false }).await;
    let ServerMessage::LobbyJoined { lobby } = next(&mut ann_socket).await else { panic!() };
    let mut bo_socket = attach(url, Some(bo), "bo").await;
    send(&mut bo_socket, ClientMessage::JoinLobby { lobby: lobby.id, password: None }).await;
    assert!(matches!(next(&mut bo_socket).await, ServerMessage::LobbyJoined { .. }));
    (ann_socket, bo_socket)
}

/// The person's balance rule (Phase 4 §9): two keys who meet again, both
/// happy with either chair, swap the chairs they last took — game after
/// game in a friend queue of two, whichever of them looks first, and
/// across a restart, since the store keeps the last pairing.
#[tokio::test]
async fn two_keys_who_meet_again_swap_chairs() {
    let dir = scratch("swap");
    let (url, _) = start(Some(dir.clone())).await;
    let (ann, bo) = (player(1), player(2));
    let (mut ann_socket, mut bo_socket) = friend_queue(&url, &ann, &bo).await;
    let mut ann_sides = Vec::new();
    for game in 0..4 {
        let side = if game % 2 == 0 { play_either(&mut ann_socket, &mut bo_socket).await } else { play_either(&mut bo_socket, &mut ann_socket).await.other() };
        ann_sides.push(side);
    }
    assert!(ann_sides.windows(2).all(|pair| pair[0] != pair[1]), "Ann's chairs: {ann_sides:?}");

    // The store has the last game's chairs, and a daemon started on the
    // same directory reads them.
    let store = store_in(&dir).await;
    let (ann_id, bo_id) = (ann.public_key().rating_id(), bo.public_key().rating_id());
    let last_corp = if ann_sides[3] == Side::Corp { &ann_id } else { &bo_id };
    tokio::time::timeout(Duration::from_secs(10), async {
        while store.last_pairing(&ann_id, &bo_id).await.unwrap().is_none_or(|pairing| &pairing.corp != last_corp) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the pairing is stored");
    store.close().await;
    let (url, _) = start(Some(dir.clone())).await;
    let (mut ann_socket, mut bo_socket) = friend_queue(&url, &ann, &bo).await;
    assert_eq!(play_either(&mut ann_socket, &mut bo_socket).await, ann_sides[3].other(), "the swap outlives the daemon");
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
    send(&mut socket, ClientMessage::MyProfile).await;
    let ServerMessage::Profile { key, profile } = next(&mut socket).await else { panic!("expected Profile") };
    assert_eq!(key, Some(ann.public_key()));
    let profile = profile.expect("the key attached, so the store keeps it");
    assert_eq!(profile.name, "ann");
    assert_eq!(profile.standings.len(), 1, "one ladder played on: {:?}", profile.standings);
    assert_eq!(profile.standings[0].format, NsgFormat::Startup, "the lobby's format is the ladder");
    assert_eq!(read.format, NsgFormat::Startup, "and the receipt names it");
    assert_eq!(profile.standings[0].standing.corp.wins, 1);
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    send(&mut socket, ClientMessage::MyProfile).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::Profile { key: None, profile: None }));
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

/// A profile is the key's to read and, in part, to set: a title granted
/// in the store is worn once chosen, an unearned one is refused, and the
/// ladder lists a key with ten rated games and not one with nine
/// (Phase 4 §10 stage 2).
#[tokio::test]
async fn a_title_earned_is_worn_an_unearned_one_refused_and_the_ladder_lists_ten_games() {
    for keeping in every_keeping("profile").await {
        use netrunner_server::protocol::{LadderRow, LADDER_MIN_GAMES};
        std::fs::create_dir_all(&keeping.dir).unwrap();
        let (ann, cy, dee) = (player(1), player(3), player(4));
        let (ann_id, cy_id, dee_id) = (ann.public_key().rating_id(), cy.public_key().rating_id(), dee.public_key().rating_id());
        // Seeded before the daemon starts, since the book is loaded from the
        // store at bind: Cy with ten rated Standard games, Dee with nine.
        let seed = keeping.store().await;
        seed.saw_player(&ann_id, "ann", 1).await.unwrap();
        seed.grant(&netrunner_store::Award { key: ann_id.clone(), kind: "title".into(), name: "Gauntlet Runner".into(), earned_at: 2, source: "event:abc".into() }).await.unwrap();
        for (id, name, corp_games, runner_games) in [(&cy_id, "cy", 6, 4), (&dee_id, "dee", 5, 4)] {
            seed.saw_player(id, name, 1).await.unwrap();
            for (role, games) in [("corp", corp_games), ("runner", runner_games)] {
                let row = RatingRow { key: id.clone(), format: "standard".into(), role: role.into(), rating: 1600.0, deviation: 120.0, volatility: 0.06, wins: games, draws: 0, losses: 0 };
                seed.put_rating(&row).await.unwrap();
            }
        }
        seed.close().await;
        let (url, _) = keeping.start().await;

        let mut socket = attach(&url, Some(&ann), "annie").await;
        send(&mut socket, ClientMessage::SetProfile { title: Some("Gauntlet Runner".into()), badge: None }).await;
        let ServerMessage::Profile { key, profile } = next(&mut socket).await else { panic!("expected Profile") };
        let profile = profile.expect("kept");
        assert_eq!((key, profile.name.as_str(), profile.title.as_deref()), (Some(ann.public_key()), "annie", Some("Gauntlet Runner")), "worn, under the name given last");
        assert_eq!(profile.titles.len(), 1);
        send(&mut socket, ClientMessage::SetProfile { title: None, badge: Some("first-gauntlet".into()) }).await;
        let ServerMessage::ProfileRefused { reason } = next(&mut socket).await else { panic!("expected ProfileRefused") };
        assert!(reason.contains("first-gauntlet"), "{reason}");
        send(&mut socket, ClientMessage::SetProfile { title: None, badge: None }).await;
        let ServerMessage::Profile { profile, .. } = next(&mut socket).await else { panic!("expected Profile") };
        assert_eq!(profile.unwrap().title, None, "nothing worn again");

        // The ladder, on a bare socket: anyone's to read.
        let (mut bare, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        send(&mut bare, ClientMessage::Ladder { format: NsgFormat::Standard }).await;
        let ServerMessage::Ladder { format, rows } = next(&mut bare).await else { panic!("expected Ladder") };
        assert_eq!(format, NsgFormat::Standard);
        let listed: Vec<(PublicKey, String)> = rows.iter().map(|LadderRow { key, name, .. }| (*key, name.clone())).collect();
        assert_eq!(listed, vec![(cy.public_key(), "cy".to_string())], "ten games listed, nine not ({LADDER_MIN_GAMES} needed): {rows:?}");
        assert_eq!(rows[0].standing.corp.wins, 6);
        keeping.remove().await;
    }
}

/// The store is an index of the results log: emptied, it is rebuilt
/// exactly; and a line nobody signed stops the rebuild rather than count.
#[tokio::test]
async fn the_store_is_rebuilt_exactly_from_the_results_log() {
    for keeping in every_keeping("rebuild").await {
        let (url, _) = keeping.start().await;
        let ann = player(1).public_key().rating_id();
        play_rated(&url, &player(1), &player(2), true).await;
        play_rated(&url, &player(1), &player(2), true).await;
        let store = keeping.store().await;
        let rows = wait_for_rows(&store, |rows| row(rows, &ann, "startup", "corp").is_some_and(|row| row.wins == 2)).await;
        let results = store.results_of(&ann).await.unwrap();
        assert_eq!(results.len(), 2);
        store.replace_rebuilt(&[], &[]).await.unwrap();
        assert_eq!(netrunner_server::serve::rebuild(&keeping.dir, keeping.database()).await.unwrap(), 2);
        assert_eq!(store.ratings().await.unwrap(), rows, "the same rows, number for number");
        assert_eq!(store.results_of(&ann).await.unwrap(), results, "the same results, receipt for receipt");
        store.close().await;

        let results = keeping.dir.join("results.jsonl");
        let forged = std::fs::read_to_string(&results).unwrap().replacen("\"rated\\\":true", "\"rated\\\":false", 1);
        assert_ne!(forged, std::fs::read_to_string(&results).unwrap(), "the test edited a line");
        std::fs::write(&results, forged).unwrap();
        let error = netrunner_server::serve::rebuild(&keeping.dir, keeping.database()).await.unwrap_err().to_string();
        assert!(error.contains("line 1"), "{error}");
        keeping.remove().await;
    }
}
