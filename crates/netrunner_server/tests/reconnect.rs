//! The reconnection handshake over real WebSockets: `Connect`, drop the
//! socket, `Resume` with the token, get the seat and a fresh view back.
//! `MatchSession`'s own tests cover the channel-level contract; this is
//! the only place the token registry and the bridge's lifetime are
//! exercised the way a remote client exercises them.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use uuid::Uuid;

use netrunner_core::rules::{PlayerAction, Side};
use netrunner_server::serve::{ServeBotKind, ServeOptions, Server};
use netrunner_server::{ClientMessage, ServerMessage};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn start_server(grace: Duration) -> String {
    let options = ServeOptions { bot_runner: ServeBotKind::Heuristic, seed: Some(1), reconnect_grace: grace };
    let server = Server::bind("127.0.0.1:0", options).await.expect("an ephemeral port binds");
    let addr = server.local_addr().unwrap();
    tokio::spawn(server.run());
    format!("ws://{addr}")
}

async fn open(url: &str, hello: ClientMessage) -> Socket {
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.expect("the server accepts");
    socket.send(WsMessage::Text(serde_json::to_string(&hello).unwrap())).await.unwrap();
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

fn joined(message: ServerMessage) -> (Uuid, Side, Uuid) {
    match message {
        ServerMessage::MatchJoined { match_id, assigned_side, session_token } => (match_id, assigned_side, session_token),
        other => panic!("expected MatchJoined, got {other:?}"),
    }
}

#[tokio::test]
async fn a_dropped_client_resumes_its_seat_with_the_session_token() {
    let url = start_server(Duration::from_secs(30)).await;

    let mut first = open(&url, ClientMessage::Connect { player_name: "first".into(), preferred_side: Some(Side::Corp) }).await;
    let (match_id, side, token) = joined(next(&mut first).await);
    assert_eq!(side, Side::Corp);
    assert!(matches!(next(&mut first).await, ServerMessage::StateUpdate(_)));
    first.close(None).await.unwrap();
    drop(first);

    let mut second = open(&url, ClientMessage::Resume { session_token: token }).await;
    let (resumed_match, resumed_side, resumed_token) = joined(next(&mut second).await);
    assert_eq!((resumed_match, resumed_side, resumed_token), (match_id, side, token), "the same seat, same credential");
    let view = match next(&mut second).await {
        ServerMessage::StateUpdate(view) => view,
        other => panic!("expected the resync StateUpdate, got {other:?}"),
    };
    assert!(view.legal_actions.contains(&PlayerAction::KeepHand), "the Corp's mulligan is still pending");

    send(&mut second, ClientMessage::SubmitAction(PlayerAction::KeepHand)).await;
    assert!(matches!(next(&mut second).await, ServerMessage::StateUpdate(_)));
    assert!(matches!(next(&mut second).await, ServerMessage::ActionLog(_)));
}

#[tokio::test]
async fn a_token_nobody_issued_is_refused() {
    let url = start_server(Duration::from_secs(30)).await;
    let mut socket = open(&url, ClientMessage::Resume { session_token: Uuid::new_v4() }).await;
    assert!(matches!(next(&mut socket).await, ServerMessage::ResumeRejected { .. }));
}

#[tokio::test]
async fn a_seat_forfeited_for_staying_away_cannot_resume() {
    let url = start_server(Duration::from_millis(200)).await;

    let mut first = open(&url, ClientMessage::Connect { player_name: "first".into(), preferred_side: Some(Side::Corp) }).await;
    let (_, _, token) = joined(next(&mut first).await);
    assert!(matches!(next(&mut first).await, ServerMessage::StateUpdate(_)));
    first.close(None).await.unwrap();
    drop(first);

    tokio::time::sleep(Duration::from_millis(600)).await;
    let mut late = open(&url, ClientMessage::Resume { session_token: token }).await;
    assert!(matches!(next(&mut late).await, ServerMessage::ResumeRejected { .. }), "the ticket went with the match");
}

/// A client that reconnects while its old socket is still open takes the
/// seat; the old socket is closed by the host rather than left holding a
/// stale channel.
#[tokio::test]
async fn the_newest_connection_wins_the_seat() {
    let url = start_server(Duration::from_secs(30)).await;

    let mut first = open(&url, ClientMessage::Connect { player_name: "first".into(), preferred_side: Some(Side::Corp) }).await;
    let (_, _, token) = joined(next(&mut first).await);
    assert!(matches!(next(&mut first).await, ServerMessage::StateUpdate(_)));

    let mut second = open(&url, ClientMessage::Resume { session_token: token }).await;
    joined(next(&mut second).await);
    assert!(matches!(next(&mut second).await, ServerMessage::StateUpdate(_)));

    let ended = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(frame) = first.next().await {
            if matches!(frame, Ok(WsMessage::Close(_)) | Err(_)) {
                break;
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the displaced socket is closed by the host");

    send(&mut second, ClientMessage::SubmitAction(PlayerAction::KeepHand)).await;
    assert!(matches!(next(&mut second).await, ServerMessage::StateUpdate(_)));
}
