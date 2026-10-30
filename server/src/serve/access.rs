//! Sign-in everywhere (decision 0043): Cratarr itself lets a request through only with a session from its sign-in page
//! (decision 0039), an API key (`Authorization: Bearer cratarr_…`, made in Settings → Authentication or on the server),
//! or a signed link (for players that cannot send a header: Kodi's playlist and the group's sound). Open to anyone: the
//! sign-in page and what it shows, the health check, the proxy's check, and the Subsonic API, which checks its own
//! logins. While the requirement is off (an install from before it, until it is switched on), or while Cratarr has no
//! account yet (a new install's first account is made on its page), everything passes.
//!
//! Switching the requirement and making a key need the account's password again (as Change password does): someone
//! at a page left open, or on the home network before the requirement is on, cannot turn it off or keep a key of their
//! own. Those tries go through the sign-in page's throttle.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::sessions::{Sessions, hex};
use super::signin::{COOKIE, path_of, session_of};
use super::who::{Scope, Who, acting_as, current};
use super::{Serve, now};

/// How long a signed link works.
pub const LINK_SECS: i64 = 24 * 3600;

fn text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty())
}

/// What anyone may load: the sign-in page (and signing out), the web app's manifest, service worker and icons, the
/// health check, the proxy's check, the OpenSubsonic API, and Last.fm's and Spotify's ways back from a sign-in. Plain paths only: none that a server could read as
/// another one (`..`, encoded dots or slashes, doubled slashes). Also the guest portal (decision 0079), whose routes
/// answer only at home while it is on (`guest`), and the built page's fingerprinted code and styles (`/assets/`), the
/// open-source app's own, which the guest page loads without a session.
pub fn public(uri: &str) -> bool {
    let path = path_of(uri);
    let lower = path.to_ascii_lowercase();
    let plain = !path.contains("..") && !path.contains("//") && !path.contains('\\') && !["%2e", "%2f", "%5c"].iter().any(|e| lower.contains(e));
    plain
        && (matches!(path, "/signin" | "/signout" | "/manifest.webmanifest" | "/sw.js" | "/api/health" | "/api/auth/check")
            || path.starts_with("/icons/")
            // Last.fm's way back from a sign-in, found by its one-time code (`app::scrobble::pending`).
            || path.starts_with("/api/scrobble/lastfm/callback/")
            // Spotify's way back (decision 0073), found by its one-time `state` (`app::spotify::web::take_pending`).
            || path == "/api/spotify/callback"
            || path.starts_with("/rest/")
            || path == "/guest"
            || path.starts_with("/api/guest/")
            || path.starts_with("/assets/"))
}

/// A browser loading a page (not a script's request, nor a picture's): it can be sent to the sign-in page.
pub fn page_load(method: &str, headers: &HeaderMap) -> bool {
    if !matches!(method, "GET" | "HEAD") {
        return false;
    }
    match text(headers, "sec-fetch-mode") {
        Some(mode) => mode == "navigate",
        None => text(headers, "accept").is_some_and(|a| a.contains("text/html")),
    }
}

/// The answer to a request that may not pass: a page load goes to the sign-in page (and back to `uri` after),
/// anything else gets 401 with Cratarr's `Cookie` challenge (which no browser answers with a login box of its own).
pub fn refuse(method: &str, headers: &HeaderMap, uri: &str) -> Response {
    if page_load(method, headers) {
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

/// Why a request may not pass, for the diagnostics log (decision 0045): which way in it tried, never what it carried.
fn refused_why(headers: &HeaderMap, query: Option<&str>) -> &'static str {
    if key_of(headers).is_some() {
        "key not valid"
    } else if super::signin::token_of(headers).is_some() {
        "session not valid"
    } else if query.is_some_and(|q| q.contains("sig=")) {
        "link not valid"
    } else {
        "not signed in"
    }
}

/// A refusal in the diagnostics log: who asked for what (the path only: a query can carry a link's signature), why, and
/// whether Cratarr refused it itself or answered a proxy's check (`by`). A page load sent to the sign-in page is not
/// trouble; anything else refused is.
pub(super) fn note_refused(r: &Response, headers: &HeaderMap, peer: Option<std::net::SocketAddr>, path: &str, query: Option<&str>, by: &str) {
    let client = super::signin::client(headers, peer);
    let (status, reason) = (r.status().as_u16(), refused_why(headers, query));
    if r.status() == StatusCode::SEE_OTHER {
        tracing::info!(target: "signin", client, path, status, reason, by, "sent to sign in");
    } else {
        tracing::warn!(target: "signin", client, path, status, reason, by, "request refused");
    }
}

/// The API key a request carries (`Authorization: Bearer …`).
pub fn key_of(headers: &HeaderMap) -> Option<&str> {
    let (scheme, key) = text(headers, "authorization")?.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| key.trim())
}

