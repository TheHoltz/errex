//! WebSocket fan-out for connected SPA clients — mounted on the same axum
//! listener as the HTTP API.
//!
//! Earlier versions ran a separate tokio-tungstenite server on :9091. In
//! production that broke the SPA: the browser builds the WS URL from
//! `location.host`, which equals the HTTP port the SPA was served from.
//! The upgrade request hit the HTTP listener, missed every API route, fell
//! through to the SPA fallback, and got `200 + index.html` instead of
//! `101 Switching Protocols`. Unifying onto one listener removes the
//! whole class of "wrong port" bugs and shaves a TCP listener off the
//! self-host footprint.
//!
//! Snapshot on connect comes from a SQLite query — there is no in-memory
//! issue cache. WAL reads are sub-millisecond and skipping the cache saves
//! `N × Issue` bytes for free. The cost is one query per connecting client,
//! which is exactly when latency doesn't matter.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use errex_proto::{ClientMessage, ServerMessage};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;

use crate::error::DaemonError;
use crate::ingest::AppState;
use crate::store::Store;

/// HTTP→WS upgrade handler mounted at `/ws/:project`. The project segment
/// is reserved for future per-project filtering — today the snapshot is
/// global and the SPA filters client-side, matching the previous server's
/// behavior. Keeping the segment in the URL means SDKs and operators can
/// already include it without a wire break later.
///
/// Authentication and origin pinning happen *before* `on_upgrade` so a
/// rejected request never opens the websocket. The snapshot leaks every
/// issue's title/culprit (and via the snapshot the SPA loads stack traces
/// for any of them), so the gate has to run on every connection.
pub async fn handle(
    upgrade: WebSocketUpgrade,
    Path(_project): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = crate::auth::require_auth(&state, &headers).await {
        return resp;
    }
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        if !origin_allowed(origin, &state.public_url, state.dev_mode) {
            return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
        }
    }
    let store = state.store.clone();
    let rx = state.fanout.subscribe();
    let metrics = state.metrics.clone();
    upgrade.on_upgrade(move |socket| async move {
        if let Err(err) = run(socket, store, rx, metrics).await {
            tracing::debug!(%err, "ws connection closed");
        }
    })
}

/// Allow the connection iff the Origin header's `scheme://host[:port]` matches
/// the configured `public_url`. In dev mode the Vite dev server origin is also
/// permitted so `bun run dev` on :5173 can drive the daemon on :9090.
///
/// Browsers always send Origin for WS upgrades, so a missing Origin only
/// happens for non-browser clients (CLIs, mobile SDKs) which are gated by
/// the cookie alone. A *present* but mismatched Origin is the attack
/// signature this rejects.
fn origin_allowed(origin: &str, public_url: &str, dev_mode: bool) -> bool {
    let normalize = |u: &str| -> Option<String> {
        let u = u.trim_end_matches('/');
        let scheme_end = u.find("://")?;
        let after_scheme = &u[scheme_end + 3..];
        let host_end = after_scheme.find('/').unwrap_or(after_scheme.len());
        Some(format!("{}://{}", &u[..scheme_end], &after_scheme[..host_end]))
    };
    let Some(public_origin) = normalize(public_url) else {
        return false;
    };
    if origin == public_origin {
        return true;
    }
    if dev_mode && origin == "http://localhost:5173" {
        return true;
    }
    false
}

async fn run(
    socket: WebSocket,
    store: Store,
    mut fanout: broadcast::Receiver<ServerMessage>,
    metrics: Arc<crate::metrics::Metrics>,
) -> Result<(), DaemonError> {
    let (mut write, mut read) = socket.split();

    // Subscribing happened before this snapshot loaded, so any concurrent
    // updates arrive after — possibly re-stating an issue already in the
    // snapshot, which the client handles idempotently by id.
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
                    metrics.add_ws_lagged(skipped);
                    tracing::warn!(skipped, "ws subscriber lagged");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },

            // Client-side traffic. Only honors pings today; richer commands
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_allowed_matches_public_url_exactly() {
        assert!(origin_allowed(
            "https://errex.example.com",
            "https://errex.example.com",
            false
        ));
        assert!(origin_allowed(
            "https://errex.example.com",
            "https://errex.example.com/",
            false
        ));
    }

    #[test]
    fn origin_allowed_strips_path_from_public_url() {
        assert!(origin_allowed(
            "https://errex.example.com",
            "https://errex.example.com/dashboard",
            false
        ));
    }

    #[test]
    fn origin_rejected_on_host_mismatch() {
        assert!(!origin_allowed(
            "https://attacker.example.com",
            "https://errex.example.com",
            false
        ));
    }

    #[test]
    fn origin_rejected_on_scheme_mismatch() {
        assert!(!origin_allowed(
            "http://errex.example.com",
            "https://errex.example.com",
            false
        ));
    }

    #[test]
    fn origin_rejected_on_port_mismatch() {
        assert!(!origin_allowed(
            "http://errex.example.com:8080",
            "http://errex.example.com:9090",
            false
        ));
    }

    #[test]
    fn vite_dev_origin_allowed_only_in_dev_mode() {
        assert!(origin_allowed(
            "http://localhost:5173",
            "http://localhost:9090",
            true
        ));
        assert!(!origin_allowed(
            "http://localhost:5173",
            "http://localhost:9090",
            false
        ));
    }
}
