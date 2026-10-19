//! An operator's tools over real WebSockets (Phase 4 §12): the operators
//! are the daemon's configuration — `--operator` and `operators.txt` in
//! its data directory — and a proved operator's key is answered what no
//! other key is: the server's status, and a title or a badge granted or
//! taken back.

use std::path::PathBuf;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use netrunner_identity::{Identity, PublicKey};
use netrunner_server::protocol::{AdminAnswer, AdminRequest, AwardKind, AwardSpec, Profile};
use netrunner_server::serve::{ServeOptions, Server, OPERATORS_FILE};
use netrunner_server::{ClientMessage, ServerMessage};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn start(data_dir: Option<PathBuf>, operators: Vec<PublicKey>) -> String {
    let options = ServeOptions { seed: Some(1), data_dir, operators, ..ServeOptions::default() };
    let server = Server::bind("127.0.0.1:0", options).await.expect("an ephemeral port binds");
    let addr = server.local_addr().unwrap();
    tokio::spawn(server.run());
    format!("ws://{addr}")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("netrunner_admin_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
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

fn player(byte: u8) -> Identity {
    Identity::from_secret([byte; 32])
}

async fn attach(url: &str, identity: Option<&Identity>, name: &str) -> Socket {
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    if let Some(identity) = identity {
        send(&mut socket, ClientMessage::Identify { key: identity.public_key() }).await;
        let ServerMessage::Challenge { nonce, server_key, .. } = next(&mut socket).await else { panic!("expected Challenge") };
        send(&mut socket, ClientMessage::Prove { signature: identity.prove(&server_key, &nonce) }).await;
        assert!(matches!(next(&mut socket).await, ServerMessage::Identified { .. }));
    }
    send(&mut socket, ClientMessage::Attach { player_name: name.into() }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::Attached { .. }));
    socket
}

/// The answer to one admin request: `Ok` the answer, `Err` the refusal.
async fn admin(socket: &mut Socket, request: AdminRequest) -> Result<AdminAnswer, String> {
    send(socket, ClientMessage::Admin(request)).await;
    loop {
        match next(socket).await {
            ServerMessage::Admin(answer) => return Ok(answer),
            ServerMessage::AdminRefused { reason } => return Err(reason),
            _ => continue,
        }
    }
}

async fn profile(socket: &mut Socket) -> Profile {
    send(socket, ClientMessage::MyProfile).await;
    loop {
        if let ServerMessage::Profile { profile, .. } = next(socket).await {
            return profile.expect("kept");
        }
    }
}

fn title(name: &str) -> AwardSpec {
    AwardSpec { kind: AwardKind::Title, name: name.into() }
}

/// The operators file is read at bind beside `--operator`, comments and
/// blank lines skipped; a key in neither is refused, and so is a
/// connection that proved no key.
#[tokio::test]
async fn an_operator_is_named_by_the_flag_or_the_file() {
    let dir = scratch("operators");
    std::fs::write(dir.join(OPERATORS_FILE), format!("# who runs this server\n\n{}  # the second\n", player(8).public_key())).unwrap();
    let url = start(Some(dir.clone()), vec![player(9).public_key()]).await;
    for operator in [player(8), player(9)] {
        let mut socket = attach(&url, Some(&operator), "op").await;
        let Ok(AdminAnswer::Status(status)) = admin(&mut socket, AdminRequest::Status).await else { panic!("an operator is answered") };
        assert_eq!((status.operators, status.keeps, status.matches, status.tournaments, status.events), (2, true, 0, 0, 0), "{status:?}");
        assert!(status.attached >= 1 && status.engine.starts_with("netrunner_server"), "{status:?}");
    }
    let mut ann = attach(&url, Some(&player(1)), "ann").await;
    assert_eq!(admin(&mut ann, AdminRequest::Status).await, Err("your key does not operate this server".into()));
    let mut nobody = attach(&url, None, "nobody").await;
    assert!(admin(&mut nobody, AdminRequest::Status).await.unwrap_err().contains("prove one"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A line of the file that is not a key stops the daemon, naming the
/// line, rather than lock its operator out.
#[tokio::test]
async fn a_bad_operators_file_is_refused_at_bind() {
    let dir = scratch("bad_operators");
    std::fs::write(dir.join(OPERATORS_FILE), format!("{}\nnot a key\n", player(8).public_key())).unwrap();
    let error = Server::bind("127.0.0.1:0", ServeOptions { data_dir: Some(dir.clone()), ..ServeOptions::default() }).await.err().expect("refused");
    assert!(error.to_string().contains("operators.txt line 2"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// An operator grants a title to a key that has attached: it is in the
/// key's profile at once, filed under the operator's key, and the key may
/// wear it; revoked, it is gone and taken off. A key never seen, a blank
/// name and an award not held are refused, and a server that keeps
/// nothing holds no award at all.
#[tokio::test]
async fn an_operator_grants_and_revokes_an_award() {
    let dir = scratch("awards");
    let operator = player(9);
    let url = start(Some(dir.clone()), vec![operator.public_key()]).await;
    let mut op = attach(&url, Some(&operator), "op").await;
    let ann_key = player(1).public_key();
    assert_eq!(admin(&mut op, AdminRequest::Grant { key: ann_key, award: title("Founder") }).await, Err("this server has never seen that key attach".into()));

    let mut ann = attach(&url, Some(&player(1)), "ann").await;
    let Ok(AdminAnswer::Awards { key, titles, badges }) = admin(&mut op, AdminRequest::Grant { key: ann_key, award: title("  Founder ") }).await else { panic!("granted") };
    assert_eq!((key, titles.len(), badges.len()), (ann_key, 1, 0));
    assert_eq!((titles[0].name.as_str(), titles[0].source.clone()), ("Founder", operator.public_key().rating_id()), "trimmed, and filed under the operator's key");
    assert_eq!(profile(&mut ann).await.titles.iter().map(|award| award.name.as_str()).collect::<Vec<_>>(), ["Founder"]);
    send(&mut ann, ClientMessage::SetProfile { title: Some("Founder".into()), badge: None }).await;
    loop {
        if let ServerMessage::Profile { profile, .. } = next(&mut ann).await {
            assert_eq!(profile.unwrap().title.as_deref(), Some("Founder"));
            break;
        }
    }

    let Ok(AdminAnswer::Awards { titles, .. }) = admin(&mut op, AdminRequest::Revoke { key: ann_key, award: title("Founder") }).await else { panic!("revoked") };
    assert!(titles.is_empty());
    let after = profile(&mut ann).await;
    assert_eq!((after.titles.len(), after.title), (0, None), "taken back and taken off");
    assert_eq!(admin(&mut op, AdminRequest::Revoke { key: ann_key, award: title("Founder") }).await, Err("that key holds no title \"Founder\"".into()));
    assert_eq!(admin(&mut op, AdminRequest::Grant { key: ann_key, award: AwardSpec { kind: AwardKind::Badge, name: "  ".into() } }).await, Err("a badge needs a name".into()));
    let _ = std::fs::remove_dir_all(&dir);

    let url = start(None, vec![operator.public_key()]).await;
    let mut op = attach(&url, Some(&operator), "op").await;
    assert_eq!(admin(&mut op, AdminRequest::Grant { key: ann_key, award: title("Founder") }).await, Err("this server keeps nothing, so it holds no award".into()));
}

/// Two unproved players paired in a Casual queue: the first seeks, the
/// second is seated against them. Both sockets are in the game.
async fn paired(url: &str) -> (Socket, Socket) {
    let chair = |id: &str| netrunner_server::protocol::Chair::Either { corp: Box::new(netrunner_core::decks::by_id("brick_stack").unwrap()), runner: Box::new(netrunner_core::decks::by_id(id).unwrap()), prefer: None };
    let lobby = netrunner_server::protocol::queue_id(netrunner_server::protocol::QueueMode::Casual, netrunner_core::format::NsgFormat::Startup);
    let mut sockets = Vec::new();
    for name in ["ann", "bo"] {
        let mut socket = attach(url, None, name).await;
        send(&mut socket, ClientMessage::JoinLobby { lobby: lobby.clone(), password: None }).await;
        assert!(matches!(next(&mut socket).await, ServerMessage::LobbyJoined { .. }));
        send(&mut socket, ClientMessage::Seek { chair: chair("dashing_mad"), opponent: netrunner_server::protocol::Opponent::Person }).await;
        sockets.push(socket);
    }
    let mut second = sockets.pop().unwrap();
    let mut first = sockets.pop().unwrap();
    assert!(matches!(next(&mut first).await, ServerMessage::Queued { .. }));
    for socket in [&mut first, &mut second] {
        loop {
            if let ServerMessage::MatchJoined { .. } = next(socket).await {
                break;
            }
        }
    }
    (first, second)
}

/// A seek's refusal, past the game's traffic.
async fn seek_refused(socket: &mut Socket) -> String {
    loop {
        match next(socket).await {
            ServerMessage::SeekRefused { reason } => return reason,
            ServerMessage::MatchJoined { .. } => panic!("seated, not refused"),
            _ => continue,
        }
    }
}

/// A drain (Phase 4 §12 stage 4): the operator drains a server with a
/// game under way and a seek waiting; the seek is told and withdrawn, a
/// new one is refused, the server runs on until the game ends and then
/// `run` returns. An undrain before then takes games again.
#[tokio::test]
async fn a_drained_server_finishes_its_games_and_stops() {
    let operator = player(9);
    let server = Server::bind("127.0.0.1:0", ServeOptions { seed: Some(1), operators: vec![operator.public_key()], ..ServeOptions::default() }).await.unwrap();
    let url = format!("ws://{}", server.local_addr().unwrap());
    let running = tokio::spawn(server.run());
    let (mut ann, mut bo) = paired(&url).await;

    // A third waits in a lobby of its own: the drain withdraws it.
    let lobby = netrunner_server::protocol::queue_id(netrunner_server::protocol::QueueMode::Casual, netrunner_core::format::NsgFormat::Standard);
    let mut cy = attach(&url, None, "cy").await;
    send(&mut cy, ClientMessage::JoinLobby { lobby, password: None }).await;
    assert!(matches!(next(&mut cy).await, ServerMessage::LobbyJoined { .. }));
    let standard = netrunner_server::protocol::Chair::Either { corp: Box::new(netrunner_core::decks::by_id("brick_stack").unwrap()), runner: Box::new(netrunner_core::decks::by_id("dashing_mad").unwrap()), prefer: None };
    send(&mut cy, ClientMessage::Seek { chair: standard.clone(), opponent: netrunner_server::protocol::Opponent::Person }).await;
    assert!(matches!(next(&mut cy).await, ServerMessage::Queued { .. }));

    let mut op = attach(&url, Some(&operator), "op").await;
    let Ok(AdminAnswer::Status(status)) = admin(&mut op, AdminRequest::Drain).await else { panic!("drained") };
    assert_eq!((status.draining, status.matches, status.waiting), (true, 1, 0), "{status:?}");
    assert!(seek_refused(&mut cy).await.contains("shutting down"));
    send(&mut cy, ClientMessage::Seek { chair: standard.clone(), opponent: netrunner_server::protocol::Opponent::Person }).await;
    assert!(seek_refused(&mut cy).await.contains("shutting down"), "a new seek is refused too");

    // Taken back and given again, with the game still running.
    let Ok(AdminAnswer::Status(status)) = admin(&mut op, AdminRequest::Undrain).await else { panic!("undrained") };
    assert!(!status.draining);
    let Ok(AdminAnswer::Status(_)) = admin(&mut op, AdminRequest::Drain).await else { panic!("drained") };
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!running.is_finished(), "a game is under way");

    send(&mut bo, ClientMessage::Surrender).await;
    loop {
        if let ServerMessage::GameEnded { .. } = next(&mut ann).await {
            break;
        }
    }
    tokio::time::timeout(Duration::from_secs(10), running).await.expect("the server stops once its last game has ended").unwrap().unwrap();
}

/// A drain with nothing running stops the server at once, and so does a
/// stop with a game under way.
#[tokio::test]
async fn an_idle_drain_and_a_stop_end_the_server_at_once() {
    let operator = player(9);
    let server = Server::bind("127.0.0.1:0", ServeOptions { seed: Some(1), operators: vec![operator.public_key()], ..ServeOptions::default() }).await.unwrap();
    let url = format!("ws://{}", server.local_addr().unwrap());
    let running = tokio::spawn(server.run());
    let mut op = attach(&url, Some(&operator), "op").await;
    let Ok(AdminAnswer::Status(status)) = admin(&mut op, AdminRequest::Drain).await else { panic!("drained") };
    assert_eq!((status.draining, status.matches), (true, 0));
    tokio::time::timeout(Duration::from_secs(10), running).await.expect("nothing running: stopped").unwrap().unwrap();

    let server = Server::bind("127.0.0.1:0", ServeOptions { seed: Some(1), ..ServeOptions::default() }).await.unwrap();
    let url = format!("ws://{}", server.local_addr().unwrap());
    let drainer = server.drainer();
    let running = tokio::spawn(server.run());
    let _game = paired(&url).await;
    drainer.stop();
    tokio::time::timeout(Duration::from_secs(10), running).await.expect("stopped at once").unwrap().unwrap();
}
