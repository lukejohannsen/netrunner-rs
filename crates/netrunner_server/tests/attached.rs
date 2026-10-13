//! Attached connections over real WebSockets (Phase 4 §7): a client
//! attaches with no deck, browses, makes and joins lobbies, looks for a
//! game with the deck its chair needs, plays, and is back in its lobby for
//! the next game — all on one socket.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use netrunner_core::decks::{self, DeckFile};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;
use netrunner_bots::Level;
use netrunner_server::protocol::{Chair, LobbyInfo, Opponent, QueueMode};
use netrunner_server::serve::{ServeOptions, Server};
use netrunner_server::{ClientMessage, ServerMessage};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn human_daemon() -> String {
    let server = Server::bind("127.0.0.1:0", ServeOptions { seed: Some(1), ..ServeOptions::default() }).await.unwrap();
    let addr = server.local_addr().unwrap();
    tokio::spawn(server.run());
    format!("ws://{addr}")
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

/// The next message that `wanted` picks out, skipping the game's traffic.
async fn next_where<T>(socket: &mut Socket, wanted: impl Fn(ServerMessage) -> Option<T>) -> T {
    loop {
        if let Some(found) = wanted(next(socket).await) {
            return found;
        }
    }
}

/// Attaches as `name`, with no deck, and returns the open lobbies.
async fn attach(url: &str, name: &str) -> (Socket, Vec<LobbyInfo>) {
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    send(&mut socket, ClientMessage::Attach { player_name: name.into() }).await;
    let ServerMessage::Attached { lobbies } = next(&mut socket).await else { panic!("expected Attached") };
    (socket, lobbies)
}

async fn join(socket: &mut Socket, lobby: &str, password: Option<&str>) -> ServerMessage {
    send(socket, ClientMessage::JoinLobby { lobby: lobby.into(), password: password.map(Into::into) }).await;
    next(socket).await
}

fn deck(id: &str) -> Box<DeckFile> {
    Box::new(decks::by_id(id).expect("a built-in deck"))
}

fn joined(message: ServerMessage) -> (Side, String, String, uuid::Uuid) {
    match message {
        ServerMessage::MatchJoined { assigned_side, corp_deck, runner_deck, session_token, .. } => (assigned_side, corp_deck, runner_deck, session_token),
        other => panic!("expected MatchJoined, got {other:?}"),
    }
}

/// No deck to attach, the server's own queues listed — a Casual one per
/// format, and no Ranked one on a daemon that keeps no book — and a lobby
/// joined with no deck either.
#[tokio::test]
async fn a_client_attaches_and_joins_a_lobby_with_no_deck() {
    let url = human_daemon().await;
    let (mut socket, lobbies) = attach(&url, "browser").await;
    let ids: Vec<&str> = lobbies.iter().map(|lobby| lobby.id.as_str()).collect();
    assert_eq!(ids, ["casual-startup", "casual-standard", "casual-eternal", "casual-snapshot", "casual-unrestricted"], "`serve::DEFAULT_FORMATS`");
    assert!(lobbies.iter().all(|lobby| lobby.permanent() && lobby.mode == QueueMode::Casual && lobby.players() == 0));
    assert!(lobbies.iter().all(|lobby| !lobby.rated), "a daemon with no data directory rates nothing, and its lobbies say so");
    // The server's own queues offer no bot: a bot sits only in a friend
    // lobby that lets one in (12 October 2026).
    assert!(lobbies.iter().all(|lobby| lobby.bots.is_empty()), "{lobbies:?}");

    let ServerMessage::LobbyJoined { lobby } = join(&mut socket, "Casual-Standard", None).await else { panic!() };
    assert_eq!((lobby.format, lobby.players()), (NsgFormat::Standard, 1));
    let ServerMessage::LobbyRefused { reason } = join(&mut socket, "nowhere", None).await else { panic!() };
    assert!(reason.contains("no lobby NOWHERE"), "a code is read in upper case: {reason}");
}

/// A friend queue is never listed, is joined by its code, and asks for
/// its password; it goes when its last player leaves. The server's own
/// stay.
#[tokio::test]
async fn a_friend_queue_is_joined_by_its_code_and_password() {
    let url = human_daemon().await;
    let (mut host, _) = attach(&url, "host").await;
    send(&mut host, ClientMessage::CreateLobby { name: "Friday".into(), format: NsgFormat::Startup, password: Some("swordfish".into()), bots: false }).await;
    let ServerMessage::LobbyJoined { lobby } = next(&mut host).await else { panic!() };
    assert!(lobby.mode == QueueMode::Friends && lobby.password && !lobby.permanent() && !lobby.rated && lobby.bots.is_empty() && lobby.id.len() == 6, "{lobby:?}");

    let (mut friend, listed) = attach(&url, "friend").await;
    assert!(listed.iter().all(|listed| listed.id != lobby.id), "a friend queue is not listed");
    let ServerMessage::LobbyRefused { reason } = join(&mut friend, &lobby.id, None).await else { panic!() };
    assert!(reason.contains("password"), "{reason}");
    let ServerMessage::LobbyRefused { .. } = join(&mut friend, &lobby.id, Some("wrong")).await else { panic!() };
    // Typed in lower case, with a space: the id is the id.
    let typed = format!(" {} ", lobby.id.to_lowercase());
    let ServerMessage::LobbyJoined { lobby: inside } = join(&mut friend, &typed, Some("swordfish")).await else { panic!() };
    assert_eq!(inside.players(), 2);

    for socket in [&mut host, &mut friend] {
        send(socket, ClientMessage::LeaveLobby).await;
        assert!(matches!(next(socket).await, ServerMessage::LobbyLeft));
    }
    let ServerMessage::LobbyRefused { .. } = join(&mut friend, &lobby.id, Some("swordfish")).await else { panic!("gone when empty") };
}

/// A chair chosen brings its deck; a random chair brings both and plays
/// the one for the side it is given. The game ends and both are back in
/// the lobby, and look for the next game on the same socket with other
/// decks.
#[tokio::test]
async fn a_game_is_looked_for_with_decks_and_the_next_one_needs_no_reconnect() {
    let url = human_daemon().await;
    let (mut corp, _) = attach(&url, "corp").await;
    let (mut random, _) = attach(&url, "random").await;
    for socket in [&mut corp, &mut random] {
        let ServerMessage::LobbyJoined { .. } = join(socket, "casual-startup", None).await else { panic!() };
    }

    send(&mut corp, ClientMessage::Seek { chair: Chair::Corp(deck("brick_stack")), opponent: Opponent::Person }).await;
    assert!(matches!(next(&mut corp).await, ServerMessage::Queued { .. }));
    send(&mut random, ClientMessage::Seek { chair: Chair::Either { corp: deck("glyph_of_warding"), runner: deck("stolen_goods"), prefer: None }, opponent: Opponent::Person }).await;
    let (side, corp_deck, runner_deck, _) = joined(next(&mut random).await);
    assert_eq!((side, corp_deck.as_str(), runner_deck.as_str()), (Side::Runner, "", "stolen_goods"), "the random chair sits opposite, with its Runner deck");
    let (side, corp_deck, _, _) = joined(next_where(&mut corp, |message| matches!(message, ServerMessage::MatchJoined { .. }).then_some(message)).await);
    assert_eq!((side, corp_deck.as_str()), (Side::Corp, "brick_stack"));

    send(&mut corp, ClientMessage::Surrender).await;
    for socket in [&mut corp, &mut random] {
        next_where(socket, |message| matches!(message, ServerMessage::GameEnded { .. }).then_some(())).await;
        let lobby = next_where(socket, |message| match message {
            ServerMessage::BackInLobby { lobby } => Some(lobby),
            _ => None,
        })
        .await;
        assert_eq!(lobby.map(|lobby| lobby.id).as_deref(), Some("casual-startup"), "back in the lobby the game was found in");
    }

    // The next game, other chairs, other decks, the same sockets.
    send(&mut corp, ClientMessage::Seek { chair: Chair::Runner(deck("dashing_mad")), opponent: Opponent::Person }).await;
    assert!(matches!(next(&mut corp).await, ServerMessage::Queued { .. }));
    send(&mut random, ClientMessage::Seek { chair: Chair::Corp(deck("glyph_of_warding")), opponent: Opponent::Person }).await;
    let (side, corp_deck, _, _) = joined(next(&mut random).await);
    assert_eq!((side, corp_deck.as_str()), (Side::Corp, "glyph_of_warding"));
}

/// A seek needs a lobby, a deck for the chair's side and one the lobby's
/// format allows; a second seek waits for a cancel; a cancel and a dropped
/// socket each take the seek off the queue.
#[tokio::test]
async fn a_seek_is_checked_cancelled_and_withdrawn_with_its_socket() {
    let url = human_daemon().await;
    let (mut socket, _) = attach(&url, "seeker").await;
    send(&mut socket, ClientMessage::Seek { chair: Chair::Corp(deck("brick_stack")), opponent: Opponent::Person }).await;
    let ServerMessage::SeekRefused { reason } = next(&mut socket).await else { panic!() };
    assert!(reason.contains("join a lobby"), "{reason}");

    join(&mut socket, "casual-startup", None).await;
    send(&mut socket, ClientMessage::Seek { chair: Chair::Corp(deck("stolen_goods")), opponent: Opponent::Person }).await;
    let ServerMessage::SeekRefused { reason } = next(&mut socket).await else { panic!() };
    assert!(reason.contains("Runner deck"), "{reason}");

    send(&mut socket, ClientMessage::Seek { chair: Chair::Corp(deck("brick_stack")), opponent: Opponent::Person }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::Queued { .. }));
    send(&mut socket, ClientMessage::Seek { chair: Chair::Corp(deck("glyph_of_warding")), opponent: Opponent::Person }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::SeekRefused { .. }), "one seek at a time");
    send(&mut socket, ClientMessage::ListLobbies).await;
    let ServerMessage::Lobbies { lobbies } = next(&mut socket).await else { panic!() };
    assert_eq!(lobbies[0].seeking(), 1);
    send(&mut socket, ClientMessage::CancelSeek).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::SeekCancelled));

    send(&mut socket, ClientMessage::Seek { chair: Chair::Corp(deck("brick_stack")), opponent: Opponent::Person }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::Queued { .. }));
    drop(socket);
    // The next Runner finds nobody: the seek went with its socket.
    let (mut runner, _) = attach(&url, "runner").await;
    join(&mut runner, "casual-startup", None).await;
    let seeking = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            send(&mut runner, ClientMessage::ListLobbies).await;
            let ServerMessage::Lobbies { lobbies } = next(&mut runner).await else { panic!() };
            if lobbies[0].seeking() == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    seeking.await.expect("the dropped seek is withdrawn");
}

