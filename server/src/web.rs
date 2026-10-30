//! Serves the built Svelte UI from a folder (0.11; 632 and the owner, 2026-10-30: a page-only fix ships without a Rust
//! build): `CRATARR_WEB_DIR`, else the image's `/web`, else `../web/dist` beside the source (development: a
//! `pnpm build` shows up at once). Read from disk on each request; the system keeps what is asked often.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use axum::{
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};

/// The page's folder (above), found once.
pub fn dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(set) = std::env::var_os("CRATARR_WEB_DIR").filter(|v| !v.is_empty()) {
            return PathBuf::from(set);
        }
        let image = PathBuf::from("/web");
        if image.is_dir() { image } else { PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../web/dist")) }
    })
}

pub async fn serve(uri: Uri) -> Response {
    serve_from(dir(), uri.path()).await
}

async fn serve_from(dir: &Path, path: &str) -> Response {
    let path = path.trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let wanted = if path.is_empty() { "index.html" } else { path };
    if let Some(file) = read(dir, wanted).await {
        return respond(wanted, file);
    }
    // Client-side routes (no file extension) fall back to the app shell.
    if !path.contains('.') {
        if let Some(index) = read(dir, "index.html").await {
            return respond("index.html", index);
        }
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "The web UI is not built. Run `pnpm build` in web/, or set CRATARR_WEB_DIR to its dist folder.",
        )
            .into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

/// A file of the page's folder: plain names only, never out of it.
async fn read(dir: &Path, rel: &str) -> Option<Vec<u8>> {
    if rel.split('/').any(|p| p.is_empty() || p == "." || p == "..") || rel.contains(['\\', ':']) {
        return None;
    }
    let path = dir.join(rel);
    if !tokio::fs::metadata(&path).await.ok()?.is_file() {
        return None;
    }
    tokio::fs::read(path).await.ok()
}

/// The page's build as its `build.json` says (decision 0085, step 1: `{version, build}`), for apps that carry their own
/// copy of the page; none when the page was built without it.
pub fn page_build() -> Option<serde_json::Value> {
    std::fs::read(dir().join("build.json")).ok().and_then(|data| serde_json::from_slice(&data).ok())
}

/// The guest portal's page (decision 0079), the build's second entry: none until it is built.
pub async fn guest_page() -> Response {
    match read(dir(), "guest.html").await {
        Some(file) => respond("guest.html", file),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn respond(path: &str, file: Vec<u8>) -> Response {
    // Vite fingerprints everything under assets/, so those files can be cached forever; the shell must not be.
    let cache = if path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    ([(header::CONTENT_TYPE, type_of(path)), (header::CACHE_CONTROL, cache)], file).into_response()
}

/// The types the page's build holds (the embedded files' guess before 0.11 gave the same).
fn type_of(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).as_deref() {
        Some("html") => "text/html",
        Some("js" | "mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("gif") => "image/gif",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("wasm") => "application/wasm",
        Some("txt") => "text/plain",
        Some("map") => "application/json",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page folder of its own for one test.
    fn page(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cratarr-web-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), "<!doctype html>shell").unwrap();
        std::fs::write(dir.join("assets/main-abc.js"), "1").unwrap();
        dir
    }

    async fn body(r: Response) -> String {
        String::from_utf8(axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap()
    }

    #[tokio::test]
    async fn unknown_api_paths_are_404_not_the_app_shell() {
        let dir = page("api");
        assert_eq!(serve_from(&dir, "/api/does-not-exist").await.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn missing_static_file_with_extension_is_404() {
        let dir = page("missing");
        assert_eq!(serve_from(&dir, "/nope.js").await.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn the_folders_files_with_their_types_and_caching() {
        let dir = page("files");
        let r = serve_from(&dir, "/assets/main-abc.js").await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()[header::CONTENT_TYPE], "text/javascript");
        assert_eq!(r.headers()[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
        let r = serve_from(&dir, "/").await;
        assert_eq!(r.headers()[header::CACHE_CONTROL], "no-cache");
        assert_eq!(body(r).await, "<!doctype html>shell");
        // A route of the page's own: the shell.
        assert_eq!(body(serve_from(&dir, "/library/albums").await).await, "<!doctype html>shell");
    }

    #[tokio::test]
    async fn nothing_outside_the_folder() {
        let dir = page("outside");
        std::fs::write(dir.parent().unwrap().join("cratarr-web-secret.txt"), "no").unwrap();
        for path in ["/../cratarr-web-secret.txt", "/assets/../../cratarr-web-secret.txt", "/assets//main-abc.js", "/C:/x.txt"] {
            assert_eq!(serve_from(&dir, path).await.status(), StatusCode::NOT_FOUND, "{path}");
        }
        // A folder is not a file: its route is the shell, not the folder.
        assert_eq!(body(serve_from(&dir, "/assets").await).await, "<!doctype html>shell");
    }
}
