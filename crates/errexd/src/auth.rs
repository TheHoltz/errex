//! Cookie-session authentication.
//!
//! Replaces the single shared `Authorization: Bearer <admin-token>` model
//! with per-user sessions. The bearer env-var lives on as a one-shot setup
//! secret consumed by `/api/auth/setup` when the `users` table is empty.
//!
//! Wire shape:
//!
//!   - `POST /api/auth/setup { token, username, password }` — bootstrap
//!     the first admin. 409 once any users exist (the env-var path is
//!     permanently disabled).
//!   - `POST /api/auth/login { username, password }` — sets `errex_session`
//!     cookie. Lockout middleware fires before password check.
//!   - `POST /api/auth/logout` — deletes the server-side session row and
//!     clears the cookie.
//!   - `GET  /api/auth/me` — returns `{ username, role }` for the current
//!     session, or 401.
//!
//! Cookie attributes: `HttpOnly` (no JS access — XSS resistance), `Secure`
//! (skipped in `ERREXD_DEV_MODE` so `http://localhost` works), `SameSite=Strict`
//! (CSRF resistance — the SPA is same-origin so this costs us nothing),
//! `Path=/`, `Max-Age=2592000` (30 days, sliding via `touch_session`).

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Json, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::Deserialize;

use crate::crypto;
use crate::ingest::{ApiError, AppState};
use crate::lockout::LockoutPolicy;
use crate::store::{Role, Session};

const COOKIE_NAME: &str = "errex_session";
const SESSION_TTL_SECS: i64 = 30 * 24 * 60 * 60; // 30 days, sliding

// ----- request shapes -----

#[derive(Debug, Deserialize)]
pub struct SetupBody {
    pub token: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub username: String,
    pub password: String,
}

// ----- cookie helpers -----

/// Reads the session id out of the `Cookie` header. Quote-stripping is
/// intentional — some cookie writers wrap values in `"…"`. We don't trust
/// the value, just look it up; bogus ids miss in `session_for_id` and
/// produce 401 the same way an absent cookie does.
pub fn extract_session_id(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get("cookie").and_then(|v| v.to_str().ok())?;
    for kv in raw.split(';') {
        let kv = kv.trim();
        let Some((k, v)) = kv.split_once('=') else {
            continue;
        };
        if k == COOKIE_NAME {
            let v = v.trim_matches('"');
            return if v.is_empty() {
                None
            } else {
                Some(v.to_string())
            };
        }
    }
    None
}

fn build_session_cookie(id: &str, dev_mode: bool) -> String {
    // Secure is required by browsers for SameSite=None, but we use Strict so
    // the only reason to set Secure is to refuse the cookie over plaintext
    // HTTP in production. In dev we need to drop it because Vite + the
    // daemon both run on http://localhost.
    let secure = if dev_mode { "" } else { "; Secure" };
    format!(
        "{COOKIE_NAME}={id}; HttpOnly{secure}; SameSite=Strict; Path=/; Max-Age={SESSION_TTL_SECS}"
    )
}

fn build_clearing_cookie(dev_mode: bool) -> String {
    let secure = if dev_mode { "" } else { "; Secure" };
    format!("{COOKIE_NAME}=; HttpOnly{secure}; SameSite=Strict; Path=/; Max-Age=0")
}

