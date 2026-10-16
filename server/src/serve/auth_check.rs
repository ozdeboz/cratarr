//! `GET /api/auth/check` (decision 0039): whether a request may pass, for a reverse proxy's forward auth (Caddy's
//! `forward_auth`, Traefik's `forwardAuth`, nginx's `auth_request`). The proxy asks it before passing a request on, with
//! the request's headers and its method and URI (`X-Forwarded-Method`, `X-Forwarded-Uri`), and hands any answer but
//! 2xx to the browser as it is. Which requests are asked is the proxy's choice: Cratarr's own pages need no sign-in at
//! home.
//!
//! A signed-in device's session (the cookie from Cratarr's sign-in page, `signin`) passes, as its account
//! (`X-Cratarr-User`). So do the sign-in page itself, the web app's manifest, service worker and icons, and the
//! OpenSubsonic API (which checks its own logins). Otherwise a page load goes to the sign-in page (303, and back after),
//! and anything else is refused: 401 with a `Cookie` challenge, which no browser answers with a login box of its own;
//! the page and the app go to the sign-in page themselves.

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;

use super::Serve;
use super::signin::{COOKIE, path_of, session_of};

pub fn router(serve: Arc<Serve>) -> Router {
    Router::new().route("/api/auth/check", get(check)).with_state(serve)
}

fn text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty())
}

/// What anyone may load: the sign-in page (and signing out), the web app's manifest, service worker and icons, and the
/// OpenSubsonic API. Plain paths only: none that a server could read as another one (`..`, encoded dots or slashes,
/// doubled slashes).
fn public(uri: &str) -> bool {
    let path = path_of(uri);
    let lower = path.to_ascii_lowercase();
    let plain = !path.contains("..") && !path.contains("//") && !path.contains('\\') && !["%2e", "%2f", "%5c"].iter().any(|e| lower.contains(e));
    plain && (matches!(path, "/signin" | "/signout" | "/manifest.webmanifest" | "/sw.js") || path.starts_with("/icons/") || path.starts_with("/rest/"))
}

/// A browser loading a page (not a script's request, nor a picture's): it can be sent to the sign-in page.
fn page_load(headers: &HeaderMap) -> bool {
    if !matches!(text(headers, "x-forwarded-method").unwrap_or("GET"), "GET" | "HEAD") {
        return false;
    }
    match text(headers, "sec-fetch-mode") {
        Some(mode) => mode == "navigate",
        None => text(headers, "accept").is_some_and(|a| a.contains("text/html")),
    }
}

async fn check(State(serve): State<Arc<Serve>>, headers: HeaderMap) -> Response {
    if let Some(s) = session_of(&serve, &headers) {
        return (StatusCode::NO_CONTENT, [("x-cratarr-user", HeaderValue::from_str(&s.user).unwrap_or(HeaderValue::from_static("")))]).into_response();
    }
    let uri = text(&headers, "x-forwarded-uri").unwrap_or("/");
    if public(uri) {
        return StatusCode::NO_CONTENT.into_response();
    }
    if page_load(&headers) {
        let query = serde_urlencoded::to_string([("next", uri)]).unwrap_or_default();
        let mut r = StatusCode::SEE_OTHER.into_response();
        r.headers_mut().insert(header::LOCATION, HeaderValue::from_str(&format!("/signin?{query}")).unwrap_or(HeaderValue::from_static("/signin")));
        r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        return r;
    }
    let challenge = format!("Cookie realm=\"Cratarr\", form-action=\"/signin\", cookie-name=\"{COOKIE}\"");
    let mut r = (StatusCode::UNAUTHORIZED, axum::Json(json!({ "error": "Sign in to Cratarr.", "signin": "/signin" }))).into_response();
    r.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_str(&challenge).expect("ASCII"));
    r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_public_paths_pass_without_a_session() {
        for uri in ["/signin", "/signin?next=%2F", "/signout", "/manifest.webmanifest", "/sw.js", "/icons/icon-192.png", "/rest/ping.view?u=me"] {
            assert!(public(uri), "{uri}");
        }
        for uri in [
            "/",
            "/api/library",
            "/signin/x",
            "/signinx",
            "/rest/../api/library",
            "/icons/..%2Fapi",
            "/icons/%2e%2e/api",
            "//rest/x",
            "/rest\\..\\api",
            "/api/x?next=/signin",
        ] {
            assert!(!public(uri), "{uri}");
        }
    }

    #[test]
    fn a_page_load_is_a_browser_navigating() {
        let h = |pairs: &[(&'static str, &'static str)]| {
            let mut m = HeaderMap::new();
            for (k, v) in pairs {
                m.insert(*k, HeaderValue::from_static(v));
            }
            m
        };
        assert!(page_load(&h(&[("sec-fetch-mode", "navigate")])));
        assert!(!page_load(&h(&[("sec-fetch-mode", "cors"), ("accept", "text/html")])));
        assert!(!page_load(&h(&[("sec-fetch-mode", "navigate"), ("x-forwarded-method", "POST")])));
        assert!(page_load(&h(&[("accept", "text/html,application/xhtml+xml")])), "a browser without Sec-Fetch");
        assert!(!page_load(&h(&[("accept", "*/*")])), "a player's stream");
        assert!(!page_load(&h(&[])));
    }
}
