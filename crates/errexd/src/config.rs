use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;

/// Daemon configuration. Every flag also reads from an `ERREX_*` env var so
/// `docker compose` can configure without overriding the entrypoint.
#[derive(Debug, Clone, Parser)]
#[command(name = "errexd", version, about = "errex error-tracking daemon")]
pub struct Config {
    /// Directory holding the SQLite database (and future data files).
    #[arg(long, env = "ERREX_DATA_DIR", default_value = "./data")]
    pub data_dir: PathBuf,

    /// Bind address for the HTTP ingest server. Loopback by default so a
    /// fresh `errexd` invocation is not reachable off-host until the
    /// operator opts in to a public bind. Docker port mapping requires the
    /// container's process to listen on `0.0.0.0`, so the supplied
    /// docker-compose.yml sets `ERREX_HOST=0.0.0.0` explicitly — the change
    /// is local-developer-friendly and prevents accidental public exposure
    /// on bare metal.
    #[arg(long, env = "ERREX_HOST", default_value = "127.0.0.1")]
    pub http_host: String,

    /// HTTP port. Reads from `ERREX_PORT` first, else falls back to
    /// `PORT` (the convention Railway / Fly / Heroku / Render all set on
    /// their side), else defaults to 9090. The fallback chain is what
    /// makes a one-click deploy "just work" without operator
    /// boilerplate. Resolved by [`Config::resolved_http_port`].
    #[arg(long, env = "ERREX_PORT")]
    pub http_port: Option<u16>,

    /// Bind address for the MCP server (AI agents). Loopback by default —
    /// the MCP listener is currently a stub that drops every connection,
    /// and a public bind here is gratuitous attack surface. Docker compose
    /// sets `ERREX_MCP_HOST=0.0.0.0` only when the operator explicitly
    /// wants the port exposed.
    #[arg(long, env = "ERREX_MCP_HOST", default_value = "127.0.0.1")]
    pub mcp_host: String,

    #[arg(long, env = "ERREX_MCP_PORT", default_value_t = 9092)]
    pub mcp_port: u16,

    /// Logging level when RUST_LOG is unset.
    #[arg(long, env = "ERREX_LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    /// When true, the HTTP server permits CORS requests from the Vite dev
    /// server (http://localhost:5173). Off in production so the embedded SPA
    /// is the only browser surface.
    #[arg(long, env = "ERREX_DEV_MODE", default_value_t = false)]
    pub dev_mode: bool,

    /// When true, the ingest endpoint requires a `sentry_key` matching the
    /// configured project's token (see `errexd project add`). Defaults to
    /// `true` so a fresh deploy is fail-closed — an operator who knows the
    /// daemon sits behind a private network can opt out with
    /// `ERREX_REQUIRE_AUTH=false`. Sentry SDKs always send `sentry_key`, so
    /// this is the safe default for any internet-reachable bind.
    #[arg(long, env = "ERREX_REQUIRE_AUTH", default_value_t = true)]
    pub require_auth: bool,

    /// Days of event payload retention. The daemon purges older events
    /// hourly. Issue rows (counts, first/last seen) are kept regardless.
    /// `0` disables retention — events live forever until the disk fills.
    #[arg(long, env = "ERREX_RETENTION_DAYS", default_value_t = 30)]
    pub retention_days: u32,

    /// Per-project ingest rate cap, events per minute. `0` = unlimited.
    /// The bucket also has burst capacity (`rate_limit_burst`) so short
    /// spikes don't get truncated; only sustained over-rate is rejected.
    ///
    /// Default 6000/min (≈100 events/sec per project): high enough that a
    /// healthy app's spike is allowed through, low enough that one
    /// misbehaving SDK can't consume the daemon's whole digest budget on
    /// a small VM. Set to `0` explicitly to disable.
    #[arg(long, env = "ERREX_RATE_LIMIT_PER_MIN", default_value_t = 6000)]
    pub rate_limit_per_min: u32,

    /// Burst capacity for the per-project rate limiter. Ignored when
    /// `rate_limit_per_min == 0`.
    #[arg(long, env = "ERREX_RATE_LIMIT_BURST", default_value_t = 200)]
    pub rate_limit_burst: u32,

