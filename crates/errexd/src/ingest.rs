//! HTTP ingest + browser-facing API server.
//!
//! Serves four categories of routes off port 9090:
//!   - `/health`, `/api/:project/envelope/`     — operations + Sentry SDK ingest
//!   - `/api/projects`, `/api/issues`           — JSON for the SPA
//!   - `/ws/:project`                           — fan-out WebSocket (axum upgrade)
//!   - everything else                          — embedded SvelteKit SPA
//!
//! Routing is intentionally flat so it's easy to add `/api/<project>/store/`
//! and similar Sentry endpoints later.

use std::io::Read;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use errex_proto::{Event, ProtoError};
use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::digest::IngestEvent;
use crate::error::DaemonError;
use crate::metrics::Metrics;
use crate::rate_limit::RateLimiter;
use crate::spa;
use crate::store::Store;

#[derive(Debug)]
pub struct AppState {
    pub events: mpsc::Sender<IngestEvent>,
    pub store: Store,
    /// Fanout sender shared with the WS server. Mutating endpoints
    /// (`PUT /status`) push `IssueUpdated` here so every connected client
    /// converges to the new state without polling.
    pub fanout: tokio::sync::broadcast::Sender<errex_proto::ServerMessage>,
    /// When true, ingest validates a `sentry_key` against the `projects`
    /// table. Off by default for self-host pequeno behind a private net.
    pub require_auth: bool,
    /// Per-project token bucket. A `RateLimiter` constructed with
    /// `per_min == 0` is a no-op, so callers don't branch on enabled.
    pub rate_limiter: Arc<RateLimiter>,
    /// One-shot setup secret consumed by `/api/auth/setup` while the
    /// `users` table is empty. After the first user exists, this value is
    /// effectively dead — the setup endpoint refuses to fire again. Wired
    /// through `ERREX_ADMIN_TOKEN` for upgrade compatibility with the
    /// previous bearer-auth deployment.
    pub setup_token: Option<String>,
    /// Public-facing URL for this daemon. Embedded in DSNs returned to the
    /// SPA so SDKs configured by users land on the right host.
    pub public_url: String,
    /// True when the operator started the daemon with `ERREX_DEV_MODE`.
    /// Relaxes cookie security flags so cookies issued over `http://`
    /// localhost are accepted by browsers (which refuse `Secure` over
    /// non-https).
    pub dev_mode: bool,
    /// When true, the per-IP lockout bucket reads `X-Forwarded-For`
    /// instead of the direct peer socket. Off by default — see
    /// [`crate::auth::extract_client_ip`] for the rationale.
    pub trust_proxy_headers: bool,
    /// Process-lifetime counters surfaced via `/metrics`.
    pub metrics: Arc<Metrics>,
    /// Capacity of the webhook channel, surfaced through `/metrics` so
    /// operators can see when alert delivery is being dropped on a hot
    /// loop (`try_send` returns Full).
    pub webhook_capacity: usize,
    /// Live handle to the webhook sender so `/metrics` can report depth
    /// without an extra plumbed counter. Not used to send.
    pub webhook_sender: tokio::sync::mpsc::Sender<crate::webhook::Trigger>,
}

/// Build the API router without binding a listener. Extracted so tests can
/// drive it via `tower::ServiceExt::oneshot` without spinning up a port.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        // Cap raw envelope body at 1 MiB. Sentry events are typically
        // <100 KiB; the legitimate p99 lives well under this. The
        // dedicated cap protects the JSON parser and the gzip
        // decompressor (the latter has its own 8 MiB cap on the
        // *expanded* output — this layer guards the *compressed*
        // input). Combined, an adversary cannot exhaust memory by
        // pumping a single oversized request.
        .route(
            "/api/:project/envelope/",
            post(ingest_envelope).layer(axum::extract::DefaultBodyLimit::max(1024 * 1024)),
        )
        .route("/api/projects", get(list_projects))
        .route("/api/issues", get(list_issues))
        .route("/api/issues/:id/event", get(latest_event))
        .route("/api/issues/:id/status", axum::routing::put(put_status))
        // ----- auth -----
        .route("/api/auth/setup", post(crate::auth::handle_setup))
        .route(
            "/api/auth/setup-status",
            get(crate::auth::handle_setup_status),
        )
        .route("/api/auth/login", post(crate::auth::handle_login))
        .route("/api/auth/logout", post(crate::auth::handle_logout))
        .route("/api/auth/me", get(crate::auth::handle_me))
        // ----- admin -----
        .route(
            "/api/admin/retention",
            get(admin_get_retention).put(admin_put_retention),
        )
        .route(
            "/api/admin/projects",
            get(admin_list_projects).post(admin_create_project),
        )
        .route(
            "/api/admin/projects/:name",
            axum::routing::patch(admin_rename_project).delete(admin_delete_project),
        )
        .route(
            "/api/admin/projects/:name/webhook",
            axum::routing::put(admin_set_webhook),
        )
        .route("/api/admin/projects/:name/rotate", post(admin_rotate_token))
        .route(
            "/api/admin/projects/:name/activity",
            get(admin_project_activity),
        )
        .route(
            "/api/admin/projects/:name/destroy-preview",
            get(admin_destroy_preview),
        )
        // ----- admin: users -----
        .route(
            "/api/admin/users",
            get(admin_list_users).post(admin_create_user),
        )
        .route(
            "/api/admin/users/:u",
            get(admin_get_user)
                .patch(admin_patch_user)
                .delete(admin_delete_user),
        )
        .route(
            "/api/admin/users/:u/sessions",
            get(admin_list_user_sessions),
        )
        .route(
            "/api/admin/users/:u/sessions/revoke-all",
            post(admin_revoke_user_sessions),
        )
        // ----- websocket fan-out -----
        // Must be a real route (not the SPA fallback) so the upgrade
        // handshake gets `101 Switching Protocols` instead of `200 +
        // index.html`. See crate::ws for the full background.
        .route("/ws/:project", get(crate::ws::handle))
        .with_state(state)
        .fallback(spa::handler)
        // Defense-in-depth response headers, applied to every route — SPA
        // assets and API JSON alike.
        //
        //   * `X-Content-Type-Options: nosniff` refuses MIME sniffing so a
        //     future static-asset content-type mismatch can't be sniffed
        //     into HTML and rendered as script.
        //   * `X-Frame-Options: DENY` forbids framing entirely. Redundant
        //     with CSP's `frame-ancestors` but covers older browsers.
        //   * `Referrer-Policy: same-origin` keeps deep-link issue URLs
        //     out of off-origin Referer logs.
        //   * `Strict-Transport-Security` pins HTTPS for a year. Browsers
        //     ignore it over plain HTTP, so unconditional emission is
        //     safe and helpful behind a TLS-terminating proxy.
        //   * `Content-Security-Policy` is opinionated for the bundled
        //     SPA: scripts are `'self'` only; inline styles are permitted
        //     because Tailwind / SvelteKit emit a small amount during
        //     hydration; the WS fan-out lands on `connect-src 'self'`.
        //
        // `if_not_present` semantics let a future handler override (e.g.
        // a download endpoint that wants `X-Frame-Options: SAMEORIGIN`)
        // without rewriting the layer chain.
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(SECURITY_CSP),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::REFERRER_POLICY,
            HeaderValue::from_static("same-origin"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        // `X-Robots-Tag` keeps every response — JSON, hashed assets,
        // arbitrary deep-link paths — out of search-engine indexes even
        // when a crawler ignores `/robots.txt` or arrives via an inbound
        // link. errex is operator-facing telemetry; nothing here belongs
        // in public results.
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex, nofollow, noarchive, nosnippet"),
        ))
}

