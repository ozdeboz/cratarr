//! `GET /api/auth/check` (decisions 0039 and 0043): whether a request may pass, for a reverse proxy's forward auth
//! (Caddy's `forward_auth`, Traefik's `forwardAuth`, nginx's `auth_request`). The proxy asks it before passing a request
//! on, with the request's headers and its method and URI (`X-Forwarded-Method`, `X-Forwarded-Uri`), and hands any answer
//! but 2xx to the browser as it is.
//!
//! The same rules as Cratarr's own (`access`), whether or not the requirement is on there: a signed-in device's
//! session passes as its account (`X-Cratarr-User`), and so do an API key and a signed link; the sign-in page and what
//! it shows, the health check and the OpenSubsonic API are open. Otherwise a page load goes to the sign-in page (303,
//! and back after) and anything else gets 401 with a `Cookie` challenge.

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use super::Serve;
use super::access::{admitted, note_refused, refuse};
use super::signin::session_of;

pub fn router(serve: Arc<Serve>) -> Router {
    Router::new().route("/api/auth/check", get(check)).with_state(serve)
}

fn text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty())
}

async fn check(State(serve): State<Arc<Serve>>, headers: HeaderMap) -> Response {
    if let Some(s) = session_of(&serve, &headers) {
        return (StatusCode::NO_CONTENT, [("x-cratarr-user", HeaderValue::from_str(&s.user).unwrap_or(HeaderValue::from_static("")))]).into_response();
    }
    let uri = text(&headers, "x-forwarded-uri").unwrap_or("/");
    let method = text(&headers, "x-forwarded-method").unwrap_or("GET");
    let (path, query) = uri.split_once('?').map_or((uri, None), |(p, q)| (p, Some(q)));
    if admitted(&serve, method, &headers, path, query) {
        return StatusCode::NO_CONTENT.into_response();
    }
    let r = refuse(method, &headers, uri);
    // The client as the proxy names it (X-Forwarded-For).
    note_refused(&r, &headers, None, path, query, "proxy");
    r
}

#[cfg(test)]
mod tests {
    use super::super::access::{page_load, public};
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn only_plain_public_paths_pass_without_a_session() {
        for uri in ["/signin", "/signin?next=%2F", "/signout", "/manifest.webmanifest", "/sw.js", "/icons/icon-192.png", "/rest/ping.view?u=me", "/api/health", "/guest", "/api/guest/now", "/assets/index-abc.js"] {
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
            "/api/healthz",
            "/guests",
            "/assets/../api/library",
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
        assert!(page_load("GET", &h(&[("sec-fetch-mode", "navigate")])));
        assert!(!page_load("GET", &h(&[("sec-fetch-mode", "cors"), ("accept", "text/html")])));
        assert!(!page_load("POST", &h(&[("sec-fetch-mode", "navigate")])));
        assert!(page_load("GET", &h(&[("accept", "text/html,application/xhtml+xml")])), "a browser without Sec-Fetch");
        assert!(!page_load("GET", &h(&[("accept", "*/*")])), "a player's stream");
        assert!(!page_load("GET", &h(&[])));
    }
}
