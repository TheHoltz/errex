//! `errexd` — error-tracking daemon.
//!
//! Wiring only lives here. Each subsystem is a module; main composes them
//! and supervises the run loop.

use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};
use tokio::signal;
use tokio::sync::{broadcast, mpsc};

mod auth;
mod config;
mod crypto;
mod digest;
mod error;
mod fingerprint;
mod ingest;
mod lockout;
mod mcp;
mod rate_limit;
mod retention;
mod spa;
mod store;
mod triage;
mod webhook;
mod ws;

use config::Config;

#[derive(Debug, Parser)]
#[command(name = "errexd", version, about = "errex error-tracking daemon")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    config: Config,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Probe the daemon's /health endpoint and exit 0 on success. Used as the
    /// container HEALTHCHECK so we don't need to bake a curl/wget into the
    /// runtime image.
    Healthcheck {
        #[arg(long, default_value = "http://127.0.0.1:9090/health")]
        url: String,
    },

    /// Manage projects + DSN ingest tokens. Operates directly on the
    /// SQLite file so it works without the daemon running.
    Project {
        #[command(subcommand)]
        action: ProjectCmd,
    },
}

#[derive(Debug, Subcommand)]
enum ProjectCmd {
    /// Create a new project and emit its DSN. Fails if the name is taken.
    Add {
        name: String,
        /// Public URL the SDK should target. Defaults to the local daemon.
        #[arg(long, default_value = "http://localhost:9090")]
        public_url: String,
    },
    /// List projects with their tokens. For self-host pequeno only — never
    /// expose this output beyond trusted operators.
    List,
    /// Replace a project's token, invalidating the previous DSN.
    Rotate { name: String },
    /// Set the webhook URL for a project. Compatible with Slack, Discord
    /// (`/slack` suffix), and Teams "Incoming Webhook" endpoints.
    SetWebhook { name: String, url: String },
    /// Clear the webhook URL for a project.
    UnsetWebhook { name: String },
}

/// Channel depth for events flowing from HTTP handlers into the digest task.
/// Backpressure: HTTP requests block briefly when this fills, which is the
/// desired signal under sustained overload. Sized for self-host: ~256 events
/// in flight × ~2 KB JSON each ≈ 512 KB worst-case buffering.
const INGEST_CHANNEL_CAPACITY: usize = 256;

/// Broadcast capacity for fan-out to SPA clients. Lagging subscribers drop
/// messages silently — the next snapshot they pull will catch them up.
const FANOUT_CHANNEL_CAPACITY: usize = 64;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Healthcheck { url }) => return run_healthcheck(&url).await,
        Some(Command::Project { action }) => return run_project_cmd(action, &cli.config).await,
        None => {}
    }

    let cfg = cli.config;
    init_tracing(&cfg.log_level);

    // Storage: the file's parent must exist before SQLite will create it.
    tokio::fs::create_dir_all(&cfg.data_dir)
        .await
        .with_context(|| format!("creating data dir {}", cfg.data_dir.display()))?;
    let db_path = cfg.data_dir.join("errex.db");
    let store = store::Store::open(&db_path)
        .await
        .with_context(|| format!("opening sqlite at {}", db_path.display()))?;
    store.migrate().await.context("running migrations")?;

    // Single-writer digest task owns the store; ingest hands events to it
    // through a channel, and it broadcasts updates to subscribers. There is
    // intentionally no in-memory issue cache — the WS server queries the
    // store directly when a client connects (see crate::ws). Self-host
    // pequeno: every byte of duplicated state is a cost we don't need.
    let (event_tx, event_rx) = mpsc::channel(INGEST_CHANNEL_CAPACITY);
    let (fanout_tx, _) = broadcast::channel(FANOUT_CHANNEL_CAPACITY);
    // Webhook channel is small: outbound notifications are infrequent (new
    // issues + regressions only). A backed-up channel only loses alerts,
    // which is preferable to backpressuring digest.
    let (webhook_tx, webhook_rx) = mpsc::channel(64);

    let digest_handle = {
        let store = store.clone();
        let fanout = fanout_tx.clone();
        let webhooks = webhook_tx.clone();
        tokio::spawn(async move { digest::run(store, event_rx, fanout, webhooks).await })
    };

    let webhook_handle = {
        let store = store.clone();
        let public_url = cfg.public_url.clone();
        tokio::spawn(async move { webhook::run(store, public_url, webhook_rx).await })
    };

    let rate_limiter = Arc::new(rate_limit::RateLimiter::new(
        cfg.rate_limit_per_min,
        cfg.rate_limit_burst,
    ));

    let http_state = Arc::new(ingest::AppState {
        events: event_tx.clone(),
        store: store.clone(),
        fanout: fanout_tx.clone(),
        require_auth: cfg.require_auth,
        rate_limiter: rate_limiter.clone(),
        setup_token: cfg.admin_token.clone().filter(|s| !s.is_empty()),
        public_url: cfg.public_url.clone(),
        dev_mode: cfg.dev_mode,
    });
    let http_handle = tokio::spawn(ingest::serve(cfg.http_addr(), http_state));
    let ws_handle = tokio::spawn(ws::serve(cfg.ws_addr(), store.clone(), fanout_tx.clone()));
    let mcp_handle = tokio::spawn(mcp::serve(cfg.mcp_addr()));
    let retention_handle = {
        let store = store.clone();
        let days = cfg.retention_days;
        tokio::spawn(async move { retention::run(store, days).await })
    };

    tracing::info!(
        "errexd listening on :{} (http), :{} (ws), :{} (mcp)",
        cfg.http_port,
        cfg.ws_port,
        cfg.mcp_port,
    );

    // Block on either Ctrl-C or any subsystem error.
    tokio::select! {
        res = http_handle => res.context("http task panicked")?.context("http server")?,
        res = ws_handle => res.context("ws task panicked")?.context("ws server")?,
        res = mcp_handle => res.context("mcp task panicked")?.context("mcp server")?,
        res = digest_handle => res.context("digest task panicked")?.context("digest")?,
        res = retention_handle => res.context("retention task panicked")?,
        res = webhook_handle => res.context("webhook task panicked")?,
        _ = shutdown_signal() => tracing::info!("shutdown signal received"),
    }

    Ok(())
}

