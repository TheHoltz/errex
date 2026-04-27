//! MCP (Model Context Protocol) server — STUB.
//!
//! The real implementation will expose issues/events/triage tools to AI agents
//! via the MCP protocol. For now we just bind the port and log incoming
//! connections so operators can verify the listener is reachable.

use std::net::SocketAddr;

use tokio::net::TcpListener;

use crate::error::DaemonError;

pub async fn serve(addr: SocketAddr) -> Result<(), DaemonError> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("mcp listener bound to {addr} (stub)");

    loop {
        match listener.accept().await {
            Ok((_stream, peer)) => {
                tracing::info!(%peer, "mcp connection received but server is a stub");
                // TODO: implement MCP server (resources: issues, events; tools:
                //       list_issues, get_event, set_status, run_triage).
            }
            Err(err) => {
                tracing::warn!(%err, "mcp accept failed");
            }
        }
    }
}
