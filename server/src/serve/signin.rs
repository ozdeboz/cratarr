//! Cratarr's own sign-in (decision 0039): the page a reverse proxy sends a browser to away from home (`auth_check`), and
//! the sessions it gives out (`sessions`). As the OWASP Authentication and Session Management cheat sheets advise: a
//! plain form that password managers fill in and that needs no script; the same answer for an unknown account as for a
//! wrong password; sign-ins throttled per client and per account (429 with Retry-After, which the proxy's log shows to a
//! ban list such as CrowdSec); the Origin checked; a new session at every sign-in; and a cookie the page's scripts
//! cannot read (`HttpOnly`), sent to this site alone (`SameSite=Lax`; over HTTPS the `__Host-` prefix: secure, no
//! Domain, path /) and dropped by the browser when the session's lifetime ends (`Max-Age`). The account is checked in
//! one place (`Serve::sign_in`), so a directory (LDAP) can be asked first later.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use serde_json::json;

use super::sessions::{LIFETIME, Session, random_hex};
use super::{Serve, now};

/// The session cookie over HTTPS (behind the proxy), and over plain HTTP (no `__Host-` without HTTPS).
pub const COOKIE: &str = "__Host-cratarr-session";
pub const COOKIE_HTTP: &str = "cratarr-session";
/// Failed sign-ins counted over this long, per client and per account; at [LIMIT] more wait.
const WINDOW: i64 = 15 * 60;
const LIMIT: usize = 10;

pub fn router(serve: Arc<Serve>) -> Router {
    Router::new()
        .route("/signin", get(page).post(sign_in))
        .route("/signout", post(sign_out))
        .route("/api/auth/session", get(this_session))
        .route("/api/auth/sessions", get(sessions))
        .route("/api/auth/sessions/{id}", delete(sign_out_device))
        .with_state(serve)
}

/// Failed sign-ins over the last [WINDOW], per client and per account (the last [LIMIT] of each).
#[derive(Default)]
pub struct Throttle {
    failed: Mutex<HashMap<String, Vec<i64>>>,
}

impl Throttle {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Vec<i64>>> {
        self.failed.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Seconds until `keys` may try again; none: now.
    pub(super) fn wait(&self, keys: &[String], now: i64) -> Option<i64> {
        let mut failed = self.lock();
        failed.retain(|_, times| {
            times.retain(|t| now - t < WINDOW);
            !times.is_empty()
        });
        keys.iter().filter_map(|k| failed.get(k)).filter(|t| t.len() >= LIMIT).map(|t| (t[t.len() - LIMIT] + WINDOW - now).max(1)).max()
    }

    pub(super) fn failed(&self, keys: &[String], now: i64) {
        let mut failed = self.lock();
        for k in keys {
            let times = failed.entry(k.clone()).or_default();
            times.push(now);
            if times.len() > LIMIT {
                times.remove(0);
            }
        }
    }

    pub(super) fn succeeded(&self, keys: &[String]) {
        let mut failed = self.lock();
        for k in keys {
            failed.remove(k);
        }
    }
}

/// The session token a request carries: the `__Host-` cookie first.
pub fn token_of(headers: &HeaderMap) -> Option<String> {
    let mut plain = None;
    for value in headers.get_all(header::COOKIE) {
        let Ok(value) = value.to_str() else { continue };
        for pair in value.split(';') {
            let Some((name, token)) = pair.trim().split_once('=') else { continue };
            let token = token.trim();
            if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
                continue;
            }
            if name == COOKIE {
                return Some(token.to_string());
            }
            if name == COOKIE_HTTP && plain.is_none() {
                plain = Some(token.to_string());
            }
        }
    }
    plain
}

/// The session a request is signed in with, while it lasts.
pub fn session_of(serve: &Serve, headers: &HeaderMap) -> Option<Session> {
    serve.sessions.check(&token_of(headers)?, now())
}

fn text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty())
}