fn init_tracing(level: &str) {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    fmt().with_env_filter(filter).with_target(false).init();
}

/// Tiny HTTP/1.1 GET implemented with bare TCP — avoids pulling in a client
/// crate just for the container HEALTHCHECK.
async fn run_healthcheck(url: &str) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    // Parse `http://host:port/path` by hand. We don't want hyper here.
    let stripped = url
        .strip_prefix("http://")
        .ok_or_else(|| anyhow::anyhow!("healthcheck url must start with http://"))?;
    let (authority, path) = stripped.split_once('/').unwrap_or((stripped, ""));
    let host_port = authority;
    let path = format!("/{path}");

    let mut stream = TcpStream::connect(host_port)
        .await
        .context("connect to daemon")?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await?;

    let mut buf = Vec::with_capacity(256);
    stream.read_to_end(&mut buf).await?;
    let head = std::str::from_utf8(&buf[..buf.len().min(64)]).unwrap_or("");
    if head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200") {
        Ok(())
    } else {
        anyhow::bail!("unhealthy: {}", head.lines().next().unwrap_or(""))
    }
}

/// CLI entry for `errexd project ...`. Opens the same SQLite file the
/// daemon uses and operates on it directly. WAL mode means the running
/// daemon doesn't need to be stopped — but a freshly-added project is only
/// visible to a *new* request after the daemon's next read of the table,
/// which is on every ingest call.
async fn run_project_cmd(cmd: ProjectCmd, cfg: &Config) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(&cfg.data_dir)
        .await
        .with_context(|| format!("creating data dir {}", cfg.data_dir.display()))?;
    let db_path = cfg.data_dir.join("errex.db");
    let store = store::Store::open(&db_path).await?;
    store.migrate().await?;

    match cmd {
        ProjectCmd::Add { name, public_url } => {
            let p = store.create_project(&name).await?;
            let dsn = format!(
                "{}/api/{}/envelope/?sentry_key={}",
                public_url.trim_end_matches('/'),
                p.name,
                p.token
            );
            println!("project: {}", p.name);
            println!("token:   {}", p.token);
            println!("dsn:     {}", dsn);
        }
        ProjectCmd::List => {
            let list = store.list_admin_projects().await?;
            if list.is_empty() {
                println!("(no projects)");
            }
            for p in list {
                let used = p
                    .last_used_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| "never".into());
                println!("{:24} token={}  last_used={}", p.name, p.token, used);
            }
        }
        ProjectCmd::Rotate { name } => {
            let p = store.rotate_token(&name).await?;
            println!("rotated: {} → token={}", p.name, p.token);
        }
        ProjectCmd::SetWebhook { name, url } => {
            store.set_project_webhook(&name, Some(&url)).await?;
            println!("webhook set for {name}: {url}");
        }
        ProjectCmd::UnsetWebhook { name } => {
            store.set_project_webhook(&name, None).await?;
            println!("webhook cleared for {name}");
        }
    }
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
