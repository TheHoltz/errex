//! Shared wire types between `errexd` (daemon) and `errex` (TUI client).
//!
//! Kept deliberately small: only what crosses crate boundaries lives here.

pub mod error;
pub mod event;
pub mod fingerprint;
pub mod issue;
pub mod wire;

pub use error::ProtoError;
pub use event::{Event, ExceptionContainer, ExceptionInfo, Frame, Level, Stacktrace};
pub use fingerprint::Fingerprint;
pub use issue::{Issue, IssueStatus};
pub use wire::{ClientMessage, ServerMessage};
