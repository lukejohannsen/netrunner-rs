//! Tournaments over real WebSockets (Phase 4 §7 stage 6a): a proved key
//! holds one on a daemon that keeps things, entrants register the two
//! decks they will play for the whole event behind a signed commitment
//! the server checks and publishes, and the tournament outlives the
//! daemon that took the registrations.

use std::path::PathBuf;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use netrunner_core::decks::{self, DeckFile};
use netrunner_core::format::NsgFormat;
use netrunner_identity::{Identity, PublicKey, Signed};
use netrunner_server::protocol::statements::{deck_hash, RegistrationStatement, REGISTRATION_TAG};
use netrunner_server::protocol::{TournamentInfo, TournamentState};
use netrunner_server::serve::{ServeBotKind, ServeOptions, Server};
use netrunner_server::{ClientMessage, ServerMessage};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn start(data_dir: Option<PathBuf>) -> (String, PublicKey) {
    let options = ServeOptions { bot_runner: ServeBotKind::None, seed: Some(1), data_dir, ..ServeOptions::default() };
    let server = Server::bind("127.0.0.1:0", options).await.expect("an ephemeral port binds");
    let (addr, key) = (server.local_addr().unwrap(), server.public_key());
    tokio::spawn(server.run());
    (format!("ws://{addr}"), key)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("netrunner_tournament_{name}_{}", std::process::id()));
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

fn player(byte: u8) -> Identity {
    Identity::from_secret([byte; 32])
}

/// Attached as `name`, proving `identity` first when there is one.
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

fn deck(id: &str) -> Box<DeckFile> {
    Box::new(decks::by_id(id).expect("a built-in deck"))
}

/// The registration a client's connection would sign (`netrunner_client::
/// connection::Connection::register`), built here by hand so the server
/// is tested against the statement and not against the client.
fn statement(identity: &Identity, server_key: PublicKey, tournament: &str, salt: &str, corp: &DeckFile, runner: &DeckFile) -> Signed {
    let said = RegistrationStatement {
        tournament: tournament.to_string(),
        server_key,
        key: identity.public_key(),
        corp_hash: deck_hash(salt, &corp.to_deck()),
        runner_hash: deck_hash(salt, &runner.to_deck()),
    };
    identity.sign(REGISTRATION_TAG, serde_json::to_string(&said).unwrap())
}

async fn create(socket: &mut Socket, name: &str) -> TournamentInfo {
    send(socket, ClientMessage::CreateTournament { name: name.into(), format: NsgFormat::Startup }).await;
    match next(socket).await {
        ServerMessage::Tournament { tournament } => tournament,
        other => panic!("not made: {other:?}"),
    }
}

