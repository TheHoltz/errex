use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable identifier for a group of similar events.
///
/// The actual derivation (hash inputs, normalization) lives in `errexd::fingerprint`.
/// Here we just give callers a typed wrapper so we don't pass bare strings around.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fingerprint(pub String);

impl Fingerprint {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for Fingerprint {
    fn from(s: String) -> Self {
        Self(s)
    }
}