/// Whether the browser reached Cratarr over HTTPS (the proxy says so).
fn over_https(headers: &HeaderMap) -> bool {
    text(headers, "x-forwarded-proto").is_some_and(|p| p.split(',').next().unwrap_or("").trim().eq_ignore_ascii_case("https"))
}

/// The client, as the nearest proxy saw it (the last `X-Forwarded-For`), else as Cratarr does.
pub(crate) fn client(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    text(headers, "x-forwarded-for")
        .and_then(|f| f.rsplit(',').next())
        .map(|ip| ip.trim().to_string())
        .filter(|ip| !ip.is_empty())
        .or_else(|| peer.map(|p| p.ip().to_string()))
        .unwrap_or_else(|| "unknown".into())
}

/// A form post from this site's own page: its Origin is the address the request came to. (Browsers send an Origin
/// with every form post; a request without one is no browser's, and a cookie set for it reaches no browser.)
fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = text(headers, "origin") else { return true };
    let Some(authority) = origin.split_once("://").map(|(_, a)| a.trim_end_matches('/')) else { return false };
    [text(headers, "host"), text(headers, "x-forwarded-host").map(|h| h.split(',').next().unwrap_or("").trim())]
        .into_iter()
        .flatten()
        .any(|h| h.eq_ignore_ascii_case(authority))
}

/// A URI's path, without its query or fragment.
pub fn path_of(uri: &str) -> &str {
    uri.split(['?', '#']).next().unwrap_or("")
}

/// Where to go after signing in: a path on this site, as the request named it (never another site, nor the sign-in
/// page itself); else Cratarr's start.
pub fn safe_next(next: Option<&str>) -> String {
    let n = next.unwrap_or("").trim();
    let ok = n.starts_with('/')
        && !n.starts_with("//")
        && n.bytes().all(|b| (0x21..0x7f).contains(&b) && b != b'\\')
        && !matches!(path_of(n), "/signin" | "/signout");
    if ok { n.to_string() } else { "/".into() }
}

/// The player's name the Cratarr app adds to its page's User-Agent (`CratarrApp/<version> (name=<encoded>)`).
fn app_player(ua: &str) -> Option<String> {
    let encoded = ua.split("CratarrApp/").nth(1)?.split("name=").nth(1)?.split([')', ' ', ';']).next()?;
    let pairs: Vec<(String, String)> = serde_urlencoded::from_str(&format!("n={encoded}")).ok()?;
    let name: String = pairs.into_iter().next()?.1.chars().filter(|c| !c.is_control()).take(40).collect();
    Some(name.trim().to_string()).filter(|n| !n.is_empty())
}

/// What signed in, from its browser's User-Agent ("Firefox on Windows"), and what its page could tell (`browser`:
/// Brave, which says it is Chrome); the Cratarr app by its player's name ("Cratarr app · Salon tablet").
fn device_of(headers: &HeaderMap, browser: Option<&str>) -> String {
    let ua = text(headers, "user-agent").unwrap_or("");
    if ua.contains("CratarrApp/") {
        return app_player(ua).map_or_else(|| "Cratarr app".to_string(), |n| format!("Cratarr app · {n}"));
    }
    let app = text(headers, "x-requested-with").is_some_and(|w| w.starts_with("app.cratarr")) || ua.contains("; wv)");
    let what = if app {
        "Cratarr app"
    } else if browser == Some("Brave") {
        "Brave"
    } else if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("Firefox/") {
        "Firefox"
    } else if ua.contains("SamsungBrowser/") {
        "Samsung Internet"
    } else if ua.contains("OPR/") {
        "Opera"
    } else if ua.contains("Chrome/") {
        "Chrome"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        "A browser"
    };
    let on = if ua.contains("Android") {
        "Android"
    } else if ua.contains("iPhone") || ua.contains("iPad") {
        "iOS"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Mac OS X") {
        "macOS"
    } else if ua.contains("CrOS") {
        "ChromeOS"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        return what.to_string();
    };
    format!("{what} on {on}")
}

