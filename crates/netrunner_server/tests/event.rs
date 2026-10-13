//! Events over real WebSockets (Phase 4 §11): an operator's key holds a
//! rolling gauntlet on a daemon that keeps things, keys enter it and are
//! paired in its pool alone, a win moves the winner's track and a
//! milestone reached is a title or a badge in the profile at once, the
//! games are rated by nobody, and the event outlives the daemon.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use netrunner_core::card::Faction;
use netrunner_core::decks::{self, FactionRule};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;
use netrunner_identity::{Identity, PublicKey};
use netrunner_server::protocol::{event_lobby_id, AwardKind, AwardSpec, Chair, EventInfo, EventSpec, Milestone, Opponent, QueueMode};
use netrunner_server::serve::{ServeOptions, Server};
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
    let dir = std::env::temp_dir().join(format!("netrunner_event_{name}_{}", std::process::id()));
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

async fn next_where<T>(socket: &mut Socket, wanted: impl Fn(ServerMessage) -> Option<T>) -> T {
    loop {
        if let Some(found) = wanted(next(socket).await) {
            return found;
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

fn gauntlet() -> EventSpec {
    EventSpec {
        name: "The Red Planet Gauntlet".into(),
        format: NsgFormat::Startup,
        rule: Some(FactionRule { corp: vec![Faction::WeylandConsortium, Faction::Jinteki], runner: Vec::new() }),
        wins_to_clear: 2,
        milestones: vec![
            Milestone { wins: 1, award: AwardSpec { kind: AwardKind::Title, name: "Net-Slinger".into() } },
            Milestone { wins: 2, award: AwardSpec { kind: AwardKind::Badge, name: "red-planet".into() } },
        ],
    }
}

async fn hold(socket: &mut Socket, spec: EventSpec) -> EventInfo {
    send(socket, ClientMessage::HoldEvent { event: spec }).await;
    match next(socket).await {
        ServerMessage::Event { event } => event,
        other => panic!("expected Event, got {other:?}"),
    }
}

async fn refused(socket: &mut Socket) -> String {
    next_where(socket, |message| match message {
        ServerMessage::EventRefused { reason } | ServerMessage::SeekRefused { reason } => Some(reason),
        _ => None,
    })
    .await
}

async fn enter(socket: &mut Socket, code: &str) -> EventInfo {
    send(socket, ClientMessage::EnterEvent { event: code.into() }).await;
    match next(socket).await {
        ServerMessage::Event { event } => event,
        other => panic!("expected Event, got {other:?}"),
    }
}

fn weyland_and_runner() -> Chair {
    Chair::Either { corp: Box::new(decks::by_id("brick_stack").unwrap()), runner: Box::new(decks::by_id("dashing_mad").unwrap()), prefer: None }
}

async fn join_pool(socket: &mut Socket, code: &str) {
    send(socket, ClientMessage::JoinLobby { lobby: event_lobby_id(code), password: None }).await;
    let ServerMessage::LobbyJoined { lobby } = next(socket).await else { panic!("expected LobbyJoined") };
    assert_eq!((lobby.mode, lobby.rated, lobby.bots.is_empty(), lobby.name.as_str()), (QueueMode::Event, false, true, "The Red Planet Gauntlet"), "{lobby:?}");
}

/// Both in the pool: `winner` seeks first, both are seated, the other
/// concedes. Each gets `Event` pushed with their track moved; the
/// winner's is returned.
async fn play(winner: &mut Socket, loser: &mut Socket) -> (EventInfo, EventInfo) {
    send(winner, ClientMessage::Seek { chair: weyland_and_runner(), opponent: Opponent::Person }).await;
    assert!(matches!(next(winner).await, ServerMessage::Queued { position: 1, .. }));
    send(loser, ClientMessage::Seek { chair: weyland_and_runner(), opponent: Opponent::Person }).await;
    assert!(matches!(next(loser).await, ServerMessage::MatchJoined { .. }));
    assert!(matches!(next(winner).await, ServerMessage::MatchJoined { .. }));
    send(loser, ClientMessage::Surrender).await;
    let event = |message: ServerMessage| match message {
        ServerMessage::Event { event } => Some(event),
        ServerMessage::Rated { .. } => panic!("an event's game is rated by nobody"),
        _ => None,
    };
    let won = next_where(winner, event).await;
    let lost = next_where(loser, event).await;
    (won, lost)
}

async fn profile(socket: &mut Socket) -> netrunner_server::protocol::Profile {
    send(socket, ClientMessage::MyProfile).await;
    next_where(socket, |message| match message {
        ServerMessage::Profile { profile, .. } => Some(profile.expect("kept")),
        _ => None,
    })
    .await
}

#[tokio::test]
async fn an_event_is_held_by_an_operator_on_a_daemon_that_keeps_things() {
    let (op, ann) = (player(1), player(2));
    let stateless = start(None, vec![op.public_key()]).await;
    let mut socket = attach(&stateless, Some(&op), "op").await;
    send(&mut socket, ClientMessage::HoldEvent { event: gauntlet() }).await;
    assert!(refused(&mut socket).await.contains("keeps nothing"));

    let dir = scratch("held");
    let url = start(Some(dir.clone()), vec![op.public_key()]).await;
    let mut nobody = attach(&url, None, "nobody").await;
    send(&mut nobody, ClientMessage::HoldEvent { event: gauntlet() }).await;
    assert!(refused(&mut nobody).await.contains("prove one"));
    let mut ann_socket = attach(&url, Some(&ann), "ann").await;
    send(&mut ann_socket, ClientMessage::HoldEvent { event: gauntlet() }).await;
    assert!(refused(&mut ann_socket).await.contains("operator"), "a key that is not the operator's holds nothing");
    send(&mut ann_socket, ClientMessage::ListEvents).await;
    let ServerMessage::Events { events, operator } = next(&mut ann_socket).await else { panic!("expected Events") };
    assert!(events.is_empty() && !operator);

    let mut op_socket = attach(&url, Some(&op), "op").await;
    send(&mut op_socket, ClientMessage::HoldEvent { event: EventSpec { milestones: vec![Milestone { wins: 5, award: AwardSpec { kind: AwardKind::Title, name: "x".into() } }], ..gauntlet() } }).await;
    assert!(refused(&mut op_socket).await.contains("off a track"), "a milestone past the track is refused");
    let held = hold(&mut op_socket, gauntlet()).await;
    assert_eq!((held.name.as_str(), held.format, held.wins_to_clear, held.open, held.entered, held.looking, held.mine.is_none()), ("The Red Planet Gauntlet", NsgFormat::Startup, 2, true, 0, 0, true));
    assert_eq!(held.id.len(), 6);
    send(&mut op_socket, ClientMessage::ListEvents).await;
    let ServerMessage::Events { events, operator } = next(&mut op_socket).await else { panic!("expected Events") };
    assert!(operator && events.len() == 1 && events[0].id == held.id);
    // Never among the queues: the events page is the way in.
    send(&mut ann_socket, ClientMessage::ListLobbies).await;
    let ServerMessage::Lobbies { lobbies } = next(&mut ann_socket).await else { panic!("expected Lobbies") };
    assert!(lobbies.iter().all(|lobby| lobby.mode != QueueMode::Event));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn entrants_are_paired_in_the_pool_alone_the_winners_track_moves_and_milestones_are_granted() {
    let (op, ann, bo) = (player(1), player(2), player(3));
    let dir = scratch("pool");
    let url = start(Some(dir.clone()), vec![op.public_key()]).await;
    let mut op_socket = attach(&url, Some(&op), "op").await;
    let code = hold(&mut op_socket, gauntlet()).await.id;

    // Entering needs a key; seeking in the pool needs an entry and a deck
    // the rule admits.
    let mut nobody = attach(&url, None, "nobody").await;
    send(&mut nobody, ClientMessage::EnterEvent { event: code.clone() }).await;
    assert!(refused(&mut nobody).await.contains("prove one"));
    let mut ann_socket = attach(&url, Some(&ann), "ann").await;
    join_pool(&mut ann_socket, &code).await;
    send(&mut ann_socket, ClientMessage::Seek { chair: weyland_and_runner(), opponent: Opponent::Person }).await;
    assert_eq!(refused(&mut ann_socket).await, "enter the event first");
    let entered = enter(&mut ann_socket, &code).await;
    assert_eq!((entered.entered, entered.mine.as_ref().map(|mine| (mine.wins, mine.losses))), (1, Some((0, 0))));
    send(&mut ann_socket, ClientMessage::EnterEvent { event: code.clone() }).await;
    assert!(refused(&mut ann_socket).await.contains("already"));
    let nbn = Chair::Either { corp: Box::new(decks::by_id("ad_nihilum").unwrap()), runner: Box::new(decks::by_id("dashing_mad").unwrap()), prefer: None };
    send(&mut ann_socket, ClientMessage::Seek { chair: nbn, opponent: Opponent::Person }).await;
    let reason = refused(&mut ann_socket).await;
    assert!(reason.contains("is NBN") && reason.contains("Jinteki or Weyland Consortium Corps"), "{reason}");
    send(&mut ann_socket, ClientMessage::Seek { chair: weyland_and_runner(), opponent: Opponent::Bot { level: netrunner_bots::Level::Novice, style: None } }).await;
    assert!(refused(&mut ann_socket).await.contains("no bot"));

    // A seek in the Casual queue never meets one in the pool.
    let mut bo_socket = attach(&url, Some(&bo), "bo").await;
    enter(&mut bo_socket, &code).await;
    join_pool(&mut bo_socket, &code).await;
    send(&mut bo_socket, ClientMessage::Seek { chair: weyland_and_runner(), opponent: Opponent::Person }).await;
    assert!(matches!(next(&mut bo_socket).await, ServerMessage::Queued { position: 1, .. }));
    let mut casual = attach(&url, Some(&player(4)), "cy").await;
    send(&mut casual, ClientMessage::JoinLobby { lobby: "casual-startup".into(), password: None }).await;
    assert!(matches!(next(&mut casual).await, ServerMessage::LobbyJoined { .. }));
    send(&mut casual, ClientMessage::Seek { chair: weyland_and_runner(), opponent: Opponent::Person }).await;
    assert!(matches!(next(&mut casual).await, ServerMessage::Queued { position: 1, .. }), "the pool is its own queue");
    send(&mut bo_socket, ClientMessage::CancelSeek).await;
    assert!(matches!(next(&mut bo_socket).await, ServerMessage::SeekCancelled));
    send(&mut ann_socket, ClientMessage::ListEvents).await;
    let ServerMessage::Events { events, .. } = next(&mut ann_socket).await else { panic!("expected Events") };
    assert_eq!((events[0].entered, events[0].looking), (2, 0));

    // Ann wins twice over Bo: the title at one, the badge at two, both
    // in the profile at once; Bo's losses count and cost nothing.
    let (won, lost) = play(&mut ann_socket, &mut bo_socket).await;
    assert_eq!(won.mine.as_ref().map(|mine| (mine.wins, mine.losses, mine.cleared_at.is_some())), Some((1, 0, false)));
    assert_eq!(lost.mine.as_ref().map(|mine| (mine.wins, mine.losses)), Some((0, 1)));
    for socket in [&mut ann_socket, &mut bo_socket] {
        next_where(socket, |message| matches!(message, ServerMessage::BackInLobby { .. }).then_some(())).await;
    }
    let ann_profile = profile(&mut ann_socket).await;
    assert_eq!(ann_profile.titles.iter().map(|award| (award.name.as_str(), award.source.as_str())).collect::<Vec<_>>(), [("Net-Slinger", event_lobby_id(&code).as_str())]);
    assert!(ann_profile.standings.is_empty(), "rated by nobody");
    let (won, _) = play(&mut ann_socket, &mut bo_socket).await;
    assert_eq!(won.mine.as_ref().map(|mine| (mine.wins, mine.losses, mine.cleared_at.is_some())), Some((2, 0, true)), "cleared at the second win");
    for socket in [&mut ann_socket, &mut bo_socket] {
        next_where(socket, |message| matches!(message, ServerMessage::BackInLobby { .. }).then_some(())).await;
    }
    let ann_profile = profile(&mut ann_socket).await;
    assert_eq!(ann_profile.badges.iter().map(|award| award.name.as_str()).collect::<Vec<_>>(), ["red-planet"]);
    // A cleared entrant keeps playing and is granted nothing more.
    let (won, _) = play(&mut ann_socket, &mut bo_socket).await;
    assert_eq!(won.mine.as_ref().map(|mine| mine.wins), Some(3));
    for socket in [&mut ann_socket, &mut bo_socket] {
        next_where(socket, |message| matches!(message, ServerMessage::BackInLobby { .. }).then_some(())).await;
    }
    assert_eq!(profile(&mut ann_socket).await.badges.len(), 1);

    // Quitting drops the track and keeps the awards; entering again
    // starts over.
    send(&mut ann_socket, ClientMessage::QuitEvent { event: code.clone() }).await;
    let ServerMessage::Event { event } = next(&mut ann_socket).await else { panic!("expected Event") };
    assert!(event.mine.is_none() && event.entered == 1);
    let again = enter(&mut ann_socket, &code).await;
    assert_eq!(again.mine.map(|mine| mine.wins), Some(0));
    assert_eq!(profile(&mut ann_socket).await.titles.len(), 1, "what was earned stays earned");

    // The operator closes it: Bo, attached and entered, is told; nobody
    // enters or seeks from now; the page is still listed.
    send(&mut op_socket, ClientMessage::CloseEvent { event: code.clone() }).await;
    let ServerMessage::Event { event } = next(&mut op_socket).await else { panic!("expected Event") };
    assert!(!event.open);
    let pushed = next_where(&mut bo_socket, |message| match message {
        ServerMessage::Event { event } => Some(event),
        _ => None,
    })
    .await;
    assert!(!pushed.open && pushed.mine.is_some());
    let mut late = attach(&url, Some(&player(5)), "dee").await;
    send(&mut late, ClientMessage::EnterEvent { event: code.clone() }).await;
    assert_eq!(refused(&mut late).await, "this event is closed");
    send(&mut bo_socket, ClientMessage::Seek { chair: weyland_and_runner(), opponent: Opponent::Person }).await;
    assert_eq!(refused(&mut bo_socket).await, "this event is closed");
    send(&mut late, ClientMessage::ListEvents).await;
    let ServerMessage::Events { events, .. } = next(&mut late).await else { panic!("expected Events") };
    assert_eq!(events.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Waits until the store in `dir` holds event `code` with `needle` in its
/// document: the writer lands a change a moment after the players are
/// told of it.
async fn stored(dir: &Path, code: &str, needle: &str) {
    let store = netrunner_store::Store::open(&netrunner_store::sqlite_url(&dir.join(netrunner_store::SQLITE_FILE))).await.unwrap();
    for _ in 0..100 {
        if store.events().await.unwrap().iter().any(|row| row.id == code && row.document.contains(needle)) {
            store.close().await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the store never held event {code} with {needle:?}");
}

#[tokio::test]
async fn an_event_and_its_tracks_outlive_the_daemon_and_a_rebuild() {
    let (op, ann) = (player(1), player(2));
    let dir = scratch("outlives");
    let url = start(Some(dir.clone()), vec![op.public_key()]).await;
    let mut op_socket = attach(&url, Some(&op), "op").await;
    let code = hold(&mut op_socket, gauntlet()).await.id;
    let mut ann_socket = attach(&url, Some(&ann), "ann").await;
    enter(&mut ann_socket, &code).await;
    stored(&dir, &code, &ann.public_key().rating_id()).await;
    drop((op_socket, ann_socket));

    let rebuilt = netrunner_server::serve::rebuild(&dir, None).await.unwrap();
    assert_eq!(rebuilt, 0, "no rated game to fold, and the event is kept");
    let url = start(Some(dir.clone()), Vec::new()).await;
    let mut ann_socket = attach(&url, Some(&ann), "ann").await;
    send(&mut ann_socket, ClientMessage::ListEvents).await;
    let ServerMessage::Events { events, operator } = next(&mut ann_socket).await else { panic!("expected Events") };
    assert_eq!((events.len(), operator), (1, false));
    assert_eq!((events[0].id.as_str(), events[0].mine.as_ref().map(|mine| mine.wins)), (code.as_str(), Some(0)), "still entered after the restart");
    assert_eq!(events[0].rule.as_ref().map(|rule| rule.corp.len()), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_closed_lobby_side_never_answers_the_other_side_of_the_rule() {
    // The rule narrows the Corp alone: any Runner identity passes it.
    let rule = gauntlet().rule.unwrap();
    assert_eq!(rule.allowed(Side::Runner), None);
    assert!(rule.problem_with(Side::Runner, Faction::Anarch, "Any").is_none());
    assert_eq!(rule.problem_with(Side::Corp, Faction::Nbn, "Reality Plus"), Some("Reality Plus is NBN; this event takes Weyland Consortium or Jinteki Corps".into()));
}
