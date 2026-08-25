//! Connects to a remote `netrunner_server --serve` daemon over WebSocket
//! and hands back the same `mpsc::UnboundedSender<ClientMessage>`/
//! `UnboundedReceiver<ServerMessage>` pair that `tui::run`'s local mode
//! gets from an in-process `MatchSession` — `App` doesn't distinguish
//! between the two.
//!
//! Deliberately its own small bridge rather than
//! `netrunner_server::net::bridge_websocket`: that helper is server-shaped
//! (reads the wire as `ClientMessage`, writes `ServerMessage`), which is
//! backwards for a client that sends `ClientMessage` and receives
//! `ServerMessage`.

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use netrunner_core::rules::Side;
use netrunner_server::{ClientMessage, ServerMessage};

pub async fn connect_remote(
    url: &str,
    preferred_side: Option<Side>,
) -> Result<(mpsc::UnboundedSender<ClientMessage>, mpsc::UnboundedReceiver<ServerMessage>), Box<dyn std::error::Error>> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(url).await?;
    let (mut ws_writer, mut ws_reader) = ws_stream.split();

    let (tx_to_server, mut rx_to_server) = mpsc::unbounded_channel::<ClientMessage>();
    let (tx_from_server, rx_from_server) = mpsc::unbounded_channel::<ServerMessage>();

    tokio::spawn(async move {
        while let Some(msg) = rx_to_server.recv().await {
            let Ok(json) = serde_json::to_string(&msg) else { continue };
            if ws_writer.send(WsMessage::Text(json)).await.is_err() {
                break;
            }
        }
    });

    tokio::spawn(async move {
        while let Some(Ok(WsMessage::Text(text))) = ws_reader.next().await {
            if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&text)
                && tx_from_server.send(server_msg).is_err()
            {
                break;
            }
        }
    });

    tx_to_server.send(ClientMessage::Connect { player_name: "CLI Player".into(), preferred_side })?;
    Ok((tx_to_server, rx_from_server))
}
