//! SPA static-asset handler.
//!
//! At compile time, `rust-embed` bundles every file under `web/build/` into
//! the binary. At request time, this module looks up `path` directly; if it's
//! not a hashed asset, it serves `index.html` so the client-side router can
//! handle deep links (`/issues/42`, etc.).
//!
//! The build directory is created by `bun run build` in `web/`. When that has
//! never been run, the directory is empty and every request 404s — wire this
//! to the bun build step in CI / Docker.

use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../web/build/"]
struct Assets;

pub async fn handler(uri: Uri) -> Response {
    let raw_path = uri.path();

    // Unknown `/api/*` paths must NOT serve index.html — a typo in a fetch
    // call would otherwise return 200 + HTML, and a JSON-expecting client
    // (the SPA, curl, an SDK) would surface "Unexpected token '<'" instead
    // of "endpoint missing". Return a clean JSON 404 so callers get a
    // useful error.
    if raw_path.starts_with("/api/") {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"endpoint not found"}"#,
        )
            .into_response();
    }

    let path = raw_path.trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = Assets::get(path) {
        return file_response(path, file.data.as_ref());
    }

    // SPA fallback: any non-asset path serves index.html so SvelteKit's
    // client router can take over.
    if let Some(file) = Assets::get("index.html") {
        return file_response("index.html", file.data.as_ref());
    }

    // Build hasn't been produced yet. Surface a clear message rather than a
    // bare 404 so dev iterations don't waste time chasing routing bugs.
    (
        StatusCode::NOT_FOUND,
        "errexd: web/build/ is empty — run `bun run build` in web/ or rebuild the container.",
    )
        .into_response()
}

fn file_response(path: &str, body: &[u8]) -> Response {
    // Hand-rolled lookup: the SPA build emits seven file types; a full
    // mime database (mime_guess) carries hundreds of mappings as static
    // tables that just sit in the binary's resident pages.
    let mime = match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(body.to_vec()))
        .expect("static response builder")
}
