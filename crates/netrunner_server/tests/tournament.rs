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
use netrunner_core::rules::Side;
use netrunner_server::protocol::swiss::{Outcome, Role};
use netrunner_server::protocol::{DrawOffer, TournamentInfo, TournamentState};
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

/// The next refusal, past whatever else the socket carries — a pushed
/// `Tournament`, or a game's messages when the organizer is a player too.
async fn refused(socket: &mut Socket) -> String {
    loop {
        match next(socket).await {
            ServerMessage::TournamentRefused { reason } => return reason,
            ServerMessage::Tournament { tournament } => panic!("not refused: {tournament:?}"),
            _ => continue,
        }
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
    { let reason = refused(&mut socket).await; assert!(reason.contains("keeps nothing"), "{reason}"); }
    send(&mut socket, ClientMessage::ListTournaments).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::Tournaments { tournaments } if tournaments.is_empty()));

    let dir = scratch("needs_key");
    let (url, _) = start(Some(dir.clone())).await;
    let mut unproved = attach(&url, None, "nobody").await;
    send(&mut unproved, ClientMessage::CreateTournament { name: "Friday".into(), format: NsgFormat::Startup }).await;
    { let reason = refused(&mut unproved).await; assert!(reason.contains("prove one"), "{reason}"); }
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
    { let reason = refused(&mut entrant).await; assert!(reason.contains("no such tournament"), "{reason}"); }
    send(&mut entrant, register(&made.id, "s1", &ann, &corp, &runner)).await;
    { let reason = refused(&mut entrant).await; assert!(reason.contains("another key"), "{reason}"); }
    let mut wrong_salt = register(&made.id, "s1", &bo, &corp, &runner);
    if let ClientMessage::Register { salt, .. } = &mut wrong_salt {
        *salt = "s2".into();
    }
    send(&mut entrant, wrong_salt).await;
    { let reason = refused(&mut entrant).await; assert!(reason.contains("does not name the decks"), "{reason}"); }
    send(&mut entrant, register(&made.id, "s1", &bo, &runner, &corp)).await;
    { let reason = refused(&mut entrant).await; assert!(reason.contains("not a Corp one"), "{reason}"); }
    let eternal_only = deck("assembly_line");
    send(&mut entrant, register(&made.id, "s1", &bo, &eternal_only, &runner)).await;
    { let reason = refused(&mut entrant).await; assert!(reason.contains("not legal in Startup"), "{reason}"); }
    send(&mut entrant, ClientMessage::Unregister { tournament: made.id.clone() }).await;
    { let reason = refused(&mut entrant).await; assert!(reason.contains("not registered"), "{reason}"); }

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
    // The organizer was told of each registration unasked (stage 6b);
    // the list comes after those pushes.
    send(&mut organizer, ClientMessage::ListTournaments).await;
    let tournaments = loop {
        match next(&mut organizer).await {
            ServerMessage::Tournaments { tournaments } => break tournaments,
            ServerMessage::Tournament { tournament: pushed } => assert_eq!(pushed.entrants.len(), 1),
            other => panic!("{other:?}"),
        }
    };
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

/// The next `Tournament` on the socket, whatever else comes first — a
/// push arrives between a request and its answer.
async fn next_tournament(socket: &mut Socket) -> TournamentInfo {
    loop {
        match next(socket).await {
            ServerMessage::Tournament { tournament } => return tournament,
            ServerMessage::TournamentRefused { reason } => panic!("refused: {reason}"),
            _ => continue,
        }
    }
}

/// The seat the socket is given: its side and its own deck's id.
async fn next_joined(socket: &mut Socket) -> (Side, String) {
    loop {
        match next(socket).await {
            ServerMessage::MatchJoined { assigned_side, corp_deck, runner_deck, .. } => {
                return (assigned_side, if assigned_side == Side::Corp { corp_deck } else { runner_deck });
            }
            ServerMessage::SeekRefused { reason } => panic!("not seated: {reason}"),
            _ => continue,
        }
    }
}

