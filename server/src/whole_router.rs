//! The whole router, swept (the audit, 2026-10-29: one harness over all the routes). Every route the source names
//! (`.route("…", …)` outside tests) is asked for twice through the router `main` serves: with no way in it is refused
//! before any handler runs (only the open routes pass), and signed in as an administrator its path is there (a method
//! it has no handler for answers 405, not the fallback's). No handler runs, so nothing is asked of a source or the
//! network.

use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;

/// Each route the source names: its path (each `{param}` a plain value) and its methods.
fn named_routes() -> Vec<(String, Vec<Method>)> {
    let mut files = Vec::new();
    let mut dirs = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") && p.file_name().is_some_and(|n| n != "whole_router.rs") {
                files.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        // The tests' own routers are not the server's.
        let text = text.split("#[cfg(test)]").next().unwrap_or("");
        let mut rest = text;
        while let Some(at) = rest.find(".route(\"") {
            rest = &rest[at + 8..];
            let Some(end) = rest.find('"') else { break };
            let path = rest[..end].to_string();
            if !path.starts_with('/') {
                continue;
            }
            let call = &rest[end..rest[end..].find(".route(\"").map_or(rest.len(), |n| end + n)];
            let methods: Vec<Method> = [("get(", Method::GET), ("post(", Method::POST), ("put(", Method::PUT), ("delete(", Method::DELETE), ("patch(", Method::PATCH)]
                .into_iter()
                .filter(|(name, _)| call.match_indices(name).any(|(i, _)| i == 0 || !call.as_bytes()[i - 1].is_ascii_alphanumeric() && call.as_bytes()[i - 1] != b'_'))
                .map(|(_, m)| m)
                .collect();
            let plain = path.split('/').map(|part| if part.starts_with('{') { "x" } else { part }).collect::<Vec<_>>().join("/");
            out.push((plain, methods));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

#[tokio::test]
async fn every_route_is_behind_the_sign_in_and_is_there() {
    let dir = std::env::temp_dir().join(format!("cratarr-whole-router-{}", std::process::id()));
    std::fs::create_dir_all(dir.join(crate::playlists::PLAYLIST_DIR)).unwrap();
    let served = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
    served.users.set("me", "pw").unwrap();
    served.sessions.set_required(true).unwrap();
    let (token, _) = served.sessions.create("me", "the sweep", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64).unwrap();
    let cfg = crate::config::Config::from_lookup(|_| None).unwrap();
    let saved = crate::settings::Saved { music_folder: Some(dir.to_string_lossy().into_owned()), ..Default::default() };
    let app = crate::app::App::new(cfg, saved, crate::settings::Origin::Env, Some(Arc::clone(&served))).await.unwrap();
    let router = super::routes(app, Arc::clone(&served));

    let routes = named_routes();
    assert!(routes.len() > 150, "the source's routes found: {}", routes.len());
    let mut open = Vec::new();
    for (path, methods) in &routes {
        for m in methods {
            let req = Request::builder().method(m.clone()).uri(path.as_str()).header("accept", "application/json").body(Body::empty()).unwrap();
            let status = router.clone().oneshot(req).await.unwrap().status();
            if crate::serve::access::public(path) {
                open.push(path.clone());
            } else {
                assert_eq!(status, StatusCode::UNAUTHORIZED, "{m} {path} without a way in");
            }
        }
        // The guest page and its routes answer only at home (decision 0079): away, as this sweep is, nothing is there.
        if path == "/guest" || path.starts_with("/api/guest/") {
            continue;
        }
        // A method no route has: the path is there (405), not the fallback's.
        let req = Request::builder()
            .method(Method::from_bytes(b"PROPFIND").unwrap())
            .uri(path.as_str())
            .header("cookie", format!("{}={token}", crate::serve::signin::COOKIE))
            .body(Body::empty())
            .unwrap();
        let status = router.clone().oneshot(req).await.unwrap().status();
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{path}: not in the router main serves");
    }
    open.sort();
    open.dedup();
    assert!(open.iter().all(|p| p == "/signin" || p == "/signout" || p.starts_with("/api/health") || p.starts_with("/api/auth/check") || p.starts_with("/rest/") || p.starts_with("/api/scrobble/lastfm/callback/") || p == "/api/spotify/callback" || p == "/guest" || p.starts_with("/api/guest/")), "open: {open:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