// `script-src` permits `'unsafe-inline'` because SvelteKit's static
// adapter emits an inline hydration <script> per page. Per-build hashing
// would be the strictly-better mitigation but requires templating the
// SPA HTML (the rust-embed-bundled file is static), so the practical
// equivalent here is to lean on Svelte's auto-escaping for XSS defense
// and treat CSP as defense-in-depth on the *other* directives:
//   * `frame-ancestors 'none'` blocks clickjacking unconditionally.
//   * `connect-src 'self' ws: wss:` pins exfiltration paths.
//   * `object-src 'none'` / `base-uri 'none'` cut classic plugin /
//     base-tag injection vectors.
//   * `img-src` / `font-src` allow `data:` for the bundled fonts and
//     the SPA's data-URI icons.
//
// The SPA source has no `{@html}`, `innerHTML`, `document.write`, or
// `eval()` (verified via grep) — Svelte 5 runes auto-escape every
// rendered value, so the practical XSS surface is small.
const SECURITY_CSP: &str = "default-src 'self'; \
script-src 'self' 'unsafe-inline'; \
style-src 'self' 'unsafe-inline'; \
connect-src 'self' ws: wss:; \
img-src 'self' data:; \
font-src 'self' data:; \
frame-ancestors 'none'; \
base-uri 'none'; \
form-action 'self'; \
object-src 'none'";

pub async fn serve(addr: SocketAddr, state: Arc<AppState>) -> Result<(), DaemonError> {
    let dev_mode = state.dev_mode;
    let mut app = build_router(state);

    if dev_mode {
        // Permit the Vite dev server origin so `bun run dev` on :5173 can
        // call the daemon directly without proxying. In production the SPA
        // is served from this same origin and CORS is irrelevant.
        let cors = CorsLayer::new()
            .allow_origin(
                "http://localhost:5173"
                    .parse::<HeaderValue>()
                    .expect("static origin"),
            )
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
            .allow_headers(tower_http::cors::Any);
        app = app.layer(cors);
        tracing::info!("dev mode: CORS enabled for http://localhost:5173");
    }

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("http server bound to {addr}");
    // `into_make_service_with_connect_info` populates the per-request
    // `ConnectInfo<SocketAddr>` extension that the auth handlers use to
    // bucket lockout state by client IP.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(DaemonError::Io)?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

/// Operator-facing metrics. Static fields (channel capacities) are read
/// once; dynamic ones (queue depth, subscriber count, RSS) are sampled
/// on each scrape. Cheap enough to allow Prometheus-style scraping every
/// few seconds without measurable overhead — the I/O cost is one
/// `/proc/self/status` read.
async fn metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let snap = state.metrics.snapshot();
    let body = json!({
        "events_accepted": snap.events_accepted,
        "events_rejected_rate_limit": snap.events_rejected_rate_limit,
        "ws_lagged_total": snap.ws_lagged_total,
        "ingest_channel": {
            "capacity": state.events.max_capacity(),
            "depth": state.events.max_capacity().saturating_sub(state.events.capacity()),
        },
        "webhook_channel": {
            "capacity": state.webhook_capacity,
            "depth": state.webhook_capacity.saturating_sub(state.webhook_sender.capacity()),
        },
        "fanout": {
            "subscribers": state.fanout.receiver_count(),
        },
        "rss_kb": crate::metrics::read_rss_kb(),
    });
    Json(body)
}

