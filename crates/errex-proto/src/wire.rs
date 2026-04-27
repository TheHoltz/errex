use serde::{Deserialize, Serialize};

use crate::issue::Issue;

/// Messages sent from `errexd` to a connected `errex` TUI client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Initial greeting; carries the daemon version.
    Hello { server_version: String },
    /// Snapshot of all currently-known issues, sent right after `Hello` so a
    /// freshly-connected TUI can catch up on events that arrived before it
    /// joined.
    Snapshot { issues: Vec<Issue> },
    /// A new issue was created (first event matching this fingerprint).
    IssueCreated { issue: Issue },
    /// An existing issue saw another event.
    IssueUpdated { issue: Issue },
}

/// Messages sent from a TUI client to `errexd`.
///
/// Currently empty save for a heartbeat; richer commands (resolve, mute,
/// triage) will land here as the daemon grows.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Ping,
}
