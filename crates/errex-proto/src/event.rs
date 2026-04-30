use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Sentry SDKs send `timestamp` either as an RFC 3339 string or as a Unix
/// epoch number in seconds (integer or float, sub-second precision). The
/// default chrono deserializer only accepts the string form, which silently
/// 400s every error envelope from the browser SDK. Accept both shapes here.
fn deserialize_sentry_timestamp<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    match Value::deserialize(deserializer)? {
        Value::String(s) => DateTime::parse_from_rfc3339(&s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(D::Error::custom),
        Value::Number(n) => {
            let secs_f = n
                .as_f64()
                .ok_or_else(|| D::Error::custom("non-finite timestamp number"))?;
            let whole = secs_f.trunc() as i64;
            let frac = secs_f - secs_f.trunc();
            let nanos = (frac * 1_000_000_000.0).round().clamp(0.0, 999_999_999.0) as u32;
            DateTime::from_timestamp(whole, nanos)
                .ok_or_else(|| D::Error::custom("timestamp out of range"))
        }
        other => Err(D::Error::custom(format!(
            "expected string or number for timestamp, got {}",
            match other {
                Value::Null => "null",
                Value::Bool(_) => "bool",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
                _ => "unknown",
            }
        ))),
    }
}

/// A single error event coming from a Sentry-compatible SDK.
///
/// We model only the fields the scaffold needs. Unknown fields are dropped
/// rather than retained: `serde(deny_unknown_fields)` would be too strict for
/// real SDK payloads, and a passthrough JSON blob would defeat the type. If
/// raw payload preservation is needed, add a `raw: serde_json::Value` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    #[serde(default = "Uuid::new_v4")]
    pub event_id: Uuid,

    #[serde(
        default = "Utc::now",
        deserialize_with = "deserialize_sentry_timestamp"
    )]
    pub timestamp: DateTime<Utc>,

    #[serde(default)]
    pub platform: Option<String>,

    #[serde(default)]
    pub level: Option<Level>,

    #[serde(default)]
    pub environment: Option<String>,

    #[serde(default)]
    pub release: Option<String>,

    #[serde(default)]
    pub server_name: Option<String>,

    #[serde(default)]
    pub message: Option<String>,

    #[serde(default)]
    pub exception: Option<ExceptionContainer>,

    // Sentry SDKs include these but errexd does not yet model them as typed
    // structs. We retain the raw JSON so the SPA can render them; once the
    // daemon needs to query/index these (alerts on tags, breadcrumb search),
    // we'll graduate them to typed fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breadcrumbs: Option<Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contexts: Option<Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<Value>,
}

impl Event {
    /// First exception in the chain, if any. Sentry payloads put the
    /// most-derived (innermost) exception last by convention; we leave that
    /// ordering to callers.
    pub fn primary_exception(&self) -> Option<&ExceptionInfo> {
        self.exception.as_ref().and_then(|c| c.values.first())
    }

    /// Convenient title used by the issue grouper and TUI.
    pub fn title(&self) -> String {
        if let Some(ex) = self.primary_exception() {
            match (&ex.ty, &ex.value) {
                (Some(t), Some(v)) => format!("{t}: {v}"),
                (Some(t), None) => t.clone(),
                (None, Some(v)) => v.clone(),
                (None, None) => "Unknown exception".to_string(),
            }
        } else if let Some(msg) = &self.message {
            msg.clone()
        } else {
            "Unknown event".to_string()
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Debug,
    Info,
    Warning,
    Error,
    Fatal,
}

/// Sentry wraps exceptions in `{ "values": [...] }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionContainer {
    #[serde(default)]
    pub values: Vec<ExceptionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionInfo {
    #[serde(rename = "type", default)]
    pub ty: Option<String>,

    #[serde(default)]
    pub value: Option<String>,

    #[serde(default)]
    pub module: Option<String>,

    #[serde(default)]
    pub stacktrace: Option<Stacktrace>,
}

impl ExceptionInfo {
    pub fn first_frame(&self) -> Option<&Frame> {
        // Sentry orders frames oldest-first; the "first interesting" frame is
        // typically the last one in the list.
        self.stacktrace.as_ref().and_then(|s| s.frames.last())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stacktrace {
    #[serde(default)]
    pub frames: Vec<Frame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    #[serde(default)]
    pub filename: Option<String>,

    #[serde(default)]
    pub function: Option<String>,

    #[serde(default)]
    pub module: Option<String>,

    #[serde(default)]
    pub lineno: Option<u32>,

    #[serde(default)]
    pub colno: Option<u32>,

    #[serde(default)]
    pub in_app: Option<bool>,
}