/// Best-effort client-IP extractor. Order:
///   1. `X-Forwarded-For` (first hop) — covers reverse-proxy deployments.
///   2. `ConnectInfo<SocketAddr>` — direct connection IP.
///   3. `"unknown"` — never observed in practice but the bucket key has
///      to be stable, so we coalesce instead of returning Option.
pub fn extract_client_ip(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            let trimmed = first.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    peer.map(|s| s.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn extract_user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

// ----- request-context resolution -----

/// Resolved-on-every-request authentication context. Returned by both
/// `require_auth` and `require_admin` so handlers don't have to re-fetch
/// the session row.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub session: Session,
    pub role: Role,
}

/// Resolves the current session and bumps `last_seen_at` (sliding expiry).
/// Returns 401 for: no cookie, unknown cookie, deactivated user.
#[allow(clippy::result_large_err)] // mirrors check_admin's pattern; Response is small enough
pub async fn require_auth(state: &AppState, headers: &HeaderMap) -> Result<AuthContext, Response> {
    let Some(id) = extract_session_id(headers) else {
        return Err((StatusCode::UNAUTHORIZED, "not signed in").into_response());
    };
    let lookup = state.store.session_for_id(&id).await.map_err(|err| {
        tracing::error!(%err, "session lookup failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
    })?;
    let Some((session, role, _deactivated)) = lookup else {
        // The query already filters deactivated users (sessions cascade on
        // deactivation), so reaching here for a missing row is the normal
        // "expired/revoked/never-existed" path.
        return Err((StatusCode::UNAUTHORIZED, "session invalid").into_response());
    };
    // Sliding expiry: every authenticated request advances the window.
    state.store.touch_session(&session.id).await;
    Ok(AuthContext { session, role })
}

/// Like `require_auth` but additionally rejects non-admin sessions with 403.
/// Used by `/api/admin/*` and any route only an operator should reach.
#[allow(clippy::result_large_err)]
pub async fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<AuthContext, Response> {
    let ctx = require_auth(state, headers).await?;
    if !ctx.role.is_admin() {
        return Err((StatusCode::FORBIDDEN, "admin role required").into_response());
    }
    Ok(ctx)
}

// ----- handlers -----

/// First-user creation. Required on a fresh install. Once the `users`
/// table has any row, this endpoint returns 409 forever.
pub async fn handle_setup(
    State(state): State<Arc<AppState>>,
    peer: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    body: Option<Json<SetupBody>>,
) -> Result<Response, ApiError> {
    let Some(Json(body)) = body else {
        return Ok((StatusCode::BAD_REQUEST, "missing body").into_response());
    };

    if state.store.user_count().await? > 0 {
        return Ok((StatusCode::CONFLICT, "setup already complete").into_response());
    }

    // The env-var IS the setup secret. Without it, the daemon has no way
    // to distinguish a legitimate operator from a stranger who got there
    // first. We reject upfront if the operator never configured one.
    let Some(expected) = state.setup_token.as_deref() else {
        return Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            "setup is disabled (set ERREXD_ADMIN_TOKEN to enable)",
        )
            .into_response());
    };
    if body.token != expected {
        return Ok((StatusCode::UNAUTHORIZED, "invalid setup token").into_response());
    }

    let username = body.username.trim();
    if username.is_empty() || username.len() > 64 {
        return Ok((StatusCode::BAD_REQUEST, "username 1..=64 chars").into_response());
    }
    if let Err(why) = crypto::validate_password_strength(&body.password) {
        return Ok((StatusCode::BAD_REQUEST, why).into_response());
    }

    let hash = match crypto::hash_password(&body.password) {
        Ok(h) => h,
        Err(err) => {
            tracing::error!(%err, "argon2 hash failed");
            return Ok((StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response());
        }
    };
    state
        .store
        .create_user(username, &hash, Role::Admin)
        .await?;

    // Sign the operator in immediately — they just proved they have host
    // access AND chose a password; making them re-type it would be silly.
    let id = crypto::generate_session_id();
    let ip = extract_client_ip(&headers, peer.map(|c| c.0));
    let ua = extract_user_agent(&headers);
    state
        .store
        .create_session(&id, username, Some(&ip), ua.as_deref())
        .await?;
    state.store.touch_user_login(username, Some(&ip)).await;

    let mut resp = (
        StatusCode::CREATED,
        Json(serde_json::json!({"username": username, "role": "admin"})),
    )
        .into_response();
    resp.headers_mut().append(
        axum::http::header::SET_COOKIE,
        HeaderValue::from_str(&build_session_cookie(&id, state.dev_mode)).expect("ascii cookie"),
    );
    Ok(resp)
}

pub async fn handle_login(
    State(state): State<Arc<AppState>>,
    peer: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    body: Option<Json<LoginBody>>,
) -> Result<Response, ApiError> {
    let Some(Json(body)) = body else {
        return Ok((StatusCode::BAD_REQUEST, "missing body").into_response());
    };
    let username = body.username.trim().to_string();
    let ip = extract_client_ip(&headers, peer.map(|c| c.0));

    // Lockout check: read the recent-failure ledger, decide, before doing
    // any password work. The hash compare is the expensive bit; gating it
    // on the cheap query makes the lockout actually help with DOS.
    let policy = LockoutPolicy::default();
    let since = Utc::now() - policy.window;
    let user_failures = if username.is_empty() {
        0
    } else {
        state
            .store
            .count_recent_failures_for_username(&username, since)
            .await?
    };
    let ip_failures = state.store.count_recent_failures_for_ip(&ip, since).await?;
    if let Some(retry_after) = policy
        .evaluate(user_failures, ip_failures)
        .retry_after_secs()
    {
        return Ok((
            StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::RETRY_AFTER, retry_after.to_string())],
            "too many attempts — try again later",
        )
            .into_response());
    }

    // Hash-anyway pattern: do the argon2 verify even when the user doesn't
    // exist, against a constant garbage hash, so we don't leak user
    // existence by timing. The control-flow path is identical between
    // "wrong password" and "no such user".
    let stored_hash = state
        .store
        .get_user_password_hash(&username)
        .await?
        .unwrap_or_else(|| {
            // Hash of a random impossible password, generated once at boot.
            // Pre-computed in OnceLock would be faster but the per-failure
            // cost is bounded by the lockout above.
            "$argon2id$v=19$m=19456,t=2,p=1$abcdefghijklmnopqrstuv$\
             aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string()
        });
    let ok = crypto::verify_password(&stored_hash, &body.password);
    let user_exists_active = state
        .store
        .get_user(&username)
        .await?
        .filter(|u| u.deactivated_at.is_none())
        .is_some();

    if !ok || !user_exists_active {
        state
            .store
            .record_attempt(
                Some(username.as_str()).filter(|s| !s.is_empty()),
                &ip,
                false,
            )
            .await;
        return Ok((StatusCode::UNAUTHORIZED, "invalid credentials").into_response());
    }

    state.store.record_attempt(Some(&username), &ip, true).await;
    state.store.touch_user_login(&username, Some(&ip)).await;

    let id = crypto::generate_session_id();
    let ua = extract_user_agent(&headers);
    state
        .store
        .create_session(&id, &username, Some(&ip), ua.as_deref())
        .await?;
    let user = state
        .store
        .get_user(&username)
        .await?
        .expect("just-verified user must exist");

    let mut resp = (
        StatusCode::OK,
        Json(serde_json::json!({"username": user.username, "role": user.role})),
    )
        .into_response();
    resp.headers_mut().append(
        axum::http::header::SET_COOKIE,
        HeaderValue::from_str(&build_session_cookie(&id, state.dev_mode)).expect("ascii cookie"),
    );
    Ok(resp)
}

