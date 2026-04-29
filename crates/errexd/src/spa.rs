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

/// Hand-rolled mime lookup. The SPA build emits a small, known set of
/// file types; a full mime database (mime_guess) carried hundreds of
/// mappings as static tables that just sat in the binary's resident
/// pages.
///
/// Unknown extensions fall back to `application/octet-stream`. The
/// `spa_mime_coverage` integration test asserts every extension that
/// actually appears in `web/build/` is covered explicitly so a future
/// SvelteKit build that emits, say, `.map` sourcemaps or `.wasm`
/// modules can't silently regress to the fallback.
pub(crate) fn mime_for_path(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("ico") => "image/x-icon",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("gif") => "image/gif",
        Some("wasm") => "application/wasm",
        Some("map") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn file_response(path: &str, body: &[u8]) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_for_path(path))
        .body(Body::from(body.to_vec()))
        .expect("static response builder")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_extensions_map_correctly() {
        assert_eq!(mime_for_path("index.html"), "text/html; charset=utf-8");
        assert_eq!(mime_for_path("a/b.js"), "application/javascript");
        assert_eq!(mime_for_path("0.css"), "text/css; charset=utf-8");
        assert_eq!(mime_for_path("env.json"), "application/json");
        assert_eq!(mime_for_path("favicon.svg"), "image/svg+xml");
        assert_eq!(mime_for_path("font.woff2"), "font/woff2");
        assert_eq!(mime_for_path("robots.txt"), "text/plain; charset=utf-8");
    }

    #[test]
    fn defensive_extensions_for_future_spa_builds() {
        // Cover the extensions a future SvelteKit build is most likely to
        // start emitting (sourcemaps, raster images, wasm). These don't
        // appear in today's build but should not silently regress to
        // octet-stream when they do — browsers refuse to execute wasm
        // served as octet-stream.
        assert_eq!(mime_for_path("chunk.js.map"), "application/json");
        assert_eq!(mime_for_path("hero.png"), "image/png");
        assert_eq!(mime_for_path("hero.webp"), "image/webp");
        assert_eq!(mime_for_path("worker.wasm"), "application/wasm");
        assert_eq!(mime_for_path("favicon.ico"), "image/x-icon");
    }

    #[test]
    fn unknown_extension_falls_back_to_octet_stream() {
        assert_eq!(mime_for_path("x.xyzqq"), "application/octet-stream");
        assert_eq!(mime_for_path("noext"), "application/octet-stream");
    }

    /// Scan the actual `web/build/` directory and assert every
    /// extension that landed in it has an explicit mapping. Skips
    /// silently when the directory is empty (CI may run before
    /// `bun run build`); fails when *any* file under the tree has
    /// an extension that falls back to `application/octet-stream`.
    /// This is the build-time tripwire for "SvelteKit started
    /// emitting a new format and we forgot to update the lookup".
    #[test]
    fn spa_build_extensions_all_covered() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let build_dir = std::path::Path::new(manifest_dir).join("../../web/build");
        let Ok(build_dir) = build_dir.canonicalize() else {
            // `web/build/` not present (e.g. fresh CI checkout before
            // bun build). Nothing to scan; let other tests cover.
            return;
        };

        let mut uncovered: Vec<(String, String)> = Vec::new();
        walk(&build_dir, &mut |path| {
            let path_str = path.to_string_lossy().into_owned();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            if mime_for_path(&path_str) == "application/octet-stream" {
                uncovered.push((path_str, ext));
            }
        });

        assert!(
            uncovered.is_empty(),
            "SPA file(s) under web/build/ have no mime mapping (would be served as application/octet-stream): {:#?}",
            uncovered,
        );
    }

    fn walk(root: &std::path::Path, cb: &mut dyn FnMut(&std::path::Path)) {
        let Ok(rd) = std::fs::read_dir(root) else {
            return;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, cb);
            } else if p.is_file() {
                cb(&p);
            }
        }
    }
}
