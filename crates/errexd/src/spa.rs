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
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = Assets::get(path) {
        return file_response(path, file.data.as_ref());
    }

    // SPA fallback: any non-asset path serves index.html so SvelteKit's
    // client router can take over. Requests under /api/* never reach here —
    // they're handled by the explicit routes above this fallback.
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
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_string();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(body.to_vec()))
        .expect("static response builder")
}