async fn list_projects(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    // Read endpoints leak production stack traces (which routinely contain
    // request bodies, secrets, env values), so they are auth-gated. Any
    // signed-in user — including viewers — may read.
    if let Err(resp) = crate::auth::require_auth(&state, &headers).await {
        return Ok(resp);
    }
    // One round-trip with GROUP BY beats loading every issue and counting
    // in the handler. Self-host pequeno may have thousands of issues; we
    // don't want to materialize them all just to return four numbers.
    let projects = state.store.project_summaries().await?;
    Ok(Json(projects).into_response())
}

#[derive(Debug, Deserialize)]
struct IssueQuery {
    project: Option<String>,
}

async fn list_issues(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<IssueQuery>,
) -> Result<Response, ApiError> {
    if let Err(resp) = crate::auth::require_auth(&state, &headers).await {
        return Ok(resp);
    }
    let issues = match q.project {
        Some(project) => state.store.list_issues_by_project(&project).await?,
        None => state.store.load_issues().await?,
    };
    Ok(Json(issues).into_response())
}

/// PUT /api/issues/:id/status — sets triage status. The body is
/// `{"status": "unresolved" | "resolved" | "muted" | "ignored"}`. Unknown
/// statuses return 400 (caught by the strict serde enum); unknown ids
/// return 404 via the `NotFound` arm of `DaemonError`.
#[derive(Debug, serde::Deserialize)]
struct StatusBody {
    status: errex_proto::IssueStatus,
}

