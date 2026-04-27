use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;

/// Daemon configuration. Every flag also reads from an `ERREXD_*` env var so
/// `docker compose` can configure without overriding the entrypoint.
#[derive(Debug, Clone, Parser)]
#[command(name = "errexd", version, about = "errex error-tracking daemon")]
pub struct Config {
    /// Directory holding the SQLite database (and future data files).
    #[arg(long, env = "ERREXD_DATA_DIR", default_value = "./data")]
    pub data_dir: PathBuf,

    /// Bind address for the HTTP ingest server.
    #[arg(long, env = "ERREXD_HTTP_HOST", default_value = "0.0.0.0")]
    pub http_host: String,

    #[arg(long, env = "ERREXD_HTTP_PORT", default_value_t = 9090)]
    pub http_port: u16,

    /// Bind address for the MCP server (AI agents). Stub for now.
    #[arg(long, env = "ERREXD_MCP_HOST", default_value = "0.0.0.0")]
    pub mcp_host: String,

    #[arg(long, env = "ERREXD_MCP_PORT", default_value_t = 9092)]
    pub mcp_port: u16,

    /// Logging level when RUST_LOG is unset.
    #[arg(long, env = "ERREXD_LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    /// When true, the HTTP server permits CORS requests from the Vite dev
    /// server (http://localhost:5173). Off in production so the embedded SPA
    /// is the only browser surface.
    #[arg(long, env = "ERREXD_DEV_MODE", default_value_t = false)]
    pub dev_mode: bool,

    /// When true, the ingest endpoint requires a `sentry_key` matching the
    /// configured project's token (see `errexd project add`). Off by default
    /// — self-host pequeno typically runs behind a private network where
    /// the daemon is unreachable from the public internet.
    #[arg(long, env = "ERREXD_REQUIRE_AUTH", default_value_t = false)]
    pub require_auth: bool,

    /// Days of event payload retention. The daemon purges older events
    /// hourly. Issue rows (counts, first/last seen) are kept regardless.
    /// `0` disables retention — events live forever until the disk fills.
    #[arg(long, env = "ERREXD_RETENTION_DAYS", default_value_t = 30)]
    pub retention_days: u32,

    /// Per-project ingest rate cap, events per minute. `0` = unlimited.
    /// The bucket also has burst capacity (`rate_limit_burst`) so short
    /// spikes don't get truncated; only sustained over-rate is rejected.
    #[arg(long, env = "ERREXD_RATE_LIMIT_PER_MIN", default_value_t = 0)]
    pub rate_limit_per_min: u32,

    /// Burst capacity for the per-project rate limiter. Ignored when
    /// `rate_limit_per_min == 0`.
    #[arg(long, env = "ERREXD_RATE_LIMIT_BURST", default_value_t = 200)]
    pub rate_limit_burst: u32,

    /// Externally-reachable base URL of this daemon. Embedded in webhook
    /// payloads + DSNs returned to the SPA so SDKs configured by users land
    /// on the right host. Defaults to the local daemon, which is rarely
    /// what an alert recipient or remote SDK needs.
    #[arg(
        long,
        env = "ERREXD_PUBLIC_URL",
        default_value = "http://localhost:9090"
    )]
    pub public_url: String,

    /// Bearer token guarding `/api/admin/*` endpoints. Unset by default →
    /// admin endpoints respond 503. Operators set this to enable the SPA's
    /// project-management UI; empty string is treated as unset.
    #[arg(long, env = "ERREXD_ADMIN_TOKEN", default_value = "")]
    pub admin_token: Option<String>,
}

impl Config {
    pub fn http_addr(&self) -> SocketAddr {
        format!("{}:{}", self.http_host, self.http_port)
            .parse()
            .expect("valid http bind addr")
    }

    pub fn mcp_addr(&self) -> SocketAddr {
        format!("{}:{}", self.mcp_host, self.mcp_port)
            .parse()
            .expect("valid mcp bind addr")
    }
}