pub async fn handle_logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(id) = extract_session_id(&headers) {
        state.store.revoke_session(&id).await?;
    }
    let mut resp = StatusCode::NO_CONTENT.into_response();
    resp.headers_mut().append(
        axum::http::header::SET_COOKIE,
        HeaderValue::from_str(&build_clearing_cookie(state.dev_mode)).expect("ascii cookie"),
    );
    Ok(resp)
}

pub async fn handle_me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    match require_auth(&state, &headers).await {
        Ok(ctx) => Ok(Json(serde_json::json!({
            "username": ctx.session.username,
            "role": ctx.role,
        }))
        .into_response()),
        Err(resp) => Ok(resp),
    }
}

/// Public endpoint that lets the SPA decide whether to land on `/setup` or
/// `/login` without leaking sensitive info. Returns `{ needs_setup: bool }`.
/// `needs_setup` is true iff the `users` table is empty AND a setup token
/// is configured (no token configured = setup is administratively
/// disabled, so the SPA should not invite anyone to try).
pub async fn handle_setup_status(State(state): State<Arc<AppState>>) -> Result<Response, ApiError> {
    let no_users = state.store.user_count().await? == 0;
    let token_configured = state.setup_token.is_some();
    Ok(Json(serde_json::json!({
        "needs_setup": no_users && token_configured,
        "setup_disabled": no_users && !token_configured,
    }))
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with_cookie(s: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("cookie", HeaderValue::from_str(s).unwrap());
        h
    }

    #[test]
    fn extract_session_id_returns_value_when_present() {
        let h = headers_with_cookie("errex_session=abcdef; other=ignored");
        assert_eq!(extract_session_id(&h).as_deref(), Some("abcdef"));
    }

    #[test]
    fn extract_session_id_handles_quoted_value() {
        let h = headers_with_cookie(r#"errex_session="abcdef""#);
        assert_eq!(extract_session_id(&h).as_deref(), Some("abcdef"));
    }

    #[test]
    fn extract_session_id_returns_none_when_cookie_missing() {
        let h = headers_with_cookie("other=value");
        assert!(extract_session_id(&h).is_none());
    }

    #[test]
    fn extract_session_id_returns_none_for_empty_value() {
        let h = headers_with_cookie("errex_session=");
        assert!(extract_session_id(&h).is_none());
    }

    #[test]
    fn extract_session_id_returns_none_when_header_missing() {
        let h = HeaderMap::new();
        assert!(extract_session_id(&h).is_none());
    }

    #[test]
    fn build_session_cookie_includes_secure_in_prod() {
        let c = build_session_cookie("abc", false);
        assert!(c.contains("Secure"));
        assert!(c.contains("HttpOnly"));
        assert!(c.contains("SameSite=Strict"));
        assert!(c.contains("Max-Age=2592000"));
    }

    #[test]
    fn build_session_cookie_drops_secure_in_dev() {
        let c = build_session_cookie("abc", true);
        assert!(!c.contains("Secure"));
        assert!(c.contains("HttpOnly"));
    }

    #[test]
    fn build_clearing_cookie_zeros_max_age() {
        let c = build_clearing_cookie(false);
        assert!(c.contains("Max-Age=0"));
    }

    #[test]
    fn extract_client_ip_prefers_xff_over_peer() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", HeaderValue::from_static("1.2.3.4"));
        let peer: SocketAddr = "9.9.9.9:1234".parse().unwrap();
        assert_eq!(extract_client_ip(&h, Some(peer)), "1.2.3.4");
    }

    #[test]
    fn extract_client_ip_uses_first_xff_hop() {
        let mut h = HeaderMap::new();
        h.insert(
            "x-forwarded-for",
            HeaderValue::from_static("1.2.3.4, 5.6.7.8"),
        );
        assert_eq!(extract_client_ip(&h, None), "1.2.3.4");
    }

    #[test]
    fn extract_client_ip_falls_back_to_peer() {
        let h = HeaderMap::new();
        let peer: SocketAddr = "9.9.9.9:1234".parse().unwrap();
        assert_eq!(extract_client_ip(&h, Some(peer)), "9.9.9.9");
    }

    #[test]
    fn extract_client_ip_falls_back_to_unknown_when_nothing() {
        let h = HeaderMap::new();
        assert_eq!(extract_client_ip(&h, None), "unknown");
    }
}