fn set_cookie(token: &str, https: bool) -> HeaderValue {
    let v = if https {
        format!("{COOKIE}={token}; Path=/; Max-Age={LIFETIME}; Secure; HttpOnly; SameSite=Lax")
    } else {
        format!("{COOKIE_HTTP}={token}; Path=/; Max-Age={LIFETIME}; HttpOnly; SameSite=Lax")
    };
    HeaderValue::from_str(&v).expect("a cookie of hex and ASCII")
}

fn clear_cookie(https: bool) -> HeaderValue {
    HeaderValue::from_static(if https {
        "__Host-cratarr-session=; Path=/; Max-Age=0; Secure; HttpOnly; SameSite=Lax"
    } else {
        "cratarr-session=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax"
    })
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// The sign-in page: Cratarr's colours (light or dark as the device is), its icon, the form, and one line when the last
/// try failed. A small script adds the page's own place (`#/…`, which the browser keeps from the address it came from
/// but never sends) to where it goes back to.
fn page_html(next: &str, user: &str, error: Option<&str>, nonce: &str) -> String {
    let error = error.map(|e| format!(r#"<p class="error" role="alert">{}</p>"#, escape(e))).unwrap_or_default();
    let (user_focus, password_focus) = if user.is_empty() { (" autofocus", "") } else { ("", " autofocus") };
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="dark light">
<meta name="robots" content="noindex">
<title>Sign in · Cratarr</title>
<link rel="icon" href="/icons/favicon-32.png">
<style nonce="{nonce}">
  :root {{ color-scheme: dark; --bg: #0f1115; --surface: #181b22; --border: #2a2f3a; --text: #e8eaf0; --muted: #9097a5;
    --accent: #12a36a; --accent-text: #06281a; --danger: #c03e34;
    --danger-ink: color-mix(in srgb, var(--danger) 75%, var(--text)); }}
  @media (prefers-color-scheme: light) {{
    :root {{ color-scheme: light; --bg: #f5f6f8; --surface: #ffffff; --border: #d9dde5; --text: #1a1d24; --muted: #5d6573;
      --danger-ink: color-mix(in srgb, var(--danger) 75%, var(--text)); }}
  }}
  * {{ box-sizing: border-box; }}
  body {{ margin: 0; min-height: 100vh; display: grid; place-items: center; background: var(--bg); color: var(--text);
    font: 16px/1.4 system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif; }}
  main {{ width: min(360px, 100vw - 32px); padding: 24px 0; }}
  header {{ display: flex; align-items: center; gap: 12px; margin-bottom: 24px; }}
  header img {{ width: 44px; height: 44px; border-radius: 10px; }}
  h1 {{ margin: 0; font-size: 1.6rem; font-weight: 700; }}
  form {{ display: grid; gap: 16px; background: var(--surface); border: 1px solid var(--border); border-radius: 16px;
    padding: 20px; }}
  label {{ display: grid; gap: 6px; font-weight: 600; }}
  input {{ font: inherit; color: inherit; background: var(--bg); border: 1px solid var(--border); border-radius: 10px;
    padding: 12px 14px; width: 100%; }}
  input:focus-visible {{ outline: 2px solid var(--accent); outline-offset: 1px; }}
  button {{ font: inherit; font-weight: 700; color: var(--accent-text); background: var(--accent); border: 0;
    border-radius: 999px; padding: 12px; cursor: pointer; margin-top: 4px; }}
  button:focus-visible {{ outline: 2px solid var(--text); outline-offset: 2px; }}
  .error {{ margin: 0; color: var(--danger-ink); }}
</style>
</head>
<body>
<main>
<header><img src="/icons/icon-192.png" alt=""><h1>Cratarr</h1></header>
<form method="post" action="/signin">
{error}<input type="hidden" name="next" value="{next}"><input type="hidden" name="browser" value="">
<label>User name<input name="username" value="{user}" autocomplete="username" autocapitalize="none" spellcheck="false" required{user_focus}></label>
<label>Password<input name="password" type="password" autocomplete="current-password" required{password_focus}></label>
<button>Sign in</button>
</form>
</main>
<script nonce="{nonce}">
  const next = document.querySelector('input[name=next]');
  if (location.hash && !next.value.includes('#')) next.value += location.hash;
  if (navigator.brave) document.querySelector('input[name=browser]').value = 'Brave';
</script>
</body>
</html>
"##,
        next = escape(next),
        user = escape(user),
    )
}

/// The page, answered with the headers a sign-in page wants: nothing kept, not framed, scripts and styles of its own only.
fn page_response(status: StatusCode, next: &str, user: &str, error: Option<&str>, retry_after: Option<i64>) -> Response {
    let nonce = random_hex(16);
    let csp = format!(
        "default-src 'none'; img-src 'self'; style-src 'nonce-{nonce}'; script-src 'nonce-{nonce}'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
    );
    let mut r = (status, page_html(next, user, error, &nonce)).into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_str(&csp).expect("ASCII"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("same-origin"));
    if let Some(s) = retry_after {
        h.insert(header::RETRY_AFTER, HeaderValue::from(s));
    }
    r
}

fn go(to: &str) -> Response {
    let mut r = StatusCode::SEE_OTHER.into_response();
    r.headers_mut().insert(header::LOCATION, HeaderValue::from_str(to).unwrap_or(HeaderValue::from_static("/")));
    r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

#[derive(serde::Deserialize)]
struct NextQuery {
    next: Option<String>,
}

const NO_ACCOUNT: &str = "Cratarr has no account yet.";

async fn page(State(serve): State<Arc<Serve>>, headers: HeaderMap, axum::extract::Query(q): axum::extract::Query<NextQuery>) -> Response {
    let next = safe_next(q.next.as_deref());
    // Signed in already: on to where the page was going.
    if session_of(&serve, &headers).is_some() {
        return go(&next);
    }
    let none = serve.users.names().is_empty();
    page_response(StatusCode::OK, &next, "", none.then_some(NO_ACCOUNT), None)
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct SignIn {
    username: String,
    password: String,
    next: String,
    /// What the page could tell of its browser (`Brave`, which says it is Chrome).
    browser: String,
}

async fn sign_in(State(serve): State<Arc<Serve>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    if !same_origin(&headers) {
        return (StatusCode::FORBIDDEN, "A sign-in from another site is refused.").into_response();
    }
    let Ok(body) = axum::body::to_bytes(body, 16 * 1024).await else { return StatusCode::PAYLOAD_TOO_LARGE.into_response() };
    let form: SignIn = serde_urlencoded::from_bytes(&body).unwrap_or_default();
    let next = safe_next(Some(&form.next));
    let name = form.username.trim();
    let now = now();
    let client = client(&headers, parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0));
    // The account in any case: the directory matches names regardless of it.
    let keys = [format!("client {client}"), format!("account {}", name.to_lowercase())];
    if let Some(wait) = serve.throttle.wait(&keys, now) {
        tracing::warn!(target: "signin", client = %client, user = name, wait, status = 429u16, "sign-in throttled");
        let error = format!("Too many tries. Try again in {} min.", (wait + 59) / 60);
        return page_response(StatusCode::TOO_MANY_REQUESTS, &next, name, Some(&error), Some(wait));
    }
    let Some(user) = serve.sign_in(name, &form.password).await else {
        serve.throttle.failed(&keys, now);
        tracing::warn!(target: "signin", client = %client, user = name, status = 401u16, "sign-in refused");
        let error = if serve.users.names().is_empty() { NO_ACCOUNT } else { "Wrong user name or password." };
        return page_response(StatusCode::UNAUTHORIZED, &next, name, Some(error), None);
    };
    serve.throttle.succeeded(&keys);
    // A new session at every sign-in: one the browser held already ends.
    if let Some(old) = token_of(&headers) {
        let _ = serve.sessions.end(&old);
    }
    let device = device_of(&headers, Some(form.browser.as_str()).filter(|b| *b == "Brave"));
    match serve.sessions.create(&user, &device, now) {
        Ok((token, _)) => {
            tracing::info!(target: "signin", client = %client, user = %user, device = %device, "signed in");
            let mut r = go(&next);
            r.headers_mut().append(header::SET_COOKIE, set_cookie(&token, over_https(&headers)));
            r
        }
        Err(e) => {
            tracing::warn!("sign-in: the session could not be kept: {e:#}");
            page_response(StatusCode::INTERNAL_SERVER_ERROR, &next, name, Some("The sign-in could not be kept."), None)
        }
    }
}

async fn sign_out(State(serve): State<Arc<Serve>>, headers: HeaderMap) -> Response {
    if !same_origin(&headers) {
        return (StatusCode::FORBIDDEN, "A sign-out from another site is refused.").into_response();
    }
    if let Some(token) = token_of(&headers)
        && let Err(e) = serve.sessions.end(&token)
    {
        tracing::warn!("sign-out: the session could not be ended: {e:#}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let mut r = go("/signin");
    r.headers_mut().append(header::SET_COOKIE, clear_cookie(over_https(&headers)));
    r
}

/// `GET /api/auth/session`: the account this request is signed in as, and its session (both null at home, where
/// nothing asks).
async fn this_session(State(serve): State<Arc<Serve>>, headers: HeaderMap) -> Response {
    let s = session_of(&serve, &headers);
    axum::Json(json!({ "user": s.as_ref().map(|s| &s.user), "id": s.as_ref().map(|s| &s.id) })).into_response()
}

/// `GET /api/auth/sessions`: the signed-in devices (Settings → Authentication), this one marked.
async fn sessions(State(serve): State<Arc<Serve>>, headers: HeaderMap) -> Response {
    let current = session_of(&serve, &headers).map(|s| s.id);
    // An administrator sees every account's sign-ins; a user, their own (decision 0051). An administrator signed in,
    // not one of their keys or a Subsonic login (access.rs: a key acts for them but changes nothing of the server's; the
    // audit, 2026-10-29: a Read or Control key of an administrator listed every account's sign-ins).
    let mine = |user: &str| super::access::administering() || serve.current_user().as_deref() == Some(user);
    match serve.sessions.list(now()) {
        Ok(list) => {
            let list: Vec<_> = list
                .into_iter()
                .filter(|s| mine(&s.user))
                .map(|s| {
                    json!({ "id": s.id, "user": s.user, "device": s.device, "created": s.created, "last_used": s.last_used,
                        "current": current.as_deref() == Some(s.id.as_str()) })
                })
                .collect();
            axum::Json(json!({ "sessions": list })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

/// `DELETE /api/auth/sessions/{id}`: signs that device out.
async fn sign_out_device(State(serve): State<Arc<Serve>>, Path(id): Path<String>) -> Response {
    // An administrator signed in ends anyone's sign-in; a user, or an administrator's key, their own (decision 0051).
    if !super::access::administering() {
        let theirs = serve.sessions.list(now()).ok().and_then(|l| l.into_iter().find(|s| s.id == id)).map(|s| s.user);
        if theirs.is_some() && theirs != serve.current_user() {
            return (StatusCode::NOT_FOUND, axum::Json(json!({ "error": "No such session." }))).into_response();
        }
    }
    match serve.sessions.revoke(&id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, axum::Json(json!({ "error": "No such session." }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::Rules;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use tower::ServiceExt;

    const FIREFOX: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:131.0) Gecko/20100101 Firefox/131.0";

    fn served() -> Arc<Serve> {
        let s = Serve::open(None, Rules::default(), &|_| None).unwrap();
        s.users.set("me", "pw").unwrap();
        s
    }

    async fn send(s: &Arc<Serve>, req: HttpRequest<Body>) -> Response {
        super::super::routes::router(s.clone()).oneshot(req).await.unwrap()
    }

    async fn body(r: Response) -> String {
        String::from_utf8(axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap()
    }

    fn header<'a>(r: &'a Response, name: &str) -> &'a str {
        r.headers().get(name).map_or("", |v| v.to_str().unwrap())
    }

    /// A sign-in form posted through the proxy over HTTPS, from `client`.
    fn sign_in_form(user: &str, password: &str, client: &str) -> HttpRequest<Body> {
        let form = serde_urlencoded::to_string([("username", user), ("password", password), ("next", "/#/crates")]).unwrap();
        HttpRequest::post("/signin")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("host", "cratarr.example")
            .header("origin", "https://cratarr.example")
            .header("x-forwarded-proto", "https")
            .header("x-forwarded-for", client)
            .header("user-agent", FIREFOX)
            .body(Body::from(form))
            .unwrap()
    }

    /// The proxy asking whether a request may pass.
    fn check(uri: &str, cookie: Option<&str>, mode: &str) -> HttpRequest<Body> {
        let mut b = HttpRequest::get("/api/auth/check").header("x-forwarded-method", "GET").header("x-forwarded-uri", uri).header("sec-fetch-mode", mode);
        if let Some(c) = cookie {
            b = b.header("cookie", c);
        }
        b.body(Body::empty()).unwrap()
    }

    /// The `name=token` part of a response's Set-Cookie.
    fn cookie_of(r: &Response) -> String {
        header(r, "set-cookie").split(';').next().unwrap().to_string()
    }

    #[tokio::test]
    async fn a_sign_in_gives_a_session_the_proxy_lets_through_until_sign_out() {
        let s = served();
        let r = send(&s, sign_in_form("me", "pw", "203.0.113.7")).await;
        assert_eq!(r.status(), StatusCode::SEE_OTHER);
        assert_eq!(header(&r, "location"), "/#/crates", "back where the page was");
        let set = header(&r, "set-cookie").to_string();
        for part in ["__Host-cratarr-session=", "Path=/", &format!("Max-Age={LIFETIME}"), "Secure", "HttpOnly", "SameSite=Lax"] {
            assert!(set.contains(part), "{set} lacks {part}");
        }
        assert!(!set.contains("Domain"), "a __Host- cookie has none");
        let cookie = cookie_of(&r);

        let ok = send(&s, check("/api/library", Some(&cookie), "cors")).await;
        assert_eq!(ok.status(), StatusCode::NO_CONTENT);
        assert_eq!(header(&ok, "x-cratarr-user"), "me");
        let me = body(send(&s, HttpRequest::get("/api/auth/session").header("cookie", &cookie).body(Body::empty()).unwrap()).await).await;
        assert!(me.contains(r#""user":"me""#), "{me}");
        let list: serde_json::Value =
            serde_json::from_str(&body(send(&s, HttpRequest::get("/api/auth/sessions").header("cookie", &cookie).body(Body::empty()).unwrap()).await).await).unwrap();
        assert_eq!(list["sessions"][0]["device"], "Firefox on Windows");
        assert_eq!(list["sessions"][0]["current"], true);

        let out = send(
            &s,
            HttpRequest::post("/signout")
                .header("cookie", &cookie)
                .header("host", "cratarr.example")
                .header("origin", "https://cratarr.example")
                .header("x-forwarded-proto", "https")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!((out.status(), header(&out, "location")), (StatusCode::SEE_OTHER, "/signin"));
        assert!(header(&out, "set-cookie").contains("Max-Age=0"));
        assert_eq!(send(&s, check("/api/library", Some(&cookie), "cors")).await.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn without_a_session_a_page_goes_to_the_sign_in_page_and_the_rest_is_refused() {
        let s = served();
        let page = send(&s, check("/library?x=1", None, "navigate")).await;
        assert_eq!(page.status(), StatusCode::SEE_OTHER);
        assert_eq!(header(&page, "location"), "/signin?next=%2Flibrary%3Fx%3D1");
        let api = send(&s, check("/api/library", None, "cors")).await;
        assert_eq!(api.status(), StatusCode::UNAUTHORIZED);
        assert!(header(&api, "www-authenticate").starts_with("Cookie realm=\"Cratarr\""), "no Basic: no browser login box");
        assert_eq!(send(&s, check("/signin?next=%2F", None, "navigate")).await.status(), StatusCode::NO_CONTENT);
        assert_eq!(send(&s, check("/rest/ping.view", None, "no-cors")).await.status(), StatusCode::NO_CONTENT);
        // The old login is no way in any more.
        let basic = HttpRequest::get("/api/auth/check")
            .header("x-forwarded-uri", "/api/library")
            .header("authorization", "Basic bWU6cHc=")
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(&s, basic).await.status(), StatusCode::UNAUTHORIZED);
        let forged = send(&s, check("/api/library", Some(&format!("{COOKIE}={}", "a".repeat(64))), "cors")).await;
        assert_eq!(forged.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_wrong_password_reads_as_an_unknown_account_and_tries_are_throttled() {
        let s = served();
        let wrong = send(&s, sign_in_form("me", "nope", "203.0.113.7")).await;
        let unknown = send(&s, sign_in_form("nobody", "pw", "203.0.113.8")).await;
        assert_eq!((wrong.status(), unknown.status()), (StatusCode::UNAUTHORIZED, StatusCode::UNAUTHORIZED));
        assert!(wrong.headers().get("set-cookie").is_none());
        let (wrong, unknown) = (body(wrong).await, body(unknown).await);
        assert!(wrong.contains("Wrong user name or password.") && unknown.contains("Wrong user name or password."));
        assert!(wrong.contains(r#"value="me""#), "the user name stays filled in");

        // Ten failures for the account, from several clients: the account waits, even with its password, from anywhere.
        for i in 1..10 {
            send(&s, sign_in_form("me", "nope", &format!("198.51.100.{i}"))).await;
        }
        let held = send(&s, sign_in_form("me", "pw", "192.0.2.1")).await;
        assert_eq!(held.status(), StatusCode::TOO_MANY_REQUESTS);
        let wait: i64 = header(&held, "retry-after").parse().unwrap();
        assert!((1..=WINDOW).contains(&wait), "{wait}");
        assert!(body(held).await.contains("Too many tries."));
        // Ten failures from one client: it waits, whichever account it tries.
        s.users.set("you", "pw2").unwrap();
        for _ in 0..10 {
            send(&s, sign_in_form("someone", "x", "203.0.113.99")).await;
        }
        assert_eq!(send(&s, sign_in_form("you", "pw2", "203.0.113.99")).await.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(send(&s, sign_in_form("you", "pw2", "203.0.113.100")).await.status(), StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn a_sign_in_from_another_site_is_refused() {
        let s = served();
        let mut req = sign_in_form("me", "pw", "203.0.113.7");
        req.headers_mut().insert("origin", HeaderValue::from_static("https://evil.example"));
        let r = send(&s, req).await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
        assert!(r.headers().get("set-cookie").is_none());
    }

    #[tokio::test]
    async fn each_sign_in_is_a_new_session_and_a_new_password_ends_the_others() {
        let s = served();
        let first = cookie_of(&send(&s, sign_in_form("me", "pw", "203.0.113.7")).await);
        let mut again = sign_in_form("me", "pw", "203.0.113.7");
        again.headers_mut().insert("cookie", HeaderValue::from_str(&first).unwrap());
        let second = cookie_of(&send(&s, again).await);
        assert_ne!(first, second);
        assert_eq!(send(&s, check("/", Some(&first), "cors")).await.status(), StatusCode::UNAUTHORIZED, "the old one ended");
        let other = cookie_of(&send(&s, sign_in_form("me", "pw", "203.0.113.8")).await);

        let change = HttpRequest::post("/api/subsonic/password")
            .header("content-type", "application/json")
            .header("cookie", &second)
            .body(Body::from(r#"{"username":"me","current":"pw","password":"pw2"}"#))
            .unwrap();
        assert_eq!(send(&s, change).await.status(), StatusCode::OK);
        assert_eq!(send(&s, check("/", Some(&second), "cors")).await.status(), StatusCode::NO_CONTENT, "the device that changed it");
        assert_eq!(send(&s, check("/", Some(&other), "cors")).await.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn over_plain_http_the_cookie_cannot_be_a_host_cookie() {
        let s = served();
        let mut req = sign_in_form("me", "pw", "192.168.1.20");
        req.headers_mut().remove("x-forwarded-proto");
        req.headers_mut().insert("origin", HeaderValue::from_static("http://cratarr.example"));
        let r = send(&s, req).await;
        let set = header(&r, "set-cookie");
        assert!(set.starts_with("cratarr-session=") && !set.contains("Secure"), "{set}");
        assert_eq!(send(&s, check("/", Some(&cookie_of(&r)), "cors")).await.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn the_page_is_its_own_and_not_kept() {
        let s = served();
        let r = send(&s, HttpRequest::get("/signin?next=%2Fcrates").body(Body::empty()).unwrap()).await;
        assert_eq!(r.status(), StatusCode::OK);
        let csp = header(&r, "content-security-policy").to_string();
        assert!(csp.contains("default-src 'none'") && csp.contains("frame-ancestors 'none'") && csp.contains("form-action 'self'"), "{csp}");
        assert_eq!(header(&r, "cache-control"), "no-store");
        assert_eq!(header(&r, "x-frame-options"), "DENY");
        let html = body(r).await;
        assert!(html.contains(r#"name="next" value="/crates""#));
        assert!(html.contains(r#"autocomplete="current-password""#));
        let nonce = csp.split("'nonce-").nth(1).unwrap().split('\'').next().unwrap();
        assert!(html.contains(&format!(r#"<script nonce="{nonce}">"#)));
        let named = send(&s, sign_in_form("<b>me</b>", "x", "203.0.113.5")).await;
        assert!(body(named).await.contains("&lt;b&gt;me&lt;/b&gt;"), "what was typed is shown as text");
    }

    #[test]
    fn after_signing_in_only_a_path_on_this_site() {
        assert_eq!(safe_next(Some("/#/crates")), "/#/crates");
        assert_eq!(safe_next(Some("/library?x=1")), "/library?x=1");
        for bad in ["//evil.example", "https://evil.example", "/\\evil.example", "evil", "/signin?next=/", "/signout", "/a b", "/\u{e9}", ""] {
            assert_eq!(safe_next(Some(bad)), "/", "{bad:?}");
        }
        assert_eq!(safe_next(None), "/");
    }

    #[test]
    fn devices_are_named_from_their_browsers() {
        let h = |ua: &'static str, with: Option<&'static str>| {
            let mut m = HeaderMap::new();
            m.insert("user-agent", HeaderValue::from_static(ua));
            if let Some(w) = with {
                m.insert("x-requested-with", HeaderValue::from_static(w));
            }
            device_of(&m, None)
        };
        assert_eq!(h(FIREFOX, None), "Firefox on Windows");
        let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";
        assert_eq!(h(edge, None), "Edge on Windows");
        let wv = "Mozilla/5.0 (Linux; Android 14; K; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/130.0.0.0 Mobile Safari/537.36";
        assert_eq!(h(wv, None), "Cratarr app on Android");
        let chrome = "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Mobile Safari/537.36";
        assert_eq!(h(chrome, Some("app.cratarr.debug")), "Cratarr app on Android");
        assert_eq!(h(chrome, None), "Chrome on Android");
        let iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
        assert_eq!(h(iphone, None), "Safari on iOS");
        let app = "Mozilla/5.0 (Linux; Android 14; K; wv) AppleWebKit/537.36 Chrome/130.0.0.0 Mobile Safari/537.36 CratarrApp/0.1.0 (name=Salon%20tablet)";
        assert_eq!(h(app, None), "Cratarr app · Salon tablet");
        assert_eq!(h("x CratarrApp/0.1.0 (name=)", None), "Cratarr app");
        let mut m = HeaderMap::new();
        m.insert("user-agent", HeaderValue::from_static("Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 Chrome/130.0.0.0 Mobile Safari/537.36"));
        assert_eq!(device_of(&m, Some("Brave")), "Brave on Android", "Brave says it is Chrome; its page tells");
    }
}