/// HMAC-SHA256 (RFC 2104).
fn hmac(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let (mut inner, mut outer) = (Sha256::new(), Sha256::new());
    inner.update(k.map(|b| b ^ 0x36));
    inner.update(message);
    outer.update(k.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

fn signature(secret: &[u8], path: &str, expires: i64) -> String {
    hex(&hmac(secret, format!("{path}\n{expires}").as_bytes()))
}

/// `url` signed for its path until `expires` (`exp` and `sig` added to its query).
fn sign_with(secret: &[u8], url: &str, expires: i64) -> String {
    let Ok(mut u) = reqwest::Url::parse(url) else { return url.to_string() };
    let sig = signature(secret, u.path(), expires);
    u.query_pairs_mut().append_pair("exp", &expires.to_string()).append_pair("sig", &sig);
    u.to_string()
}

/// `url` signed for its path for [LINK_SECS], for a player that cannot send a header (decision 0043): as it is while
/// Cratarr's server is not open (tests).
pub fn signed(url: &str) -> String {
    let Some(serve) = super::instance() else { return url.to_string() };
    match serve.sessions.link_secret() {
        Ok(secret) => sign_with(&secret, url, now() + LINK_SECS),
        Err(e) => {
            tracing::warn!("access: no secret to sign links with: {e:#}");
            url.to_string()
        }
    }
}

/// Whether a request's query signs its path, and has not expired.
fn link_ok(secret: &[u8], path: &str, query: Option<&str>, now: i64) -> bool {
    let pairs: Vec<(String, String)> = query.and_then(|q| serde_urlencoded::from_str(q).ok()).unwrap_or_default();
    let get = |n: &str| pairs.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());
    let (Some(Ok(expires)), Some(sig)) = (get("exp").map(str::parse::<i64>), get("sig")) else { return false };
    let want = signature(secret, path, expires);
    expires >= now && sig.len() == want.len() && sig.bytes().zip(want.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

/// Whether a request may pass: open, or with a session, an API key or a signed link.
pub fn admitted(serve: &Serve, method: &str, headers: &HeaderMap, path: &str, query: Option<&str>) -> bool {
    if public(path) || session_of(serve, headers).is_some() {
        return true;
    }
    if let Some(key) = key_of(headers) {
        return serve.sessions.check_key(key, now()).is_some();
    }
    matches!(method, "GET" | "HEAD") && query.is_some_and(|q| q.contains("sig=")) && serve.sessions.link_secret().is_ok_and(|s| link_ok(&s, path, query, now()))
}

/// Who a request admitted is for (decision 0051): its session's account, its key's (the owner for a key from before
/// keys had one), within the key's scope; a signed link's, none (it reads what it names). `None`: not admitted.
fn who_of(serve: &Serve, method: &str, headers: &HeaderMap, path: &str, query: Option<&str>, home: bool) -> Option<Option<Who>> {
    let account = |name: &str, scope: Scope| serve.users.get(name).filter(|u| u.active).map(|u| Who { user: u.name, admin: u.admin, scope, home });
    if let Some(session) = session_of(serve, headers) {
        return Some(account(&session.user, Scope::Full)).filter(Option::is_some);
    }
    if let Some(key) = key_of(headers) {
        let key = serve.sessions.check_key(key, now())?;
        let owner = key.owner.clone().or_else(|| serve.users.owner())?;
        return Some(account(&owner, if key.scope == "read" { Scope::Read } else { Scope::Control })).filter(Option::is_some);
    }
    if public(path) || admitted(serve, method, headers, path, query) {
        return Some(None);
    }
    None
}

/// The requirement, around every route (`main`): a request without a way in is refused; one let in runs as whoever it
/// is for (`who`), from the home network or not (the client after the proxies' screen, `api::request_ids`).
pub async fn require(State(serve): State<Arc<Serve>>, request: Request, next: Next) -> Response {
    let (method, uri) = (request.method().as_str().to_string(), request.uri().clone());
    let peer = request.extensions().get::<axum::extract::ConnectInfo<std::net::SocketAddr>>().map(|c| c.0);
    let home = super::proxy::at_home(&super::signin::client(request.headers(), peer));
    if !serve.sign_in_required() {
        // Off, a key still counts as used: before the switch, Settings shows that a client sends its key.
        if let Some(key) = key_of(request.headers()) {
            let _ = serve.sessions.check_key(key, now());
        }
        // Not required, still heard (decision 0051): one signed in, or with a key, is that account; anyone else is
        // the owner, as an administrator (one user, as before accounts).
        let who = match who_of(&serve, &method, request.headers(), uri.path(), uri.query(), home) {
            Some(Some(who)) => Some(who),
            _ => serve.users.owner().map(|user| Who { user, admin: true, scope: Scope::Full, home }),
        };
        return match who {
            Some(who) if who.scope == Scope::Read && !matches!(method.as_str(), "GET" | "HEAD") => {
                (StatusCode::FORBIDDEN, axum::Json(json!({ "error": "This key only reads." }))).into_response()
            }
            Some(who) => acting_as(who, next.run(request)).await,
            None => next.run(request).await,
        };
    }
    match who_of(&serve, &method, request.headers(), uri.path(), uri.query(), home) {
        // A key that only reads changes nothing.
        Some(Some(who)) if who.scope == Scope::Read && !matches!(method.as_str(), "GET" | "HEAD") => {
            return (StatusCode::FORBIDDEN, axum::Json(json!({ "error": "This key only reads." }))).into_response();
        }
        Some(Some(who)) => return acting_as(who, next.run(request)).await,
        Some(None) => return next.run(request).await,
        None => {}
    }
    let shown = uri.path_and_query().map_or("/", |p| p.as_str()).to_string();
    let r = refuse(&method, request.headers(), &shown);
    note_refused(&r, request.headers(), peer, uri.path(), uri.query(), "cratarr");
    r
}

/// Whether the running request is an administrator's (decision 0051): signed in as one, not through a key (a key acts
/// for one of them, but changes nothing of the server's). No one: only while signing in is off (one user, an
/// administrator); an open route or a signed link is no one's.
pub fn administering() -> bool {
    match current() {
        Some(w) => w.admin && w.scope == Scope::Full,
        None => !super::instance().is_some_and(|s| s.sign_in_required()),
    }
}

/// The administrators' routes.
pub async fn admin(request: Request, next: Next) -> Response {
    if administering() {
        return next.run(request).await;
    }
    (StatusCode::FORBIDDEN, axum::Json(json!({ "error": "This is for administrators." }))).into_response()
}

// --- Settings, Authentication: the requirement and the API keys ---

pub fn router(serve: Arc<Serve>) -> Router {
    Router::new()
        .route("/api/auth/required", get(required).put(set_required))
        .route("/api/auth/keys", get(keys).post(new_key))
        .route("/api/auth/keys/{id}", delete(revoke_key))
        .route("/api/connections/home-assistant", get(home_assistant).put(home_assistant_on))
        .layer(axum::middleware::from_fn(admin))
        .with_state(serve)
}

/// The account's password again (the account signed in here, else the owner): the sign-in's own check and throttle.
async fn reauthenticate(serve: &Serve, headers: &HeaderMap, password: &str) -> Result<String, Response> {
    let Some(name) = session_of(serve, headers).map(|s| s.user).or_else(|| serve.users.owner()) else {
        return Err((StatusCode::CONFLICT, axum::Json(json!({ "error": "Cratarr has no account yet." }))).into_response());
    };
    let keys = [format!("client {}", super::signin::client(headers, None)), format!("account {name}")];
    if let Some(wait) = serve.throttle.wait(&keys, now()) {
        let mut r = (StatusCode::TOO_MANY_REQUESTS, axum::Json(json!({ "error": format!("Too many tries. Try again in {} min.", (wait + 59) / 60) }))).into_response();
        r.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(wait));
        return Err(r);
    }
    match serve.sign_in(&name, password).await {
        Some(user) => {
            serve.throttle.succeeded(&keys);
            Ok(user)
        }
        None => {
            serve.throttle.failed(&keys, now());
            tracing::warn!(target: "signin", user = %name, status = 403u16, "wrong password in Settings");
            Err((StatusCode::FORBIDDEN, axum::Json(json!({ "error": "Wrong password." }))).into_response())
        }
    }
}

async fn required(State(serve): State<Arc<Serve>>) -> Response {
    axum::Json(json!({ "required": serve.sessions.required().unwrap_or(false) })).into_response()
}

#[derive(serde::Deserialize)]
struct SetRequired {
    on: bool,
    password: String,
}

async fn set_required(State(serve): State<Arc<Serve>>, headers: HeaderMap, axum::Json(s): axum::Json<SetRequired>) -> Response {
    if let Err(r) = reauthenticate(&serve, &headers, &s.password).await {
        return r;
    }
    match serve.sessions.set_required(s.on) {
        Ok(()) => {
            tracing::info!(target: "signin", required = s.on, "sign-in requirement changed");
            axum::Json(json!({ "required": s.on })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

async fn keys(State(serve): State<Arc<Serve>>) -> Response {
    match serve.sessions.keys() {
        Ok(keys) => axum::Json(json!({ "keys": keys })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

/// Connections' Home Assistant (the owner, 2026-10-24: its key there, not in Authentication; no other client needs
/// one): its keys, a key made before keys had a purpose among them (their last use is when it last called), and the
/// weather it last told.
async fn home_assistant(State(serve): State<Arc<Serve>>) -> Response {
    match serve.sessions.keys() {
        Ok(keys) => {
            let keys: Vec<_> = keys.into_iter().filter(|k| k.purpose.as_deref().is_none_or(|p| p == HOME_ASSISTANT)).collect();
            let on = !serve.sessions.purpose_off(HOME_ASSISTANT);
            axum::Json(json!({ "keys": keys, "weather": crate::app::moment::last_told(), "on": on })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

#[derive(serde::Deserialize)]
struct SwitchedOn {
    on: bool,
}

/// Home Assistant switched on or off: off, its keys are kept but refused.
async fn home_assistant_on(State(serve): State<Arc<Serve>>, axum::Json(b): axum::Json<SwitchedOn>) -> Response {
    match serve.sessions.set_purpose_on(HOME_ASSISTANT, b.on) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

/// The purpose of Home Assistant's keys.
const HOME_ASSISTANT: &str = "home_assistant";

#[derive(serde::Deserialize)]
struct NewKey {
    name: String,
    password: String,
    /// The account it acts for (decision 0051); none: the one making it.
    #[serde(default)]
    owner: Option<String>,
    /// `read` or `control` (the default).
    #[serde(default)]
    scope: Option<String>,
    /// What uses it: `home_assistant`; none for any other.
    #[serde(default)]
    purpose: Option<String>,
}

/// A new API key: shown in this answer once, never again.
async fn new_key(State(serve): State<Arc<Serve>>, headers: HeaderMap, axum::Json(k): axum::Json<NewKey>) -> Response {
    let name = k.name.trim();
    if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
        return (StatusCode::BAD_REQUEST, axum::Json(json!({ "error": "Name the key." }))).into_response();
    }
    if let Err(r) = reauthenticate(&serve, &headers, &k.password).await {
        return r;
    }
    let owner = k.owner.filter(|o| !o.trim().is_empty()).or_else(|| current().map(|w| w.user));
    if let Some(o) = &owner {
        if serve.users.get(o).is_none_or(|u| !u.active) {
            return (StatusCode::BAD_REQUEST, axum::Json(json!({ "error": "No such user." }))).into_response();
        }
    }
    let purpose = k.purpose.as_deref().filter(|p| *p == HOME_ASSISTANT);
    match serve.sessions.new_key(name, owner.as_deref(), k.scope.as_deref().unwrap_or("control"), purpose, now()) {
        Ok((key, made)) => {
            tracing::info!(target: "signin", name = %made.name, "API key made");
            let mut r = axum::Json(json!({ "key": key, "id": made.id, "name": made.name })).into_response();
            r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            r
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

async fn revoke_key(State(serve): State<Arc<Serve>>, Path(id): Path<String>) -> Response {
    match serve.sessions.revoke_key(&id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, axum::Json(json!({ "error": "No such key." }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

/// `--new-api-key <name> --out <file>` (decision 0043): a key made on the server itself and written to an owner-only
/// file, never printed, for a client (Home Assistant) to be given without anyone seeing it.
pub fn mint(args: &[String]) -> anyhow::Result<()> {
    use anyhow::Context;
    let name = args.first().filter(|n| !n.starts_with("--")).context("usage: --new-api-key <name> --out <file>")?;
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).context("usage: --new-api-key <name> --out <file>")?;
    let data = std::env::var("CRATARR_DATA_DIR").context("CRATARR_DATA_DIR is not set")?;
    mint_to_file(std::path::Path::new(&data), name, std::path::Path::new(out))?;
    println!("made the key {name:?} in {out}");
    Ok(())
}

fn mint_to_file(data_dir: &std::path::Path, name: &str, out: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::io::Write;
    let sessions = Sessions::open(Some(&data_dir.join("sessions.sqlite")))?;
    let (key, _) = sessions.new_key(name, None, "control", (name == "Home Assistant").then_some(HOME_ASSISTANT), now())?;
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(out).with_context(|| format!("cannot write {}", out.display()))?;
    f.write_all(key.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::Rules;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use tower::ServiceExt;

    #[test]
    fn hmac_is_rfc_4231s() {
        // RFC 4231, test case 2.
        assert_eq!(hex(&hmac(b"Jefe", b"what do ya want for nothing?")), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
        // Test case 6: a key longer than the block.
        assert_eq!(
            hex(&hmac(&[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First")),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn a_signed_link_opens_its_own_path_until_it_expires() {
        let secret = b"secret".to_vec();
        let url = sign_with(&secret, "http://h:8081/api/file/cratarr/tr%201", 1_000);
        let u = reqwest::Url::parse(&url).unwrap();
        assert_eq!(u.path(), "/api/file/cratarr/tr%201");
        assert!(link_ok(&secret, u.path(), u.query(), 999));
        assert!(link_ok(&secret, u.path(), u.query(), 1_000));
        assert!(!link_ok(&secret, u.path(), u.query(), 1_001), "expired");
        assert!(!link_ok(&secret, "/api/file/cratarr/tr-2", u.query(), 999), "another track");
        assert!(!link_ok(b"other", u.path(), u.query(), 999), "another secret");
        let later = u.query().unwrap().replace("exp=1000", "exp=9999");
        assert!(!link_ok(&secret, u.path(), Some(&later), 999), "a longer life is not signed");
        assert!(!link_ok(&secret, u.path(), None, 999));
    }

    fn served(required: bool) -> Arc<Serve> {
        let s = Serve::open(None, Rules::default(), &|_| None).unwrap();
        s.users.set("me", "pw").unwrap();
        s.sessions.set_required(required).unwrap();
        s
    }

    /// The whole server's routes behind the requirement, as `main` has them.
    fn app(s: &Arc<Serve>) -> Router {
        super::super::routes::router(s.clone())
            .route("/api/library", get(|| async { "the library" }))
            .route("/", get(|| async { "the page" }))
            .layer(axum::middleware::from_fn_with_state(s.clone(), require))
    }

    async fn status(s: &Arc<Serve>, req: HttpRequest<Body>) -> (StatusCode, Option<String>) {
        let r = app(s).oneshot(req).await.unwrap();
        (r.status(), r.headers().get("location").map(|v| v.to_str().unwrap().to_string()))
    }

    fn get_(uri: &str) -> HttpRequest<Body> {
        HttpRequest::get(uri).body(Body::empty()).unwrap()
    }

    /// A JSON request, and its answer's status and body.
    async fn send_json(s: &Arc<Serve>, method: &str, uri: &str, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
        let req = HttpRequest::builder().method(method).uri(uri).header("content-type", "application/json").body(Body::from(body.to_string())).unwrap();
        let r = app(s).oneshot(req).await.unwrap();
        let status = r.status();
        let bytes = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }

    #[tokio::test]
    async fn with_signing_in_off_one_still_signed_in_is_told_who_they_are() {
        let s = served(false);
        s.users.from_directory("ann", false, Some(" Ann Example ")).unwrap();
        let (token, _) = s.sessions.create("ann", "d", now()).unwrap();
        let as_ann = |uri: &str| HttpRequest::get(uri).header("cookie", format!("{COOKIE}={token}")).body(Body::empty()).unwrap();
        let me = app(&s).oneshot(as_ann("/api/me")).await.unwrap();
        assert_eq!(me.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(me.into_body(), 1 << 16).await.unwrap()).unwrap();
        assert_eq!((body["name"].as_str(), body["admin"].as_bool()), (Some("ann"), Some(false)), "ann, no administrator");
        assert_eq!(body["display_name"].as_str(), Some("Ann Example"), "the name the directory gives her, trimmed");
        assert_eq!(status(&s, as_ann("/api/users")).await.0, StatusCode::FORBIDDEN, "as the server treats her");
        // No one signed in: the one user, as before.
        assert_eq!(status(&s, HttpRequest::get("/api/me").body(Body::empty()).unwrap()).await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn the_users_panel_makes_and_keeps_cratarr_accounts_and_says_why_not() {
        // Signing in off: the owner ("me") administers.
        let s = served(false);
        let (status, ann) = send_json(&s, "POST", "/api/users", json!({ "name": "ann", "password": "pw", "admin": false })).await;
        assert_eq!((status, ann["name"].as_str(), ann["directory"].as_bool(), ann["admin"].as_bool()), (StatusCode::OK, Some("ann"), Some(false), Some(false)));
        let (status, taken) = send_json(&s, "POST", "/api/users", json!({ "name": "Ann", "password": "pw" })).await;
        assert_eq!((status, taken["error"].as_str()), (StatusCode::CONFLICT, Some("That name is taken.")));
        assert_eq!(send_json(&s, "POST", "/api/users", json!({ "name": "a/b", "password": "pw" })).await.0, StatusCode::BAD_REQUEST);
        let (status, own) = send_json(&s, "PUT", "/api/users/me/role", json!({ "admin": false })).await;
        assert_eq!((status, own["error"].as_str()), (StatusCode::CONFLICT, Some("You cannot change your own role.")));
        assert_eq!(send_json(&s, "PUT", "/api/users/ann/password", json!({ "password": "new" })).await.0, StatusCode::NO_CONTENT);
        assert_eq!(send_json(&s, "PUT", "/api/users/ann/active", json!({ "active": false })).await.0, StatusCode::NO_CONTENT);
        assert!(s.users.sign_in("ann", "new").is_none(), "removed");
        assert_eq!(send_json(&s, "PUT", "/api/users/nobody/active", json!({ "active": true })).await.0, StatusCode::NOT_FOUND);
        // One's own password: the current one asked.
        let (status, wrong) = send_json(&s, "PUT", "/api/me/password", json!({ "current": "nope", "password": "x" })).await;
        assert_eq!((status, wrong["error"].as_str()), (StatusCode::FORBIDDEN, Some("The current password is wrong.")));
        assert_eq!(send_json(&s, "PUT", "/api/me/password", json!({ "current": "pw", "password": "x" })).await.0, StatusCode::NO_CONTENT);
        assert!(s.users.sign_in("me", "x").is_some());
        // A Cratarr account's app password is its password.
        assert_eq!(send_json(&s, "PUT", "/api/me/subsonic-password", json!({ "password": "y" })).await.0, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn a_refusal_is_in_the_diagnostics_log_with_why_and_without_what_it_carried() {
        use tracing_subscriber::layer::SubscriberExt;
        let s = served(true);
        let (layer, pending) = crate::diagnostics::layer();
        let log = crate::diagnostics::Diagnostics::start(None, pending).unwrap();
        let _logging = crate::diagnostics::log_here(tracing_subscriber::registry().with(layer));
        let linked = HttpRequest::get("/api/library?exp=1&sig=abc").header("x-forwarded-for", "84.1.2.3").body(Body::empty()).unwrap();
        assert_eq!(status(&s, linked).await.0, StatusCode::UNAUTHORIZED);
        let keyed = HttpRequest::get("/api/library").header("authorization", "Bearer cratarr_0000").body(Body::empty()).unwrap();
        assert_eq!(status(&s, keyed).await.0, StatusCode::UNAUTHORIZED);
        log.flush();
        let rows = log.query(&crate::diagnostics::Query::default()).unwrap();
        let seen: Vec<_> = rows.iter().map(|r| (r.what.as_str(), r.fields["path"].as_str().unwrap(), r.fields["reason"].as_str().unwrap(), r.fields["status"].as_u64().unwrap())).collect();
        assert_eq!(seen, [("request refused", "/api/library", "key not valid", 401), ("request refused", "/api/library", "link not valid", 401)]);
        assert_eq!((rows[0].fields["client"].as_str(), rows[1].fields["client"].as_str()), (Some("unknown"), Some("84.1.2.3")));
        assert!(rows.iter().all(|r| !r.fields.to_string().contains("abc") && !r.fields.to_string().contains("cratarr_0000")), "neither the signature nor the key");
    }

    #[tokio::test]
    async fn an_account_goes_as_far_as_it_may_and_a_key_no_further_than_its_scope() {
        let s = served(true);
        s.users.from_directory("ann", false, None).unwrap();
        let signed_in = |user: &str, uri: &str| {
            let (token, _) = s.sessions.create(user, "d", now()).unwrap();
            HttpRequest::get(uri).header("cookie", format!("{COOKIE}={token}")).body(Body::empty()).unwrap()
        };
        for admins in ["/api/auth/keys", "/api/subsonic/import/sources", "/api/subsonic/accounts", "/api/users"] {
            assert_eq!(status(&s, signed_in("me", admins)).await.0, StatusCode::OK, "{admins}");
            assert_eq!(status(&s, signed_in("ann", admins)).await.0, StatusCode::FORBIDDEN, "{admins}");
        }
        let me = app(&s).oneshot(signed_in("ann", "/api/me")).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(me.into_body(), 1 << 16).await.unwrap()).unwrap();
        assert_eq!((body["name"].as_str(), body["admin"].as_bool()), (Some("ann"), Some(false)));
        assert!(body["display_name"].is_null(), "none given: null");

        let keyed = |key: &str, method: &str, uri: &str| HttpRequest::builder().method(method).uri(uri).header("authorization", format!("Bearer {key}")).body(Body::empty()).unwrap();
        let (reads, _) = s.sessions.new_key("Dashboard", Some("me"), "read", None, now()).unwrap();
        assert_eq!(status(&s, keyed(&reads, "GET", "/api/library")).await.0, StatusCode::OK);
        assert_eq!(status(&s, keyed(&reads, "POST", "/api/library")).await.0, StatusCode::FORBIDDEN, "it only reads");
        let (controls, _) = s.sessions.new_key("Home Assistant", Some("me"), "control", Some(HOME_ASSISTANT), now()).unwrap();
        assert_eq!(status(&s, keyed(&controls, "GET", "/api/auth/keys")).await.0, StatusCode::FORBIDDEN, "an administrator's key is no administrator");
        // One who left the directory's group is no one.
        s.users.deactivate("ann").unwrap();
        assert_eq!(status(&s, signed_in("ann", "/api/library")).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_request_is_from_home_as_the_client_after_the_proxies_screen_is() {
        let s = served(true);
        let (token, _) = s.sessions.create("me", "d", now()).unwrap();
        let app = || {
            Router::new()
                .route("/api/home", get(|| async { format!("{:?}", current().map(|w| w.home)) }))
                .layer(axum::middleware::from_fn_with_state(s.clone(), require))
                .layer(axum::middleware::from_fn(crate::api::request_ids))
        };
        let home = |peer: &str, forwarded: Option<&str>| {
            let mut b = HttpRequest::get("/api/home").header("cookie", format!("{COOKIE}={token}"));
            if let Some(f) = forwarded {
                b = b.header("x-forwarded-for", f);
            }
            let mut req = b.body(Body::empty()).unwrap();
            req.extensions_mut().insert(axum::extract::ConnectInfo(std::net::SocketAddr::new(peer.parse().unwrap(), 50000)));
            let r = app().oneshot(req);
            async move { String::from_utf8(axum::body::to_bytes(r.await.unwrap().into_body(), 64).await.unwrap().to_vec()).unwrap() }
        };
        assert_eq!(home("192.168.1.20", None).await, "Some(true)");
        assert_eq!(home("203.0.113.9", None).await, "Some(false)");
        assert_eq!(home("203.0.113.9", Some("192.168.1.20")).await, "Some(false)", "a peer not believed names no one");
        assert_eq!(home("172.19.0.3", Some("203.0.113.9")).await, "Some(false)", "a local proxy names the client");
        assert_eq!(home("172.19.0.3", Some("192.168.1.20")).await, "Some(true)");
        assert_eq!(home("2001:db8:7::5", None).await, "Some(false)");
        super::super::proxy::set_home(vec!["2001:db8:7::/48".parse().unwrap()]);
        assert_eq!(home("2001:db8:7::5", None).await, "Some(true)", "a home network named");
        super::super::proxy::set_home(Vec::new());
    }

    #[tokio::test]
    async fn while_it_is_off_or_there_is_no_account_everything_passes() {
        let s = served(false);
        assert_eq!(status(&s, get_("/api/library")).await.0, StatusCode::OK);
        // A key sent while it is off still shows as used: the check before switching on.
        let (key, made) = s.sessions.new_key("Home Assistant", None, "control", Some(HOME_ASSISTANT), now()).unwrap();
        let keyed = HttpRequest::get("/api/library").header("authorization", format!("Bearer {key}")).body(Body::empty()).unwrap();
        assert_eq!(status(&s, keyed).await.0, StatusCode::OK);
        assert!(s.sessions.keys().unwrap().iter().find(|k| k.id == made.id).unwrap().last_used.is_some());
        // Not required, still heard: one signed in is themselves, not the owner (decision 0051).
        assert_eq!(status(&s, get_("/api/auth/keys")).await.0, StatusCode::OK, "anyone else: the owner");
        s.users.from_directory("ann", false, None).unwrap();
        let (token, _) = s.sessions.create("ann", "d", now()).unwrap();
        let ann = HttpRequest::get("/api/auth/keys").header("cookie", format!("{COOKIE}={token}")).body(Body::empty()).unwrap();
        assert_eq!(status(&s, ann).await.0, StatusCode::FORBIDDEN);
        let fresh = Serve::open(None, Rules::default(), &|_| None).unwrap();
        fresh.sessions.set_required(true).unwrap();
        assert_eq!(status(&fresh, get_("/api/library")).await.0, StatusCode::OK, "no account to sign in with yet");
    }

    #[tokio::test]
    async fn required_only_a_session_a_key_or_a_signed_link_pass() {
        let s = served(true);
        assert_eq!(status(&s, get_("/api/library")).await.0, StatusCode::UNAUTHORIZED);
        let page = HttpRequest::get("/?x=1").header("sec-fetch-mode", "navigate").body(Body::empty()).unwrap();
        assert_eq!(status(&s, page).await, (StatusCode::SEE_OTHER, Some("/signin?next=%2F%3Fx%3D1".into())));
        for open in ["/api/health", "/signin", "/icons/icon-192.png", "/rest/ping.view"] {
            assert_ne!(status(&s, get_(open)).await.0, StatusCode::UNAUTHORIZED, "{open}");
        }

        let (token, _) = s.sessions.create("me", "d", now()).unwrap();
        let with_session = HttpRequest::get("/api/library").header("cookie", format!("{COOKIE}={token}")).body(Body::empty()).unwrap();
        assert_eq!(status(&s, with_session).await.0, StatusCode::OK);

        let (key, made) = s.sessions.new_key("Home Assistant", None, "control", Some(HOME_ASSISTANT), now()).unwrap();
        let with_key = |k: &str| HttpRequest::get("/api/library").header("authorization", format!("Bearer {k}")).body(Body::empty()).unwrap();
        assert_eq!(status(&s, with_key(&key)).await.0, StatusCode::OK);
        assert_eq!(status(&s, with_key("cratarr_0000")).await.0, StatusCode::UNAUTHORIZED);
        s.sessions.revoke_key(&made.id).unwrap();
        assert_eq!(status(&s, with_key(&key)).await.0, StatusCode::UNAUTHORIZED, "revoked");

        let secret = s.sessions.link_secret().unwrap();
        let link = sign_with(&secret, "http://h/api/library", now() + 60);
        let u = reqwest::Url::parse(&link).unwrap();
        let q = format!("{}?{}", u.path(), u.query().unwrap());
        assert_eq!(status(&s, get_(&q)).await.0, StatusCode::OK);
        let expired = sign_with(&secret, "http://h/api/library", now() - 1);
        let e = reqwest::Url::parse(&expired).unwrap();
        assert_eq!(status(&s, get_(&format!("{}?{}", e.path(), e.query().unwrap()))).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(status(&s, get_(&q.replace("/api/library", "/"))).await.0, StatusCode::UNAUTHORIZED, "another path");
    }

    #[tokio::test]
    async fn the_switch_and_a_new_key_need_the_password_again() {
        let s = served(false);
        let put = |pw: &str| {
            HttpRequest::put("/api/auth/required")
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"on":true,"password":"{pw}"}}"#)))
                .unwrap()
        };
        assert_eq!(status(&s, put("wrong")).await.0, StatusCode::FORBIDDEN);
        assert_eq!(s.sessions.required(), Some(false));
        assert_eq!(status(&s, put("pw")).await.0, StatusCode::OK);
        assert_eq!(s.sessions.required(), Some(true));

        let (token, _) = s.sessions.create("me", "d", now()).unwrap();
        let mint = |pw: &str| {
            HttpRequest::post("/api/auth/keys")
                .header("content-type", "application/json")
                .header("cookie", format!("{COOKIE}={token}"))
                .body(Body::from(format!(r#"{{"name":"Home Assistant","password":"{pw}","purpose":"home_assistant"}}"#)))
                .unwrap()
        };
        assert_eq!(status(&s, mint("wrong")).await.0, StatusCode::FORBIDDEN);
        let r = app(&s).oneshot(mint("pw")).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(r.into_body(), 1 << 16).await.unwrap()).unwrap();
        let key = body["key"].as_str().unwrap();
        assert!(s.sessions.check_key(key, now()).is_some());
        let listed = app(&s).oneshot(HttpRequest::get("/api/auth/keys").header("cookie", format!("{COOKIE}={token}")).body(Body::empty()).unwrap()).await.unwrap();
        let text = String::from_utf8(axum::body::to_bytes(listed.into_body(), 1 << 16).await.unwrap().to_vec()).unwrap();
        assert!(text.contains("Home Assistant") && !text.contains(key), "listed without the key");
        let ha = app(&s)
            .oneshot(HttpRequest::get("/api/connections/home-assistant").header("cookie", format!("{COOKIE}={token}")).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let ha: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(ha.into_body(), 1 << 16).await.unwrap()).unwrap();
        assert_eq!(ha["keys"][0]["purpose"], "home_assistant", "shown with its connection");
    }

    #[test]
    fn a_key_minted_on_the_server_goes_to_an_owner_only_file() {
        let dir = std::env::temp_dir().join(format!("cratarr-mint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("keys").join("home-assistant");
        mint_to_file(&dir, "Home Assistant", &out).unwrap();
        let key = std::fs::read_to_string(&out).unwrap();
        assert!(key.starts_with(super::super::sessions::KEY_PREFIX));
        let s = Sessions::open(Some(&dir.join("sessions.sqlite"))).unwrap();
        assert_eq!(s.check_key(&key, now()).unwrap().name, "Home Assistant");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(std::fs::metadata(out.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
