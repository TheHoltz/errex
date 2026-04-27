//! WebSocket fan-out server for connected clients.
//!
//! On port 9091, separate from the HTTP ingest server. tokio-tungstenite
//! directly is the smaller dep footprint vs axum's WS extractors.
//!
//! Snapshot on connect comes from a SQLite query — there is no in-memory
//! issue cache. WAL reads are sub-millisecond and skipping the cache saves
//! `N × Issue` bytes for free. The cost is one query per connecting client,
//! which is exactly when latency doesn't matter.

use std::net::SocketAddr;

use errex_proto::{ClientMessage, ServerMessage};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

use crate::error::DaemonError;
use crate::store::Store;

pub async fn serve(
    addr: SocketAddr,
    store: Store,
    fanout: broadcast::Sender<ServerMessage>,
) -> Result<(), DaemonError> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("ws fan-out bound to {addr}");

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(p) => p,
            Err(err) => {
                tracing::warn!(%err, "ws accept failed");
                continue;
            }
        };
        let rx = fanout.subscribe();
        let store = store.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream, store, rx).await {
                tracing::debug!(%peer, %err, "ws connection closed");
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    store: Store,
    mut fanout: broadcast::Receiver<ServerMessage>,
) -> Result<(), DaemonError> {
    let ws = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(|e| DaemonError::Io(std::io::Error::other(e)))?;
    let (mut write, mut read) = ws.split();

    // Greet, then catch the client up. Subscribing to `fanout` happened
    // before this snapshot was loaded, so any concurrent updates arrive
    // after it — possibly re-stating an issue already in the snapshot,
    // which the client handles idempotently by id.
    let issues = store.load_issues().await?;
    let hello = ServerMessage::Hello {
        server_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let snapshot = ServerMessage::Snapshot { issues };
    for msg in [hello, snapshot] {
        let payload = serde_json::to_string(&msg).expect("ServerMessage is always serializable");
        if write.send(Message::Text(payload)).await.is_err() {
            return Ok(());
        }
    }

    loop {
        tokio::select! {
            // Server-pushed updates.
            msg = fanout.recv() => match msg {
                Ok(m) => {
                    let payload = serde_json::to_string(&m)
                        .expect("ServerMessage is always serializable");
                    if write.send(Message::Text(payload)).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::debug!(skipped, "ws subscriber lagged");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },

            // Client-side traffic. We only honor pings today; richer commands
            // (resolve, mute) land here as the daemon grows.
            incoming = read.next() => match incoming {
                Some(Ok(Message::Text(t))) => {
                    if let Ok(ClientMessage::Ping) = serde_json::from_str::<ClientMessage>(&t) {
                        // No-op; the keepalive itself is the signal.
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {}
                Some(Err(_)) => break,
            },
        }
    }

    Ok(())
}
