use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error(transparent)]
    Proto(#[from] errex_proto::ProtoError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// 404-equivalent: a row identified by the caller does not exist.
    /// Maps to HTTP 404 in the API layer; surfaces from `Store::set_status`
    /// and similar mutators that take an id from the URL.
    #[error("not found: {0}")]
    NotFound(String),

    /// Catch-all for crypto/parse/etc. failures whose underlying error type
    /// we don't want to leak through `From`. Constructed via `Crypto(msg)`.
    #[error("{0}")]
    Crypto(String),
}