async fn seek_refused(socket: &mut Socket) -> String {
    loop {
        match next(socket).await {
            ServerMessage::SeekRefused { reason } => return reason,
            ServerMessage::MatchJoined { .. } | ServerMessage::Queued { .. } => panic!("seated, not refused"),
            _ => continue,
        }
    }
}

/// The rounds (Phase 4 §7 stage 6b): the organizer begins round 1, which
/// closes registration at three entrants and pairs one table with a bye;
/// the two paired sit and are seated on the sides the pairing gave, each
/// dealt the deck they registered for that side; a surrender writes the
/// table's result and every entrant on is told unasked; the next round
/// is refused while a table is open and recorded by the organizer where
/// nobody showed; the bye moves to who has not had one; the organizer
/// ends it, the standings fold off the rounds, and a daemon restarted on
/// the directory holds all of it.
#[tokio::test]
async fn rounds_are_paired_played_recorded_and_kept() {
    let dir = scratch("rounds");
    let (url, server_key) = start(Some(dir.clone())).await;
    let people = [player(1), player(2), player(3)];
    let keys: Vec<PublicKey> = people.iter().map(Identity::public_key).collect();
    let mut sockets = Vec::new();
    for (identity, name) in people.iter().zip(["ann", "bo", "cy"]) {
        sockets.push(attach(&url, Some(identity), name).await);
    }
    let made = create(&mut sockets[0], "Rounds").await;
    let (corp, runner) = (deck("brick_stack"), deck("dashing_mad"));
    for (index, identity) in people.iter().enumerate() {
        let salt = format!("s{index}");
        send(&mut sockets[index], ClientMessage::Register { tournament: made.id.clone(), corp: corp.clone(), runner: runner.clone(), salt: salt.clone(), statement: statement(identity, server_key, &made.id, &salt, &corp, &runner) }).await;
        let entered = next_tournament(&mut sockets[index]).await;
        assert_eq!(entered.entrants.len(), index + 1);
    }
    // Entrants on are told of each other's arrival unasked: ann saw bo
    // and cy register, bo saw cy.
    assert_eq!(next_tournament(&mut sockets[0]).await.entrants.len(), 2);
    assert_eq!(next_tournament(&mut sockets[0]).await.entrants.len(), 3);
    assert_eq!(next_tournament(&mut sockets[1]).await.entrants.len(), 3);

    // Only the organizer begins a round; the first closes registration.
    send(&mut sockets[1], ClientMessage::BeginRound { tournament: made.id.clone() }).await;
    { let reason = refused(&mut sockets[1]).await; assert!(reason.contains("only the organizer"), "{reason}"); }
    send(&mut sockets[0], ClientMessage::BeginRound { tournament: made.id.clone() }).await;
    let first = next_tournament(&mut sockets[0]).await;
    assert_eq!(first.state, TournamentState::Playing { round: 1 });
    assert_eq!(first.seeding.len(), 3);
    assert_eq!((first.rounds.len(), first.rounds[0].tables.len()), (1, 1));
    let bye = first.rounds[0].bye.expect("three entrants: one sits out");
    assert_eq!(bye, *first.seeding.last().unwrap(), "round 1's bye is the seeding's last");
    assert_eq!(next_tournament(&mut sockets[1]).await, first, "pushed to the entrants on");
    assert_eq!(next_tournament(&mut sockets[2]).await, first);
    send(&mut sockets[1], ClientMessage::Register { tournament: made.id.clone(), corp: corp.clone(), runner: runner.clone(), salt: "late".into(), statement: statement(&people[1], server_key, &made.id, "late", &corp, &runner) }).await;
    { let reason = refused(&mut sockets[1]).await; assert!(reason.contains("closed"), "{reason}"); }

    // The bye has no table; the two paired sit, first one then the other,
    // and are seated on the pairing's sides with the decks they locked.
    let index_of = |key: &PublicKey| keys.iter().position(|k| k == key).unwrap();
    let table = first.rounds[0].tables[0].clone();
    let (corp_at, runner_at) = (index_of(&table.corp), index_of(&table.runner));
    send(&mut sockets[index_of(&bye)], ClientMessage::Sit { tournament: made.id.clone() }).await;
    let reason = seek_refused(&mut sockets[index_of(&bye)]).await;
    assert!(reason.contains("no table"), "{reason}");
    send(&mut sockets[runner_at], ClientMessage::Sit { tournament: made.id.clone() }).await;
    assert!(matches!(next(&mut sockets[runner_at]).await, ServerMessage::Queued { position: 1, .. }));
    send(&mut sockets[runner_at], ClientMessage::Sit { tournament: made.id.clone() }).await;
    { let reason = seek_refused(&mut sockets[runner_at]).await; assert!(reason.contains("cancel first"), "{reason}"); }
    // The organizer can neither begin the next round nor record over a
    // waiting seat's table without standing it up — recording does
    // that, so it is tried only on the next round; here, the game.
    send(&mut sockets[corp_at], ClientMessage::Sit { tournament: made.id.clone() }).await;
    assert_eq!(next_joined(&mut sockets[corp_at]).await, (Side::Corp, "brick_stack".to_string()));
    assert_eq!(next_joined(&mut sockets[runner_at]).await, (Side::Runner, "dashing_mad".to_string()));
    send(&mut sockets[0], ClientMessage::BeginRound { tournament: made.id.clone() }).await;
    { let reason = refused(&mut sockets[0]).await; assert!(reason.contains("no result yet"), "{reason}"); }
    send(&mut sockets[0], ClientMessage::RecordResult { tournament: made.id.clone(), table: 0, outcome: Outcome::Tie }).await;
    { let reason = refused(&mut sockets[0]).await; assert!(reason.contains("being played"), "{reason}"); }
    send(&mut sockets[0], ClientMessage::FinishTournament { tournament: made.id.clone() }).await;
    { let reason = refused(&mut sockets[0]).await; assert!(reason.contains("no result yet"), "{reason}"); }

    // The Corp concedes: the Runner won the table, and everyone is told.
    send(&mut sockets[corp_at], ClientMessage::Surrender).await;
    let after = next_tournament(&mut sockets[index_of(&bye)]).await;
    for index in [corp_at, runner_at] {
        assert_eq!(next_tournament(&mut sockets[index]).await, after, "the players are told too, past their game's end");
    }
    assert_eq!(after.rounds[0].tables[0].result, Some(Outcome::RunnerWon));
    assert!(after.rounds[0].complete());
    let standings = after.standings();
    assert_eq!(standings.iter().map(|row| row.points).collect::<Vec<_>>(), vec![3, 3, 0]);
    assert!(standings[..2].iter().any(|row| row.key == bye && row.byes == 1));

    // Round 2: the bye goes to the lowest who has not had one — the
    // loser — and the two on 3 points meet.
    send(&mut sockets[0], ClientMessage::BeginRound { tournament: made.id.clone() }).await;
    let second = next_tournament(&mut sockets[0]).await;
    assert_eq!(second.state, TournamentState::Playing { round: 2 });
    assert_eq!(second.rounds[1].bye, Some(table.corp), "the loser sits out");
    let rematch = &second.rounds[1].tables[0];
    assert!(rematch.role_of(&table.runner).is_some() && rematch.role_of(&bye).is_some(), "the two on 3 points meet");
    // Nobody shows: the organizer records a tie, after which the round is
    // complete and a seat is refused.
    send(&mut sockets[0], ClientMessage::RecordResult { tournament: made.id.clone(), table: 0, outcome: Outcome::Tie }).await;
    let recorded = next_tournament(&mut sockets[0]).await;
    assert_eq!(recorded.rounds[1].tables[0].result, Some(Outcome::Tie));
    send(&mut sockets[0], ClientMessage::RecordResult { tournament: made.id.clone(), table: 0, outcome: Outcome::CorpWon }).await;
    { let reason = refused(&mut sockets[0]).await; assert!(reason.contains("has its result"), "{reason}"); }
    send(&mut sockets[index_of(&bye)], ClientMessage::Sit { tournament: made.id.clone() }).await;
    let reason = seek_refused(&mut sockets[index_of(&bye)]).await;
    assert!(reason.contains("has its result"), "round 1's bye is at round 2's recorded table: {reason}");

    // Finished: the standings are final, and nobody sits again.
    send(&mut sockets[0], ClientMessage::FinishTournament { tournament: made.id.clone() }).await;
    let finished = next_tournament(&mut sockets[0]).await;
    assert_eq!(finished.state, TournamentState::Finished);
    let standings = finished.standings();
    assert_eq!(standings.iter().map(|row| (row.points, row.wins + row.byes, row.ties, row.losses)).collect::<Vec<_>>(), vec![(4, 1, 1, 0), (4, 1, 1, 0), (3, 1, 0, 1)], "the loser: a loss, then a bye");
    assert_eq!(standings[2].key, table.corp, "the loser's bye leaves them third");
    assert!(standings.iter().find(|row| row.key == table.runner).is_some_and(|row| row.runner_games == 1 && row.corp_games + row.runner_games == 2 || row.rounds_played() == 2));
    send(&mut sockets[0], ClientMessage::BeginRound { tournament: made.id.clone() }).await;
    { let reason = refused(&mut sockets[0]).await; assert!(reason.contains("over"), "{reason}"); }
    send(&mut sockets[index_of(&table.runner)], ClientMessage::Sit { tournament: made.id.clone() }).await;
    let reason = seek_refused(&mut sockets[index_of(&table.runner)]).await;
    assert!(reason.contains("no round"), "{reason}");
    let _ = Role::Corp;

    // A daemon restarted on the directory holds the rounds.
    let (again, _) = start(Some(dir.clone())).await;
    let mut socket = attach(&again, None, "anyone").await;
    send(&mut socket, ClientMessage::ListTournaments).await;
    let ServerMessage::Tournaments { tournaments } = next(&mut socket).await else { panic!() };
    assert_eq!(tournaments, vec![finished]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A drop mid-event (Phase 4 §7 stage 6b's leftover): the entry and its
/// results stay, the key is paired no more; a table of the round it has
/// not played is forfeit to the opponent, who is stood up if waiting
/// there; a drop with a game under way is refused until the game is
/// conceded on the board; a tournament with one player left pairs no
/// further round; a restarted daemon holds who dropped.
#[tokio::test]
async fn a_drop_forfeits_its_table_and_is_paired_no_more() {
    let dir = scratch("drop");
    let (url, server_key) = start(Some(dir.clone())).await;
    let people = [player(1), player(2), player(3)];
    let keys: Vec<PublicKey> = people.iter().map(Identity::public_key).collect();
    let mut sockets = Vec::new();
    for (identity, name) in people.iter().zip(["ann", "bo", "cy"]) {
        sockets.push(attach(&url, Some(identity), name).await);
    }
    let made = create(&mut sockets[0], "Drops").await;
    let (corp, runner) = (deck("brick_stack"), deck("dashing_mad"));
    for (index, identity) in people.iter().enumerate() {
        let salt = format!("s{index}");
        send(&mut sockets[index], ClientMessage::Register { tournament: made.id.clone(), corp: corp.clone(), runner: runner.clone(), salt: salt.clone(), statement: statement(identity, server_key, &made.id, &salt, &corp, &runner) }).await;
        next_tournament(&mut sockets[index]).await;
    }
    next_tournament(&mut sockets[0]).await;
    next_tournament(&mut sockets[0]).await;
    next_tournament(&mut sockets[1]).await;
    send(&mut sockets[0], ClientMessage::BeginRound { tournament: made.id.clone() }).await;
    let first = next_tournament(&mut sockets[0]).await;
    assert_eq!(next_tournament(&mut sockets[1]).await, first);
    assert_eq!(next_tournament(&mut sockets[2]).await, first);
    let index_of = |key: &PublicKey| keys.iter().position(|k| k == key).unwrap();
    let table = first.rounds[0].tables[0].clone();
    let bye = first.rounds[0].bye.expect("three entrants: one sits out");
    let (corp_at, runner_at) = (index_of(&table.corp), index_of(&table.runner));

    // The Runner sits and waits; the Corp drops: the table is the
    // Runner's, who is stood up and told, and everyone sees the drop.
    send(&mut sockets[runner_at], ClientMessage::Sit { tournament: made.id.clone() }).await;
    assert!(matches!(next(&mut sockets[runner_at]).await, ServerMessage::Queued { position: 1, .. }));
    send(&mut sockets[corp_at], ClientMessage::Unregister { tournament: made.id.clone() }).await;
    let dropped = next_tournament(&mut sockets[corp_at]).await;
    assert_eq!(dropped.dropped, vec![table.corp]);
    assert_eq!(dropped.entrants.len(), 3, "a drop is still an entrant");
    assert_eq!(dropped.rounds[0].tables[0].result, Some(Outcome::RunnerWon), "forfeit");
    // The push goes out on the connection's own channel and the refusal
    // through the seat's, so the Runner may see them in either order.
    let (mut told, mut stood_up) = (None, None);
    while told.is_none() || stood_up.is_none() {
        match next(&mut sockets[runner_at]).await {
            ServerMessage::Tournament { tournament } => told = Some(tournament),
            ServerMessage::SeekRefused { reason } => stood_up = Some(reason),
            _ => {}
        }
    }
    assert_eq!(told, Some(dropped.clone()));
    assert!(stood_up.as_deref().is_some_and(|reason| reason.contains("opponent dropped")), "{stood_up:?}");
    assert_eq!(next_tournament(&mut sockets[index_of(&bye)]).await, dropped);
    send(&mut sockets[corp_at], ClientMessage::Unregister { tournament: made.id.clone() }).await;
    { let reason = refused(&mut sockets[corp_at]).await; assert!(reason.contains("dropped already"), "{reason}"); }
    send(&mut sockets[corp_at], ClientMessage::Sit { tournament: made.id.clone() }).await;
    { let reason = seek_refused(&mut sockets[corp_at]).await; assert!(reason.contains("has its result"), "{reason}"); }

    // Round 2 pairs the two left, with no bye, and the drop at no table.
    send(&mut sockets[0], ClientMessage::BeginRound { tournament: made.id.clone() }).await;
    let second = next_tournament(&mut sockets[0]).await;
    assert_eq!(second.state, TournamentState::Playing { round: 2 });
    assert_eq!((second.rounds[1].bye, second.rounds[1].tables.len()), (None, 1));
    let rematch = second.rounds[1].tables[0].clone();
    assert!(rematch.role_of(&table.corp).is_none() && rematch.role_of(&table.runner).is_some() && rematch.role_of(&bye).is_some());
    for socket in sockets.iter_mut().skip(1) {
        assert_eq!(next_tournament(socket).await, second, "the drop is told too: still an entrant");
    }
    send(&mut sockets[corp_at], ClientMessage::Sit { tournament: made.id.clone() }).await;
    { let reason = seek_refused(&mut sockets[corp_at]).await; assert!(reason.contains("no table"), "{reason}"); }

    // Both sit; a drop with the game under way is refused; the loser
    // concedes, then drops, after which nobody is left to pair and the
    // organizer ends it with the drops in the final standings.
    let (corp2, runner2) = (index_of(&rematch.corp), index_of(&rematch.runner));
    send(&mut sockets[corp2], ClientMessage::Sit { tournament: made.id.clone() }).await;
    assert!(matches!(next(&mut sockets[corp2]).await, ServerMessage::Queued { .. }));
    send(&mut sockets[runner2], ClientMessage::Sit { tournament: made.id.clone() }).await;
    next_joined(&mut sockets[runner2]).await;
    next_joined(&mut sockets[corp2]).await;
    send(&mut sockets[runner2], ClientMessage::Unregister { tournament: made.id.clone() }).await;
    { let reason = refused(&mut sockets[runner2]).await; assert!(reason.contains("under way"), "{reason}"); }
    send(&mut sockets[runner2], ClientMessage::Surrender).await;
    let after = next_tournament(&mut sockets[corp_at]).await;
    assert_eq!(after.rounds[1].tables[0].result, Some(Outcome::CorpWon));
    for index in [corp2, runner2] {
        assert_eq!(next_tournament(&mut sockets[index]).await, after);
    }
    send(&mut sockets[runner2], ClientMessage::Unregister { tournament: made.id.clone() }).await;
    let two_gone = next_tournament(&mut sockets[runner2]).await;
    assert_eq!(two_gone.dropped.len(), 2);
    assert_eq!(two_gone.rounds[1].tables[0].result, Some(Outcome::CorpWon), "a played table is not forfeit");
    for (index, socket) in sockets.iter_mut().enumerate() {
        if index != runner2 {
            next_tournament(socket).await;
        }
    }
    send(&mut sockets[0], ClientMessage::BeginRound { tournament: made.id.clone() }).await;
    { let reason = refused(&mut sockets[0]).await; assert!(reason.contains("fewer than two"), "{reason}"); }
    send(&mut sockets[0], ClientMessage::FinishTournament { tournament: made.id.clone() }).await;
    let finished = next_tournament(&mut sockets[0]).await;
    assert_eq!(finished.state, TournamentState::Finished);
    let standings = finished.standings();
    assert_eq!(standings.len(), 3, "the drops keep their rows");
    assert_eq!(standings[0].key, rematch.corp);
    assert!(finished.dropped.contains(&table.corp) && finished.dropped.contains(&rematch.runner));

    // A daemon restarted on the directory holds who dropped.
    let (again, _) = start(Some(dir.clone())).await;
    let mut socket = attach(&again, None, "anyone").await;
    send(&mut socket, ClientMessage::ListTournaments).await;
    let ServerMessage::Tournaments { tournaments } = next(&mut socket).await else { panic!() };
    assert_eq!(tournaments, vec![finished]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// An intentional draw (Organized Play Policies 2.5.8) is two offers:
/// the first is published to the table and waits, a second from the same
/// key is refused, the opponent's is the agreement — the table is a tie
/// and a player seated and waiting there is stood up and told — and a
/// game under way takes no offer, an earlier one having lapsed when the
/// game started.
#[tokio::test]
async fn an_intentional_draw_is_offered_by_both_players() {
    let dir = scratch("draw");
    let (url, server_key) = start(Some(dir.clone())).await;
    let people = [player(1), player(2)];
    let keys: Vec<PublicKey> = people.iter().map(Identity::public_key).collect();
    let mut sockets = Vec::new();
    for (identity, name) in people.iter().zip(["ann", "bo"]) {
        sockets.push(attach(&url, Some(identity), name).await);
    }
    let made = create(&mut sockets[0], "Draws").await;
    let (corp, runner) = (deck("brick_stack"), deck("dashing_mad"));
    for (index, identity) in people.iter().enumerate() {
        let salt = format!("s{index}");
        send(&mut sockets[index], ClientMessage::Register { tournament: made.id.clone(), corp: corp.clone(), runner: runner.clone(), salt: salt.clone(), statement: statement(identity, server_key, &made.id, &salt, &corp, &runner) }).await;
        next_tournament(&mut sockets[index]).await;
    }
    next_tournament(&mut sockets[0]).await;
    send(&mut sockets[1], ClientMessage::OfferDraw { tournament: made.id.clone() }).await;
    { let reason = refused(&mut sockets[1]).await; assert!(reason.contains("no round"), "{reason}"); }
    send(&mut sockets[0], ClientMessage::BeginRound { tournament: made.id.clone() }).await;
    let first = next_tournament(&mut sockets[0]).await;
    assert_eq!(next_tournament(&mut sockets[1]).await, first);
    let index_of = |key: &PublicKey| keys.iter().position(|k| k == key).unwrap();
    let table = first.rounds[0].tables[0].clone();
    let (corp_at, runner_at) = (index_of(&table.corp), index_of(&table.runner));

    // The Corp offers, then sits and waits; the Corp cannot offer twice;
    // the Runner's offer back is the tie, and the Corp is stood up.
    send(&mut sockets[corp_at], ClientMessage::OfferDraw { tournament: made.id.clone() }).await;
    let offered = next_tournament(&mut sockets[corp_at]).await;
    assert_eq!(offered.draw_offers, vec![DrawOffer { table: 0, by: table.corp }]);
    assert_eq!(offered.rounds[0].tables[0].result, None, "one offer is not a draw");
    assert_eq!(next_tournament(&mut sockets[runner_at]).await, offered);
    send(&mut sockets[corp_at], ClientMessage::Sit { tournament: made.id.clone() }).await;
    assert!(matches!(next(&mut sockets[corp_at]).await, ServerMessage::Queued { .. }));
    send(&mut sockets[corp_at], ClientMessage::OfferDraw { tournament: made.id.clone() }).await;
    { let reason = refused(&mut sockets[corp_at]).await; assert!(reason.contains("offered a draw already"), "{reason}"); }
    send(&mut sockets[runner_at], ClientMessage::OfferDraw { tournament: made.id.clone() }).await;
    let agreed = next_tournament(&mut sockets[runner_at]).await;
    assert_eq!(agreed.rounds[0].tables[0].result, Some(Outcome::Tie));
    assert!(agreed.draw_offers.is_empty(), "the agreement empties the table's offers");
    let (mut told, mut stood_up) = (None, None);
    while told.is_none() || stood_up.is_none() {
        match next(&mut sockets[corp_at]).await {
            ServerMessage::Tournament { tournament } => told = Some(tournament),
            ServerMessage::SeekRefused { reason } => stood_up = Some(reason),
            _ => {}
        }
    }
    assert_eq!(told, Some(agreed.clone()));
    assert!(stood_up.as_deref().is_some_and(|reason| reason.contains("agreed a draw")), "{stood_up:?}");
    send(&mut sockets[runner_at], ClientMessage::OfferDraw { tournament: made.id.clone() }).await;
    { let reason = refused(&mut sockets[runner_at]).await; assert!(reason.contains("has its result"), "{reason}"); }

    // Round 2: an offer lapses when the game starts, and none is taken
    // while it is under way.
    send(&mut sockets[0], ClientMessage::BeginRound { tournament: made.id.clone() }).await;
    let second = next_tournament(&mut sockets[0]).await;
    next_tournament(&mut sockets[1]).await;
    let rematch = second.rounds[1].tables[0].clone();
    let (corp2, runner2) = (index_of(&rematch.corp), index_of(&rematch.runner));
    send(&mut sockets[corp2], ClientMessage::OfferDraw { tournament: made.id.clone() }).await;
    assert_eq!(next_tournament(&mut sockets[corp2]).await.draw_offers.len(), 1);
    next_tournament(&mut sockets[runner2]).await;
    send(&mut sockets[corp2], ClientMessage::Sit { tournament: made.id.clone() }).await;
    assert!(matches!(next(&mut sockets[corp2]).await, ServerMessage::Queued { .. }));
    send(&mut sockets[runner2], ClientMessage::Sit { tournament: made.id.clone() }).await;
    next_joined(&mut sockets[runner2]).await;
    next_joined(&mut sockets[corp2]).await;
    send(&mut sockets[runner2], ClientMessage::OfferDraw { tournament: made.id.clone() }).await;
    { let reason = refused(&mut sockets[runner2]).await; assert!(reason.contains("under way"), "{reason}"); }
    send(&mut sockets[runner2], ClientMessage::ListTournaments).await;
    let listed = loop {
        match next(&mut sockets[runner2]).await {
            ServerMessage::Tournaments { tournaments } => break tournaments,
            _ => continue,
        }
    };
    assert!(listed[0].draw_offers.is_empty(), "the offer lapsed when the game started");
    let _ = std::fs::remove_dir_all(&dir);
}