    /// Externally-reachable base URL of this daemon. Embedded in webhook
    /// payloads + DSNs returned to the SPA so SDKs configured by users land
    /// on the right host. Defaults to the local daemon, which is rarely
    /// what an alert recipient or remote SDK needs.
    #[arg(
        long,
        env = "ERREX_PUBLIC_URL",
        default_value = "http://localhost:9090"
    )]
    pub public_url: String,

    /// Bearer token guarding `/api/admin/*` endpoints. Unset by default →
    /// admin endpoints respond 503. Operators set this to enable the SPA's
    /// project-management UI; empty string is treated as unset.
    #[arg(long, env = "ERREX_ADMIN_TOKEN", default_value = "")]
    pub admin_token: Option<String>,

    /// When true, the daemon trusts the first hop in the `X-Forwarded-For`
    /// header as the client IP for lockout / audit purposes. Off by default
    /// because the daemon's stock deploy is reachable directly — an attacker
    /// could otherwise rotate this header per request to bypass the per-IP
    /// lockout policy. Operators who front the daemon with a reverse proxy
    /// that strips and re-emits a single XFF should opt in via
    /// `ERREX_TRUST_PROXY_HEADERS=true`.
    #[arg(long, env = "ERREX_TRUST_PROXY_HEADERS", default_value_t = false)]
    pub trust_proxy_headers: bool,
}

impl Config {
    /// `ERREX_PORT` > `PORT` (PaaS convention) > 9090 default.
    pub fn resolved_http_port(&self) -> u16 {
        if let Some(p) = self.http_port {
            return p;
        }
        if let Ok(s) = std::env::var("PORT") {
            if let Ok(p) = s.parse::<u16>() {
                return p;
            }
        }
        9090
    }

    pub fn http_addr(&self) -> SocketAddr {
        format!("{}:{}", self.http_host, self.resolved_http_port())
            .parse()
            .expect("valid http bind addr")
    }

    pub fn mcp_addr(&self) -> SocketAddr {
        format!("{}:{}", self.mcp_host, self.mcp_port)
            .parse()
            .expect("valid mcp bind addr")
    }

    /// Default `public_url` is the local-loopback fallback. Detect it so
    /// `main` can log a warning when the daemon is bound to a public
    /// interface but the operator forgot to set `ERREX_PUBLIC_URL` to
    /// the real hostname — DSNs and webhook links would otherwise point
    /// at `localhost:9090` which is useless to remote SDKs.
    pub fn public_url_is_default(&self) -> bool {
        self.public_url == "http://localhost:9090"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// `rate_limit_per_min` defaults to a non-zero value so a self-host
    /// daemon ships with backpressure on by default. The exact number
    /// can change but `0` (unlimited) is wrong as a default.
    #[test]
    fn rate_limit_default_is_nonzero() {
        let cfg = Config::parse_from(["errexd"]);
        assert!(
            cfg.rate_limit_per_min > 0,
            "rate_limit_per_min default must be > 0, got {}",
            cfg.rate_limit_per_min
        );
    }

    /// Explicit `--http-port` always wins over the PORT env fallback.
    /// We don't mutate env vars in tests (cargo runs tests in
    /// parallel; PORT manipulation would race with other tests reading
    /// env), so this asserts only the deterministic path: explicit
    /// value present → use it.
    #[test]
    fn resolved_http_port_explicit_wins() {
        let cfg = Config::parse_from(["errexd", "--http-port", "12345"]);
        assert_eq!(cfg.resolved_http_port(), 12345);
    }

    #[test]
    fn public_url_default_detection() {
        let cfg = Config::parse_from(["errexd"]);
        assert!(cfg.public_url_is_default());
        let cfg2 = Config::parse_from(["errexd", "--public-url", "https://errex.example.com"]);
        assert!(!cfg2.public_url_is_default());
    }

    /// Fail-closed default: a fresh daemon without explicit configuration
    /// must not be reachable off-host. Operators in docker / on remote
    /// hosts opt in to `0.0.0.0` deliberately — a public bind is never the
    /// implicit behaviour.
    #[test]
    fn http_host_default_is_loopback() {
        let cfg = Config::parse_from(["errexd"]);
        assert_eq!(cfg.http_host, "127.0.0.1");
    }

    #[test]
    fn mcp_host_default_is_loopback() {
        let cfg = Config::parse_from(["errexd"]);
        assert_eq!(cfg.mcp_host, "127.0.0.1");
    }

    /// Fail-closed default: ingest is auth-gated unless the operator
    /// explicitly opts out. Without this, a public daemon would happily
    /// accept anonymous events from anyone on the network and let an
    /// attacker exhaust the digest budget.
    #[test]
    fn require_auth_default_is_true() {
        let cfg = Config::parse_from(["errexd"]);
        assert!(cfg.require_auth, "ingest must be auth-gated by default");
    }
}