async fn put_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: axum::http::HeaderMap,
    body: Option<Json<StatusBody>>,
) -> Result<Response, ApiError> {
    // Triage mutation is admin-only — a read-only viewer cannot silence
    // alerts on a compromised account.
    if let Err(resp) = crate::auth::require_admin(&state, &headers).await {
        return Ok(resp);
    }
    let Some(Json(body)) = body else {
        return Ok((StatusCode::BAD_REQUEST, "invalid status body").into_response());
    };
    match state.store.set_status(id, body.status).await {
        Ok(issue) => {
            // Best-effort broadcast: a missing receiver only means no client
            // is subscribed, which is harmless — they'll catch up via the
            // next Snapshot. We deliberately don't fail the request on a
            // dead channel.
            let _ = state.fanout.send(errex_proto::ServerMessage::IssueUpdated {
                issue: issue.clone(),
            });
            Ok(Json(issue).into_response())
        }
        Err(DaemonError::NotFound(_)) => {
            Ok((StatusCode::NOT_FOUND, "issue not found").into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// Latest event payload for an issue, returned as the verbatim Event JSON.
/// Used by the SPA to populate StackTrace / Breadcrumbs / Tags. Returns 404
/// when the issue exists but has no events yet (possible only during the
/// brief window after upsert before insert_event lands).
async fn latest_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    if let Err(resp) = crate::auth::require_auth(&state, &headers).await {
        return Ok(resp);
    }
    match state.store.latest_event(id).await? {
        Some(stored) => Ok(Json(stored).into_response()),
        None => Ok((StatusCode::NOT_FOUND, "no event for issue").into_response()),
    }
}

// ----- admin endpoints -----

/// JSON shape returned to the SPA's project-settings UI. Both `dsn`
/// (Sentry-standard, for SDK consumers) and `ingest_url` (raw POST URL,
/// for curl) are server-computed from `public_url`.
#[derive(serde::Serialize)]
struct AdminProjectView {
    name: String,
    token: String,
    webhook_url: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Sentry-standard DSN: `<scheme>://<token>@<host>/<project>`.
    /// Drop into `Sentry.init({ dsn })` and the SDK does the rest.
    dsn: String,
    /// Plain HTTP URL for curl-based testing without an SDK.
    /// Includes the auth token as `?sentry_key=...`.
    ingest_url: String,
    /// Most recent webhook delivery health. `None` until the webhook task
    /// fires for the first time. See `record_webhook_attempt` for the 0
    /// sentinel meaning.
    last_webhook_status: Option<i16>,
    last_webhook_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Build the Sentry-standard DSN for a project: `<scheme>://<token>@<host>/<project>`.
///
/// Sentry SDKs construct the ingest URL themselves — given this DSN they
/// POST to `<scheme>://<host>/api/<project>/envelope/` with an
/// `X-Sentry-Auth` header carrying the token. This format is what every
/// SDK (browser, Node, Python, Ruby, etc.) expects; `init({ dsn })` will
/// fail to parse anything else.
fn dsn_for(public_url: &str, project: &str, token: &str) -> String {
    let trimmed = public_url.trim_end_matches('/');
    let (scheme, authority) = match trimmed.split_once("://") {
        Some(parts) => parts,
        None => ("http", trimmed),
    };
    format!("{scheme}://{token}@{authority}/{project}")
}

/// Build the raw POST URL an operator can use with `curl` to hand-test
/// ingest without going through an SDK. Matches what the SDK would
/// construct internally from the DSN, plus `?sentry_key=` for header-less
/// auth (we accept the query-param form too).
fn ingest_url_for(public_url: &str, project: &str, token: &str) -> String {
    format!(
        "{}/api/{}/envelope/?sentry_key={}",
        public_url.trim_end_matches('/'),
        project,
        token,
    )
}

impl AdminProjectView {
    fn from(p: crate::store::Project, public_url: &str) -> Self {
        let dsn = dsn_for(public_url, &p.name, &p.token);
        let ingest_url = ingest_url_for(public_url, &p.name, &p.token);
        AdminProjectView {
            name: p.name,
            token: p.token,
            webhook_url: p.webhook_url,
            created_at: p.created_at,
            last_used_at: p.last_used_at,
            dsn,
            ingest_url,
            last_webhook_status: p.last_webhook_status,
            last_webhook_at: p.last_webhook_at,
        }
    }
}

/// Bridges `/api/admin/*` handlers to the cookie-session model. Earlier
/// versions accepted a shared `Authorization: Bearer ERREX_ADMIN_TOKEN`;
/// after the multi-user migration that token is consumed by the setup
/// wizard once and then forever ignored. Admin endpoints now require:
///
///   1. A valid `errex_session` cookie (issued by `/api/auth/login`).
///   2. The session's user has the `admin` role.
///
/// The clippy `result_large_err` lint fires because `Response` is ~128 B;
/// boxing would obscure the simple flow.
#[allow(clippy::result_large_err)]
async fn check_admin(state: &AppState, headers: &axum::http::HeaderMap) -> Result<(), Response> {
    crate::auth::require_admin(state, headers).await.map(|_| ())
}

async fn admin_get_retention(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    let s = state.store.get_retention_settings().await?;
    Ok(Json(s).into_response())
}

#[derive(Debug, Deserialize)]
struct RetentionBody {
    events_per_issue_max: i64,
    issues_per_project_max: i64,
    event_retention_days: i64,
}

async fn admin_put_retention(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: Option<Json<RetentionBody>>,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    let Some(Json(body)) = body else {
        return Ok((StatusCode::BAD_REQUEST, "missing body").into_response());
    };
    // Reject negatives so a UI bug can't accidentally send -1 and have
    // the row store an unrepresentable cap. 0 (= unlimited) is the only
    // permitted off switch.
    if body.events_per_issue_max < 0
        || body.issues_per_project_max < 0
        || body.event_retention_days < 0
    {
        return Ok((StatusCode::BAD_REQUEST, "values must be >= 0").into_response());
    }
    let s = crate::store::RetentionSettings {
        events_per_issue_max: body.events_per_issue_max,
        issues_per_project_max: body.issues_per_project_max,
        event_retention_days: body.event_retention_days,
    };
    state.store.set_retention_settings(s).await?;
    Ok(Json(s).into_response())
}

async fn admin_list_projects(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    let list = state.store.list_admin_projects().await?;
    let view: Vec<AdminProjectView> = list
        .into_iter()
        .map(|p| AdminProjectView::from(p, &state.public_url))
        .collect();
    Ok(Json(view).into_response())
}

#[derive(Debug, Deserialize)]
struct CreateProjectBody {
    name: String,
}

async fn admin_create_project(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: Option<Json<CreateProjectBody>>,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    let Some(Json(body)) = body else {
        return Ok((StatusCode::BAD_REQUEST, "missing body").into_response());
    };
    let name = body.name.trim();
    if name.is_empty() {
        return Ok((StatusCode::BAD_REQUEST, "name must not be empty").into_response());
    }
    match state.store.create_project(name).await {
        Ok(p) => {
            let view = AdminProjectView::from(p, &state.public_url);
            Ok((StatusCode::CREATED, Json(view)).into_response())
        }
        Err(DaemonError::Sqlx(sqlx::Error::Database(db_err))) if db_err.is_unique_violation() => {
            Ok((StatusCode::CONFLICT, "project name already exists").into_response())
        }
        Err(other) => Err(other.into()),
    }
}

#[derive(Debug, Deserialize)]
struct WebhookBody {
    /// Null clears the webhook; a string sets it.
    url: Option<String>,
}

async fn admin_set_webhook(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
    body: Option<Json<WebhookBody>>,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    let Some(Json(body)) = body else {
        return Ok((StatusCode::BAD_REQUEST, "missing body").into_response());
    };
    // SSRF gate: reject loopback / private / IMDS / internal-zone targets
    // before they make it to the store. Null clears the webhook and
    // bypasses validation.
    if let Some(url) = body.url.as_deref() {
        if let Err(why) = crate::webhook::validate_url(url) {
            return Ok((StatusCode::BAD_REQUEST, why).into_response());
        }
    }
    match state
        .store
        .set_project_webhook(&name, body.url.as_deref())
        .await
    {
        Ok(()) => {
            let p = state
                .store
                .project_by_name(&name)
                .await?
                .expect("just-updated project must exist");
            Ok(Json(AdminProjectView::from(p, &state.public_url)).into_response())
        }
        Err(DaemonError::NotFound(_)) => {
            Ok((StatusCode::NOT_FOUND, "project not found").into_response())
        }
        Err(other) => Err(other.into()),
    }
}

async fn admin_rotate_token(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    match state.store.rotate_token(&name).await {
        Ok(p) => Ok(Json(AdminProjectView::from(p, &state.public_url)).into_response()),
        Err(DaemonError::NotFound(_)) => {
            Ok((StatusCode::NOT_FOUND, "project not found").into_response())
        }
        Err(other) => Err(other.into()),
    }
}

#[derive(Debug, Deserialize)]
struct RenameBody {
    name: String,
}

async fn admin_rename_project(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
    body: Option<Json<RenameBody>>,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    let Some(Json(body)) = body else {
        return Ok((StatusCode::BAD_REQUEST, "missing body").into_response());
    };
    let new_name = body.name.trim();
    if new_name.is_empty() {
        return Ok((StatusCode::BAD_REQUEST, "name must not be empty").into_response());
    }
    match state.store.rename_project(&name, new_name).await {
        Ok(p) => Ok(Json(AdminProjectView::from(p, &state.public_url)).into_response()),
        Err(DaemonError::NotFound(_)) => {
            Ok((StatusCode::NOT_FOUND, "project not found").into_response())
        }
        Err(DaemonError::Sqlx(sqlx::Error::Database(db_err))) if db_err.is_unique_violation() => {
            Ok((StatusCode::CONFLICT, "project name already exists").into_response())
        }
        Err(other) => Err(other.into()),
    }
}

async fn admin_delete_project(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    match state.store.delete_project(&name).await {
        Ok(summary) => Ok(Json(summary).into_response()),
        Err(DaemonError::NotFound(_)) => {
            Ok((StatusCode::NOT_FOUND, "project not found").into_response())
        }
        Err(other) => Err(other.into()),
    }
}

async fn admin_destroy_preview(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    match state.store.delete_preview(&name).await {
        Ok(summary) => Ok(Json(summary).into_response()),
        Err(DaemonError::NotFound(_)) => {
            Ok((StatusCode::NOT_FOUND, "project not found").into_response())
        }
        Err(other) => Err(other.into()),
    }
}

async fn admin_project_activity(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    // Verify the project exists so unknown names get 404 instead of "all
    // zeros, looks legit." The activity query itself returns zeros for any
    // string and won't tell the operator their typo.
    if state.store.project_by_name(&name).await?.is_none() {
        return Ok((StatusCode::NOT_FOUND, "project not found").into_response());
    }
    let stats = state
        .store
        .activity_stats(&name, chrono::Utc::now())
        .await?;
    Ok(Json(stats).into_response())
}

// ----- admin: users -----

async fn admin_list_users(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    let users = state.store.list_users().await?;
    Ok(Json(users).into_response())
}

#[derive(Debug, Deserialize)]
struct CreateUserBody {
    username: String,
    password: String,
    role: crate::store::Role,
}

async fn admin_create_user(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: Option<Json<CreateUserBody>>,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    let Some(Json(body)) = body else {
        return Ok((StatusCode::BAD_REQUEST, "missing body").into_response());
    };
    let username = body.username.trim();
    if username.is_empty() || username.len() > 64 {
        return Ok((StatusCode::BAD_REQUEST, "username 1..=64 chars").into_response());
    }
    if let Err(why) = crate::crypto::validate_password_strength(&body.password) {
        return Ok((StatusCode::BAD_REQUEST, why).into_response());
    }
    let hash = crate::crypto::hash_password(&body.password)?;
    match state.store.create_user(username, &hash, body.role).await {
        Ok(u) => Ok((StatusCode::CREATED, Json(u)).into_response()),
        Err(DaemonError::Sqlx(sqlx::Error::Database(db_err))) if db_err.is_unique_violation() => {
            Ok((StatusCode::CONFLICT, "username already exists").into_response())
        }
        Err(other) => Err(other.into()),
    }
}

async fn admin_get_user(
    State(state): State<Arc<AppState>>,
    Path(u): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    match state.store.get_user(&u).await? {
        Some(user) => Ok(Json(user).into_response()),
        None => Ok((StatusCode::NOT_FOUND, "user not found").into_response()),
    }
}

#[derive(Debug, Deserialize)]
struct PatchUserBody {
    /// New password. Validated for strength when present. None = no change.
    password: Option<String>,
    role: Option<crate::store::Role>,
    /// `true` deactivates (revoking sessions), `false` reactivates.
    deactivated: Option<bool>,
}

async fn admin_patch_user(
    State(state): State<Arc<AppState>>,
    Path(u): Path<String>,
    headers: axum::http::HeaderMap,
    body: Option<Json<PatchUserBody>>,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    let Some(Json(body)) = body else {
        return Ok((StatusCode::BAD_REQUEST, "missing body").into_response());
    };
    if state.store.get_user(&u).await?.is_none() {
        return Ok((StatusCode::NOT_FOUND, "user not found").into_response());
    }

    // Validate the proposed change BEFORE applying anything: if any field
    // is rejected we want a clean 4xx, not a half-applied PATCH.
    if let Some(p) = &body.password {
        if let Err(why) = crate::crypto::validate_password_strength(p) {
            return Ok((StatusCode::BAD_REQUEST, why).into_response());
        }
    }

    // Last-active-admin guard. A demote, deactivate, or delete that would
    // leave zero active admins is refused — otherwise an errant click can
    // permanently lock everyone out of the daemon.
    let current = state
        .store
        .get_user(&u)
        .await?
        .expect("existence checked above");
    let demoting = body
        .role
        .map(|r| r != crate::store::Role::Admin && current.role == crate::store::Role::Admin)
        .unwrap_or(false);
    let deactivating = body.deactivated.unwrap_or(false)
        && current.role == crate::store::Role::Admin
        && current.deactivated_at.is_none();
    if (demoting || deactivating) && state.store.count_active_admins().await? <= 1 {
        return Ok((
            StatusCode::CONFLICT,
            "cannot demote or deactivate the last active admin",
        )
            .into_response());
    }

    if let Some(p) = &body.password {
        let hash = crate::crypto::hash_password(p)?;
        state.store.set_user_password(&u, &hash).await?;
    }
    if let Some(role) = body.role {
        state.store.set_user_role(&u, role).await?;
    }
    if let Some(deact) = body.deactivated {
        state.store.set_user_deactivated(&u, deact).await?;
    }

    let updated = state
        .store
        .get_user(&u)
        .await?
        .expect("existence checked above");
    Ok(Json(updated).into_response())
}

async fn admin_delete_user(
    State(state): State<Arc<AppState>>,
    Path(u): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    let target = match state.store.get_user(&u).await? {
        Some(t) => t,
        None => return Ok((StatusCode::NOT_FOUND, "user not found").into_response()),
    };
    if target.role == crate::store::Role::Admin
        && target.deactivated_at.is_none()
        && state.store.count_active_admins().await? <= 1
    {
        return Ok((StatusCode::CONFLICT, "cannot delete the last active admin").into_response());
    }
    state.store.delete_user(&u).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn admin_list_user_sessions(
    State(state): State<Arc<AppState>>,
    Path(u): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    if state.store.get_user(&u).await?.is_none() {
        return Ok((StatusCode::NOT_FOUND, "user not found").into_response());
    }
    let sessions = state.store.list_user_sessions(&u).await?;
    Ok(Json(sessions).into_response())
}

async fn admin_revoke_user_sessions(
    State(state): State<Arc<AppState>>,
    Path(u): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    if let Err(resp) = check_admin(&state, &headers).await {
        return Ok(resp);
    }
    if state.store.get_user(&u).await?.is_none() {
        return Ok((StatusCode::NOT_FOUND, "user not found").into_response());
    }
    let revoked = state.store.revoke_user_sessions(&u).await?;
    Ok(Json(serde_json::json!({"sessions_revoked": revoked})).into_response())
}

/// Surface store errors as a 500 with a tracing breadcrumb. We deliberately
/// don't leak the underlying message to the client.
pub(crate) struct ApiError(DaemonError);

impl From<DaemonError> for ApiError {
    fn from(e: DaemonError) -> Self {
        Self(e)
    }
}

// Allow `password_hash::Error` (argon2 init / verify) to bubble through
// `?` in auth handlers without wrapping it manually.
impl From<password_hash::Error> for ApiError {
    fn from(e: password_hash::Error) -> Self {
        Self(DaemonError::Crypto(format!("crypto: {e}")))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!(err = %self.0, "api error");
        (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
    }
}

/// Pull the SDK ingest token out of either the `X-Sentry-Auth` header
/// (Sentry SDK convention: `Sentry sentry_key=..., sentry_version=...`)
/// or a `?sentry_key=` query param (also Sentry-supported).
fn extract_sentry_key(req_headers: &axum::http::HeaderMap, query: &str) -> Option<String> {
    if let Some(auth) = req_headers
        .get("x-sentry-auth")
        .and_then(|v| v.to_str().ok())
    {
        for part in auth.split(',') {
            let part = part.trim();
            // The leading `Sentry ` realm is optional; many SDKs prefix it.
            for kv in part.split_whitespace() {
                if let Some(rest) = kv.strip_prefix("sentry_key=") {
                    return Some(rest.trim_matches('"').to_string());
                }
            }
        }
    }
    for pair in query.split('&') {
        if let Some(rest) = pair.strip_prefix("sentry_key=") {
            return Some(rest.to_string());
        }
    }
    None
}

async fn ingest_envelope(
    State(state): State<Arc<AppState>>,
    Path(project): Path<String>,
    headers: axum::http::HeaderMap,
    uri: axum::http::Uri,
    body: Bytes,
) -> impl IntoResponse {
    if state.require_auth {
        let key = extract_sentry_key(&headers, uri.query().unwrap_or(""));
        let Some(key) = key else {
            return (StatusCode::UNAUTHORIZED, "missing sentry_key").into_response();
        };
        match state.store.project_by_token(&key).await {
            Ok(Some(p)) if p.name == project => {}
            Ok(_) => {
                return (StatusCode::UNAUTHORIZED, "invalid sentry_key for project")
                    .into_response();
            }
            Err(err) => {
                tracing::error!(%err, "ingest auth lookup failed");
                return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
            }
        }
    }

    // Rate limit AFTER auth so unauthenticated traffic doesn't fill the
    // limiter map and DoS legitimate projects.
    if !state
        .rate_limiter
        .check(&project, std::time::Instant::now())
    {
        state.metrics.inc_rejected_rate_limit();
        return (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded").into_response();
    }
    let raw = match maybe_gunzip(&body) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(%err, "ingest: failed to gunzip envelope");
            return (StatusCode::BAD_REQUEST, "invalid gzip").into_response();
        }
    };

    let events = match parse_envelope(&raw) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(%err, "ingest: failed to parse envelope");
            return (StatusCode::BAD_REQUEST, "invalid envelope").into_response();
        }
    };

    if events.is_empty() {
        // Sentry SDKs send envelopes that contain only sessions or
        // transactions; that's not an error, just nothing for us yet.
        // Still bump telemetry — SDK is alive even if it sent no events.
        state.store.touch_project_used(&project).await;
        return StatusCode::OK.into_response();
    }

    for event in events {
        let rec = crate::digest::prepare(project.clone(), event);
        if state.events.send(rec).await.is_err() {
            // Digest receiver gone = shutting down. Don't bump telemetry
            // for a request we didn't actually accept.
            return (StatusCode::SERVICE_UNAVAILABLE, "shutting down").into_response();
        }
        state.metrics.inc_accepted();
    }

    // Telemetry: SPA's project header reads `last_used_at` for the "last
    // event" line, so it must reflect SDK liveness regardless of whether
    // ingest auth is enabled. Bump only after the envelope was accepted.
    state.store.touch_project_used(&project).await;
    StatusCode::OK.into_response()
}

/// Bound on the size of an ingested payload after gzip decompression.
/// Sentry events are typically <1 MiB raw; even attachments-rich events
/// rarely exceed a few hundred KiB. 8 MiB leaves headroom for unusual
/// outliers while keeping a gzip bomb from expanding a 2 MiB body into
/// gigabytes and OOM-ing the daemon on a 1 GB VM.
const MAX_DECOMPRESSED: usize = 8 * 1024 * 1024;

/// Detect gzip via magic bytes rather than trusting `Content-Encoding`. Sentry
/// SDKs are inconsistent about setting the header, and the cost of sniffing
/// two bytes is negligible.
///
/// The decompressor is wrapped in `Read::take(MAX_DECOMPRESSED + 1)` so a
/// well-compressed adversarial payload (e.g. a 2 MiB body of zeros that
/// expands ~1024:1) cannot exhaust memory.
fn maybe_gunzip(body: &[u8]) -> std::io::Result<Vec<u8>> {
    if body.len() >= 2 && body[0] == 0x1f && body[1] == 0x8b {
        let mut out = Vec::with_capacity(4096);
        let mut limited = GzDecoder::new(body).take((MAX_DECOMPRESSED as u64) + 1);
        limited.read_to_end(&mut out)?;
        if out.len() > MAX_DECOMPRESSED {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "decompressed payload exceeds limit",
            ));
        }
        Ok(out)
    } else {
        Ok(body.to_vec())
    }
}

/// Parse a Sentry envelope and return the event payloads it contains.
///
/// The envelope format:
/// - line 1: envelope header (JSON object)
/// - then pairs of `{item_header}\n{item_payload}\n`
///
/// Item headers may include a `length` field; if so, the payload is exactly
/// that many bytes. If absent, the payload runs until the next newline.
/// We only surface items of `type == "event"` — everything else (sessions,
/// transactions, attachments) is skipped silently.
fn parse_envelope(raw: &[u8]) -> Result<Vec<Event>, ProtoError> {
    let mut events = Vec::new();
    let mut cursor = 0usize;

    // Envelope header.
    let (_header, after_header) = read_line(raw, cursor)
        .ok_or_else(|| ProtoError::InvalidEnvelope("missing envelope header".into()))?;
    let _: Value = serde_json::from_slice(&raw[cursor..after_header.saturating_sub(1)])?;
    cursor = after_header;

    while cursor < raw.len() {
        let (item_header_end, next_cursor) = match read_line(raw, cursor) {
            Some(r) => r,
            None => break,
        };
        let header_bytes = &raw[cursor..item_header_end.saturating_sub(1)];
        if header_bytes.is_empty() {
            cursor = next_cursor;
            continue;
        }
        let header: Value = serde_json::from_slice(header_bytes)?;
        cursor = next_cursor;

        let item_type = header.get("type").and_then(|v| v.as_str()).unwrap_or("");

        let payload_end = if let Some(len) = header.get("length").and_then(|v| v.as_u64()) {
            let end = cursor + len as usize;
            if end > raw.len() {
                return Err(ProtoError::InvalidEnvelope(
                    "item length exceeds envelope size".into(),
                ));
            }
            end
        } else {
            // No length: payload runs to the next newline (or to EOF).
            raw[cursor..]
                .iter()
                .position(|&b| b == b'\n')
                .map(|p| cursor + p)
                .unwrap_or(raw.len())
        };

        let payload = &raw[cursor..payload_end];
        if item_type == "event" {
            let event: Event = serde_json::from_slice(payload)?;
            events.push(event);
        }

        // Advance past payload and its trailing newline (if any).
        cursor = payload_end;
        if cursor < raw.len() && raw[cursor] == b'\n' {
            cursor += 1;
        }
    }

    Ok(events)
}

/// Returns `(line_end_exclusive_of_newline_position, next_cursor)`. Both are
/// byte offsets into `buf`. None when no newline is found and `start` is at EOF.
fn read_line(buf: &[u8], start: usize) -> Option<(usize, usize)> {
    if start >= buf.len() {
        return None;
    }
    match buf[start..].iter().position(|&b| b == b'\n') {
        Some(rel) => Some((start + rel + 1, start + rel + 1)),
        None => Some((buf.len(), buf.len())),
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the Sentry envelope parser. These pin the parts of the
    //! format the daemon relies on: header line, length-prefixed items,
    //! length-less items terminated by newline or EOF, gzip detection, and
    //! the "ignore non-event items" contract.

    use super::*;
    use std::io::Write;

    fn header() -> &'static str {
        r#"{"event_id":"abc","sent_at":"2026-01-01T00:00:00Z"}"#
    }

    fn event_body(ty: &str) -> String {
        format!(
            r#"{{"timestamp":"2026-01-01T00:00:00Z","exception":{{"values":[{{"type":"{}","value":"v"}}]}}}}"#,
            ty
        )
    }

    #[test]
    fn parses_single_event_with_length_prefix() {
        let body = event_body("Boom");
        let item_header = format!(r#"{{"type":"event","length":{}}}"#, body.len());
        let envelope = format!("{}\n{}\n{}\n", header(), item_header, body);
        let events = parse_envelope(envelope.as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].primary_exception().and_then(|e| e.ty.as_deref()),
            Some("Boom")
        );
    }

    #[test]
    fn parses_event_without_length_field() {
        // Length-less items terminate at the next newline. Sentry SDKs in
        // older versions sent these.
        let body = event_body("NoLen");
        let envelope = format!("{}\n{}\n{}\n", header(), r#"{"type":"event"}"#, body);
        let events = parse_envelope(envelope.as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].primary_exception().and_then(|e| e.ty.as_deref()),
            Some("NoLen")
        );
    }

    #[test]
    fn parses_event_without_trailing_newline() {
        // Some clients omit the final newline. Last item must run to EOF.
        let body = event_body("Trail");
        let envelope = format!("{}\n{}\n{}", header(), r#"{"type":"event"}"#, body);
        let events = parse_envelope(envelope.as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn skips_non_event_items() {
        // sessions, transactions, attachments — all silently ignored.
        let session = r#"{"sid":"s1","status":"ok"}"#;
        let event_b = event_body("KeepMe");
        let session_header = format!(r#"{{"type":"session","length":{}}}"#, session.len());
        let envelope = format!(
            "{}\n{}\n{}\n{}\n{}\n",
            header(),
            session_header,
            session,
            r#"{"type":"event"}"#,
            event_b,
        );
        let events = parse_envelope(envelope.as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].primary_exception().and_then(|e| e.ty.as_deref()),
            Some("KeepMe")
        );
    }

    #[test]
    fn parses_two_events_in_one_envelope() {
        let b1 = event_body("First");
        let b2 = event_body("Second");
        let envelope = format!(
            "{}\n{}\n{}\n{}\n{}\n",
            header(),
            r#"{"type":"event"}"#,
            b1,
            r#"{"type":"event"}"#,
            b2,
        );
        let events = parse_envelope(envelope.as_bytes()).unwrap();
        assert_eq!(events.len(), 2);
        let types: Vec<_> = events
            .iter()
            .map(|e| e.primary_exception().and_then(|x| x.ty.clone()))
            .collect();
        assert_eq!(
            types,
            vec![Some("First".to_string()), Some("Second".to_string())]
        );
    }

    #[test]
    fn empty_envelope_with_only_header_yields_no_events() {
        let envelope = format!("{}\n", header());
        let events = parse_envelope(envelope.as_bytes()).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn rejects_envelope_with_no_header() {
        let err = parse_envelope(b"").unwrap_err();
        assert!(matches!(err, ProtoError::InvalidEnvelope(_)));
    }

    #[test]
    fn rejects_invalid_json_header() {
        let err = parse_envelope(b"not-json\n").unwrap_err();
        // Either Json error or InvalidEnvelope — both are valid 400s.
        assert!(matches!(
            err,
            ProtoError::Json(_) | ProtoError::InvalidEnvelope(_)
        ));
    }

    #[test]
    fn rejects_length_overflowing_envelope() {
        let envelope = format!(
            "{}\n{}\nshort",
            header(),
            r#"{"type":"event","length":9999}"#
        );
        let err = parse_envelope(envelope.as_bytes()).unwrap_err();
        assert!(matches!(err, ProtoError::InvalidEnvelope(_)));
    }

    // ----- gzip detection -----

    #[test]
    fn maybe_gunzip_passes_through_plain_bytes() {
        let plain = b"hello world";
        let out = maybe_gunzip(plain).unwrap();
        assert_eq!(out, plain);
    }

    #[test]
    fn maybe_gunzip_decompresses_gzip_magic() {
        let plain = b"the quick brown fox jumps over the lazy dog";
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(plain).unwrap();
        let gz = enc.finish().unwrap();
        assert_eq!(gz[0..2], [0x1f, 0x8b], "encoder must emit gzip magic");
        let out = maybe_gunzip(&gz).unwrap();
        assert_eq!(out, plain);
    }

    #[test]
    fn maybe_gunzip_handles_empty_input() {
        let out = maybe_gunzip(&[]).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn maybe_gunzip_rejects_oversized_decompression() {
        // Gzip bomb: 16 MiB of zeros compresses to ~16 KiB — well past
        // MAX_DECOMPRESSED (8 MiB). A naïve `read_to_end` would happily
        // allocate the full output and OOM a small VM.
        let payload = vec![0u8; (MAX_DECOMPRESSED + 1024) * 2];
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        enc.write_all(&payload).unwrap();
        let bomb = enc.finish().unwrap();
        // Sanity: the encoded bomb is small enough to be a viable attack.
        assert!(
            bomb.len() < 1_000_000,
            "compressed bomb should be tiny relative to expanded size"
        );
        let err = maybe_gunzip(&bomb).expect_err("must reject oversized");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn maybe_gunzip_accepts_payload_at_or_below_limit() {
        // Boundary check: a payload that decompresses to exactly the cap
        // should still pass. A regression that uses `<=` instead of `<`
        // for the rejection comparison would fail this test.
        let payload = vec![b'x'; MAX_DECOMPRESSED];
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&payload).unwrap();
        let gz = enc.finish().unwrap();
        let out = maybe_gunzip(&gz).expect("payload at limit must decode");
        assert_eq!(out.len(), MAX_DECOMPRESSED);
    }

    // ----- read_line edge cases -----

    #[test]
    fn read_line_returns_position_after_newline() {
        let buf = b"first\nsecond";
        let (end, next) = read_line(buf, 0).unwrap();
        assert_eq!(end, 6, "end is one past the newline");
        assert_eq!(next, 6);
    }

    #[test]
    fn read_line_handles_no_trailing_newline() {
        let buf = b"only-line";
        let (end, next) = read_line(buf, 0).unwrap();
        assert_eq!(end, buf.len());
        assert_eq!(next, buf.len());
    }

    #[test]
    fn read_line_returns_none_at_eof() {
        let buf = b"abc";
        assert!(read_line(buf, 3).is_none());
    }
}