/// A seat resumed on a new socket is attached again: the game goes on
/// there, and when it ends that socket is back in the lobby.
#[tokio::test]
async fn a_resumed_seat_goes_back_to_its_lobby() {
    let url = human_daemon().await;
    let (mut corp, _) = attach(&url, "corp").await;
    let (mut runner, _) = attach(&url, "runner").await;
    for socket in [&mut corp, &mut runner] {
        join(socket, "casual-startup", None).await;
    }
    send(&mut corp, ClientMessage::Seek { chair: Chair::Corp(deck("brick_stack")), opponent: Opponent::Person }).await;
    next(&mut corp).await;
    send(&mut runner, ClientMessage::Seek { chair: Chair::Runner(deck("stolen_goods")), opponent: Opponent::Person }).await;
    let (_, _, _, token) = joined(next(&mut runner).await);
    drop(runner);

    let (mut back, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    send(&mut back, ClientMessage::Resume { session_token: token }).await;
    let (side, _, runner_deck, _) = joined(next(&mut back).await);
    assert_eq!((side, runner_deck.as_str()), (Side::Runner, "stolen_goods"));
    send(&mut back, ClientMessage::Surrender).await;
    let lobby = next_where(&mut back, |message| match message {
        ServerMessage::BackInLobby { lobby } => Some(lobby),
        _ => None,
    })
    .await;
    assert_eq!(lobby.map(|lobby| lobby.id).as_deref(), Some("casual-startup"));
}

fn either(prefer: Option<Side>) -> Chair {
    Chair::Either { corp: deck("glyph_of_warding"), runner: deck("stolen_goods"), prefer }
}

/// A preference seats the pair (Phase 4 §9): a waiter who prefers the
/// Runner's chair and a newcomer who prefers the Corp's sit as they asked,
/// and a newcomer who prefers the Corp's is paired with the waiter who
/// wants the Runner's rather than the first in line, who wants the Corp's
/// too.
#[tokio::test]
async fn preferences_choose_the_partner_and_the_chairs() {
    let url = human_daemon().await;
    let mut sockets = Vec::new();
    for name in ["early", "late", "newcomer"] {
        let (mut socket, _) = attach(&url, name).await;
        join(&mut socket, "casual-startup", None).await;
        sockets.push(socket);
    }
    let [early, late, newcomer] = &mut sockets[..] else { unreachable!() };
    send(early, ClientMessage::Seek { chair: either(Some(Side::Corp)), opponent: Opponent::Person }).await;
    assert!(matches!(next(early).await, ServerMessage::Queued { position: 1, .. }));
    send(late, ClientMessage::Seek { chair: either(Some(Side::Runner)), opponent: Opponent::Person }).await;
    // Two seeks for either chair are compatible, so the second pairs at
    // once — and the preferences decide who is the Corp.
    let ServerMessage::MatchJoined { assigned_side: late_side, .. } = next(late).await else { panic!() };
    let ServerMessage::MatchJoined { assigned_side: early_side, .. } = next(early).await else { panic!() };
    assert_eq!((early_side, late_side), (Side::Corp, Side::Runner));

    send(newcomer, ClientMessage::Seek { chair: either(Some(Side::Corp)), opponent: Opponent::Person }).await;
    assert!(matches!(next(newcomer).await, ServerMessage::Queued { position: 1, .. }));
}

/// A bot sits only where a casual lobby asks for one (Phase 4 §9): asked
/// in the Casual queue it is seated at once, in the chair preferred; a
/// friend queue seats one only if its maker let bots in; a rung the
/// server does not offer is refused, and a server offering none says so.
#[tokio::test]
async fn a_bot_sits_only_in_a_friend_lobby_that_allows_one() {
    let url = human_daemon().await;
    let elite = Opponent::Bot { level: Level::Elite, style: None };
    let (mut player, lobbies) = attach(&url, "player").await;
    assert!(lobbies.iter().all(|lobby| lobby.bots.is_empty()), "the server's own queues offer no bot: {lobbies:?}");
    join(&mut player, "casual-startup", None).await;
    send(&mut player, ClientMessage::Seek { chair: either(Some(Side::Runner)), opponent: elite }).await;
    let ServerMessage::SeekRefused { reason } = next(&mut player).await else { panic!("a bot never sits in the Casual queue") };
    assert!(reason.contains("friend lobby"), "{reason}");

    send(&mut player, ClientMessage::CreateLobby { name: "no bots".into(), format: NsgFormat::Startup, password: None, bots: false }).await;
    assert!(matches!(next(&mut player).await, ServerMessage::LobbyJoined { .. }));
    send(&mut player, ClientMessage::Seek { chair: either(None), opponent: elite }).await;
    let ServerMessage::SeekRefused { reason } = next(&mut player).await else { panic!() };
    assert!(reason.contains("kept bots out"), "{reason}");
    send(&mut player, ClientMessage::CreateLobby { name: "bots".into(), format: NsgFormat::Startup, password: None, bots: true }).await;
    let ServerMessage::LobbyJoined { lobby } = next(&mut player).await else { panic!() };
    assert_eq!(lobby.bots, Level::ALL.to_vec());
    send(&mut player, ClientMessage::Seek { chair: either(Some(Side::Runner)), opponent: elite }).await;
    let ServerMessage::MatchJoined { assigned_side, .. } = next(&mut player).await else { panic!("a friend lobby that lets bots in seats one at once") };
    assert_eq!(assigned_side, Side::Runner, "in the chair preferred");
    send(&mut player, ClientMessage::Surrender).await;
    loop {
        if let ServerMessage::BackInLobby { .. } = next(&mut player).await {
            break;
        }
    }
    send(&mut player, ClientMessage::Seek { chair: Chair::Corp(deck("brick_stack")), opponent: Opponent::Bot { level: Level::Novice, style: None } }).await;
    let ServerMessage::MatchJoined { assigned_side, .. } = next(&mut player).await else { panic!("and again after the game") };
    assert_eq!(assigned_side, Side::Corp);

    let server = Server::bind("127.0.0.1:0", ServeOptions { bots: vec![Level::Novice], ..ServeOptions::default() }).await.unwrap();
    let addr = server.local_addr().unwrap();
    tokio::spawn(server.run());
    let (mut picky, _) = attach(&format!("ws://{addr}"), "picky").await;
    send(&mut picky, ClientMessage::CreateLobby { name: "novices".into(), format: NsgFormat::Startup, password: None, bots: true }).await;
    let ServerMessage::LobbyJoined { lobby } = next(&mut picky).await else { panic!() };
    assert_eq!(lobby.bots, [Level::Novice], "a friend lobby offers what the server offers");
    send(&mut picky, ClientMessage::Seek { chair: either(None), opponent: elite }).await;
    let ServerMessage::SeekRefused { reason } = next(&mut picky).await else { panic!() };
    assert!(reason.contains("no elite bot"), "{reason}");
    let none = Server::bind("127.0.0.1:0", ServeOptions { bots: Vec::new(), ..ServeOptions::default() }).await.unwrap();
    let addr = none.local_addr().unwrap();
    tokio::spawn(none.run());
    let (mut alone, _) = attach(&format!("ws://{addr}"), "alone").await;
    send(&mut alone, ClientMessage::CreateLobby { name: "nobody".into(), format: NsgFormat::Startup, password: None, bots: true }).await;
    let ServerMessage::LobbyJoined { lobby } = next(&mut alone).await else { panic!() };
    assert!(lobby.bots.is_empty());
    send(&mut alone, ClientMessage::Seek { chair: either(None), opponent: elite }).await;
    let ServerMessage::SeekRefused { reason } = next(&mut alone).await else { panic!() };
    assert_eq!(reason, "this server offers no bot");
}