async fn refused(socket: &mut Socket) -> String {
    match next(socket).await {
        ServerMessage::TournamentRefused { reason } => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// A tournament outlives any socket, so only a daemon that keeps things
/// holds one; and it is run by a key, which the policies give the final
/// say, so a connection that proved none cannot make one.
#[tokio::test]
async fn a_tournament_needs_a_key_and_a_daemon_that_keeps_things() {
    let (stateless, _) = start(None).await;
    let mut socket = attach(&stateless, Some(&player(1)), "ann").await;
    send(&mut socket, ClientMessage::CreateTournament { name: "Friday".into(), format: NsgFormat::Startup }).await;
    assert!(refused(&mut socket).await.contains("keeps nothing"));
    send(&mut socket, ClientMessage::ListTournaments).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::Tournaments { tournaments } if tournaments.is_empty()));

    let dir = scratch("needs_key");
    let (url, _) = start(Some(dir.clone())).await;
    let mut unproved = attach(&url, None, "nobody").await;
    send(&mut unproved, ClientMessage::CreateTournament { name: "Friday".into(), format: NsgFormat::Startup }).await;
    assert!(refused(&mut unproved).await.contains("prove one"));
    send(&mut unproved, ClientMessage::CreateTournament { name: "   ".into(), format: NsgFormat::Startup }).await;
    refused(&mut unproved).await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// An entrant's registration is checked before it is kept — the
/// statement against the key, the server and the tournament, the hashes
/// against the decks sent, the decks against the format — and what the
/// server then publishes is the commitment and never the lists.
#[tokio::test]
async fn a_registration_is_a_checked_commitment_the_server_publishes() {
    let dir = scratch("registration");
    let (url, server_key) = start(Some(dir.clone())).await;
    let (ann, bo) = (player(1), player(2));
    let mut organizer = attach(&url, Some(&ann), "ann").await;
    let made = create(&mut organizer, "Friday Night Startup").await;
    assert_eq!((made.organizer, made.state, made.entrants.len()), (ann.public_key(), TournamentState::Registering, 0));
    assert_eq!(made.id.len(), 6, "a code, as a player's lobby has: {}", made.id);

    let mut entrant = attach(&url, Some(&bo), "bo").await;
    let (corp, runner) = (deck("brick_stack"), deck("dashing_mad"));
    let register = |tournament: &str, salt: &str, signed_by: &Identity, corp: &DeckFile, runner: &DeckFile| ClientMessage::Register {
        tournament: tournament.to_string(),
        corp: Box::new(corp.clone()),
        runner: Box::new(runner.clone()),
        salt: salt.to_string(),
        statement: statement(signed_by, server_key, tournament, salt, corp, runner),
    };

    // Refused, each for its own reason, and nothing kept.
    send(&mut entrant, register("NOSUCH", "s1", &bo, &corp, &runner)).await;
    assert!(refused(&mut entrant).await.contains("no such tournament"));
    send(&mut entrant, register(&made.id, "s1", &ann, &corp, &runner)).await;
    assert!(refused(&mut entrant).await.contains("another key"));
    let mut wrong_salt = register(&made.id, "s1", &bo, &corp, &runner);
    if let ClientMessage::Register { salt, .. } = &mut wrong_salt {
        *salt = "s2".into();
    }
    send(&mut entrant, wrong_salt).await;
    assert!(refused(&mut entrant).await.contains("does not name the decks"));
    send(&mut entrant, register(&made.id, "s1", &bo, &runner, &corp)).await;
    assert!(refused(&mut entrant).await.contains("not a Corp one"));
    let eternal_only = deck("assembly_line");
    send(&mut entrant, register(&made.id, "s1", &bo, &eternal_only, &runner)).await;
    assert!(refused(&mut entrant).await.contains("not legal in Startup"));
    send(&mut entrant, ClientMessage::Unregister { tournament: made.id.clone() }).await;
    assert!(refused(&mut entrant).await.contains("not registered"));

    // Taken: the entrant is public by commitment, the lists are not.
    send(&mut entrant, register(&made.id.to_lowercase(), "s1", &bo, &corp, &runner)).await;
    let ServerMessage::Tournament { tournament } = next(&mut entrant).await else { panic!() };
    let [only] = &tournament.entrants[..] else { panic!("one entrant: {:?}", tournament.entrants) };
    assert_eq!((only.name.as_str(), only.key), ("bo", bo.public_key()));
    assert_eq!(only.corp_hash, deck_hash("s1", &corp.to_deck()));
    assert_eq!(only.runner_hash, deck_hash("s1", &runner.to_deck()));
    let payload = only.commitment.verify(REGISTRATION_TAG).expect("bo signed it");
    let said: RegistrationStatement = serde_json::from_str(payload).unwrap();
    // The statement names the code as the client typed it; the server
    // read both as the code they spell.
    assert_eq!((said.tournament.to_uppercase().as_str(), said.server_key, said.key), (made.id.as_str(), server_key, bo.public_key()));
    let wire = serde_json::to_string(&tournament).unwrap();
    assert!(!wire.contains("brick_stack") && !wire.contains("dashing_mad"), "no deck on the wire: {wire}");

    // Registering again replaces; the organizer sees the same list.
    send(&mut entrant, register(&made.id, "s3", &bo, &corp, &runner)).await;
    let ServerMessage::Tournament { tournament } = next(&mut entrant).await else { panic!() };
    assert_eq!(tournament.entrants.len(), 1);
    assert_eq!(tournament.entrants[0].corp_hash, deck_hash("s3", &corp.to_deck()));
    send(&mut organizer, ClientMessage::ListTournaments).await;
    let ServerMessage::Tournaments { tournaments } = next(&mut organizer).await else { panic!() };
    assert_eq!(tournaments, vec![tournament.clone()]);

    // Withdrawn.
    send(&mut entrant, ClientMessage::Unregister { tournament: made.id.clone() }).await;
    let ServerMessage::Tournament { tournament } = next(&mut entrant).await else { panic!() };
    assert!(tournament.entrants.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The registrations are kept whole in the data directory, so a daemon
/// started on it holds the same tournament, entrant and commitment.
#[tokio::test]
async fn a_tournament_outlives_the_daemon_that_took_its_registrations() {
    let dir = scratch("outlives");
    let (url, server_key) = start(Some(dir.clone())).await;
    let (ann, bo) = (player(1), player(2));
    let made = create(&mut attach(&url, Some(&ann), "ann").await, "Saturday").await;
    let mut entrant = attach(&url, Some(&bo), "bo").await;
    let (corp, runner) = (deck("brick_stack"), deck("dashing_mad"));
    send(
        &mut entrant,
        ClientMessage::Register { tournament: made.id.clone(), corp: corp.clone(), runner: runner.clone(), salt: "s".into(), statement: statement(&bo, server_key, &made.id, "s", &corp, &runner) },
    )
    .await;
    let ServerMessage::Tournament { tournament: before } = next(&mut entrant).await else { panic!() };
    assert!(std::fs::read_to_string(dir.join("tournaments.json")).unwrap().contains(&made.id));

    let (again, same_key) = start(Some(dir.clone())).await;
    assert_eq!(same_key, server_key, "the same data directory is the same server");
    let mut socket = attach(&again, None, "anyone").await;
    send(&mut socket, ClientMessage::ListTournaments).await;
    let ServerMessage::Tournaments { tournaments } = next(&mut socket).await else { panic!() };
    assert_eq!(tournaments, vec![before]);
    let _ = std::fs::remove_dir_all(&dir);
}
