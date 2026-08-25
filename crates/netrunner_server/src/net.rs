//! WebSocket framing for a `MatchSession`'s channel-backed `PlayerSlot`:
//! translates between typed `ClientMessage`/`ServerMessage` protocol values
//! and raw WebSocket text frames, so `run_serve` can hand a `MatchSession`
//! the exact same `PlayerSlot::Channel` it would build for an in-process
//! client — the session itself has no idea a network is involved.
//!
//! Server-shaped only: reads wire text as `ClientMessage`, writes
//! `ServerMessage` to the wire. `netrunner_cli::remote` needs the mirror
//! image (send `ClientMessage`, receive `ServerMessage`) and keeps its own
//! small bridge for that rather than reusing this one under swapped type
//! parameters — the message roles are baked in, not generic.

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::WebSocketStream;

use crate::protocol::{ClientMessage, ServerMessage};

pub async fn bridge_websocket<S>(
    ws_stream: WebSocketStream<S>,
    tx_to_session: mpsc::UnboundedSender<ClientMessage>,
    mut rx_from_session: mpsc::UnboundedReceiver<ServerMessage>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut ws_writer, mut ws_reader) = ws_stream.split();

    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx_from_session.recv().await {
            let Ok(json) = serde_json::to_string(&msg) else { continue };
            if ws_writer.send(WsMessage::Text(json)).await.is_err() {
                break;
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        while let Some(Ok(WsMessage::Text(text))) = ws_reader.next().await {
            if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text)
                && tx_to_session.send(client_msg).is_err()
            {
                break;
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}
