//! The guest portal (decision 0079; the owner, 2026-10-30): `/guest`, on the home network only and without an account.
//! A guest gives a name, sees what the host plays, searches the library and asks for a track; the request goes to the
//! host chosen in Settings (a user, or a shared device whose owner answers) as any queue request does, never straight
//! in. Off by default (`app::runtime`'s `guest`). Away from home, or while it is off, every one of its paths answers
//! 404, as if there were none.
//!
//! A guest is a cookie (`cratarr-guest`: a random token, 12 hours, kept in memory) given by `POST /api/guest/hello`;
//! everything else asks for it. Searches and requests are limited per guest and per address.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use super::Serve;
use crate::app::queue_requests::{Asker, State as Asked};
use crate::app::runtime::{Guest, GuestHost};
use crate::app::{PlayOn, QueueTrack};

/// The guest's cookie.
pub const COOKIE: &str = "cratarr-guest";
/// How long a guest stays one (s).
const GUEST_SECS: i64 = 12 * 3600;
const NAME_MOST: usize = 40;
/// Albums and tracks a search finds, at most, of each.
const FOUND_MOST: usize = 20;
/// The limits (the owner, 2026-10-30), per guest and per address: (window s, at most, why).
const SEARCHES: &[(i64, usize, &str)] = &[(60, 30, "Thirty searches a minute at most: wait a moment.")];
const REQUESTS: &[(i64, usize, &str)] = &[(30, 1, "One request every 30 seconds."), (3600, 10, "Ten requests an hour at most.")];
/// Names given from one address, at most (a new name is a new guest: the limits above go by address too).
const HELLOS: &[(i64, usize, &str)] = &[(3600, 20, "Too many names from here: wait a while.")];
/// Requests of one guest waiting at once, at most.
const WAITING_MOST: usize = 3;

/// What the portal knows.
#[derive(Clone)]
pub struct Portal {
    on: Arc<PlayOn>,
    serve: Arc<Serve>,
    guests: Arc<Guests>,
    /// The guest setting in force (`runtime`'s; a test's own).
    setting: Arc<dyn Fn() -> Guest + Send + Sync>,
}

/// The guests (token → name and until when) and what each guest and address asked lately.
#[derive(Default)]
struct Guests {
    tokens: Mutex<HashMap<String, (String, i64)>>,
    hits: Mutex<HashMap<String, VecDeque<i64>>>,
}

/// The client's address, as the gate read it.
#[derive(Clone)]
struct Client(String);

pub fn router(on: Arc<PlayOn>, serve: Arc<Serve>) -> Router {
    router_with(on, serve, Arc::new(|| crate::app::runtime::now().guest))
}

fn router_with(on: Arc<PlayOn>, serve: Arc<Serve>, setting: Arc<dyn Fn() -> Guest + Send + Sync>) -> Router {
    let p = Portal { on, serve, guests: Arc::default(), setting };
    Router::new()
        .route("/guest", get(page))
        .route("/api/guest/hello", post(hello))
        .route("/api/guest/now", get(now))
        .route("/api/guest/search", get(search))
        .route("/api/guest/albums/{id}", get(album))
        .route("/api/guest/queue-requests", get(own_requests).post(ask))
        // Only where a route matched: a method it does not have still answers 405.
        .route_layer(axum::middleware::from_fn_with_state(p.clone(), gate))
        .with_state(p)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

fn refused(status: StatusCode, error: &str) -> Response {
    (status, Json(json!({ "error": error }))).into_response()
}

/// Open while the guest page is on and the client is at home (after the proxies' screen, as `access::require` reads
/// it); else nothing is here.
async fn gate(State(p): State<Portal>, mut request: Request, next: Next) -> Response {
    let peer = request.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0);
    let client = super::signin::client(request.headers(), peer);
    if !(p.setting)().on || !super::proxy::at_home(&client) {
        return StatusCode::NOT_FOUND.into_response();
    }
    request.extensions_mut().insert(Client(client));
    next.run(request).await
}

/// A guest token: 16 random bytes, as hex.
fn new_token() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("the operating system's random source");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn token_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, token)| *name == COOKIE && token.len() == 32 && token.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|(_, token)| token.to_string())
}

/// A path's part as it goes in a URL.
fn seg(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// A path of Cratarr's signed for a day (`access::signed`), so it loads without a session.
fn signed(path: &str) -> String {
    const BASE: &str = "http://cratarr.invalid";
    let url = super::access::signed(&format!("{BASE}{path}"));
    url.strip_prefix(BASE).map(str::to_string).unwrap_or(url)
}

/// A track as a guest sees it: QueueTrack's fields and its cover, signed.
fn track_view(t: &QueueTrack) -> Value {
    let mut v = serde_json::to_value(t).unwrap_or_default();
    v["cover"] = signed(&format!("/api/cover/{}/{}", seg(&t.source), seg(&t.id))).into();
    v
}

/// A library song as a queue holds it.
fn own_track(s: super::api::Song) -> QueueTrack {
    let named = |s: String| Some(s).filter(|s| !s.is_empty());
    QueueTrack {
        source: crate::source::cratarr::KIND.id.to_string(),
        id: s.id,
        name: s.title,
        artist: named(s.artist),
        album: named(s.album),
        album_id: named(s.album_id),
        duration_secs: Some(s.duration as f64).filter(|d| *d > 0.0),
        ..QueueTrack::default()
    }
}

impl Guests {
    /// Whether each of `keys` may go once more under every rule; counted for all of them when they may, else why not.
    fn may(&self, keys: &[String], rules: &[(i64, usize, &str)], now: i64) -> Result<(), String> {
        let mut hits = self.hits.lock().unwrap_or_else(|p| p.into_inner());
        let longest = rules.iter().map(|r| r.0).max().unwrap_or(0);
        for k in keys {
            let q = hits.entry(k.clone()).or_default();
            while q.front().is_some_and(|t| now - t >= longest) {
                q.pop_front();
            }
            for (window, most, why) in rules {
                if q.iter().filter(|t| now - **t < *window).count() >= *most {
                    return Err(why.to_string());
                }
            }
        }
        for k in keys {
            hits.entry(k.clone()).or_default().push_back(now);
        }
        if hits.len() > 4096 {
            hits.retain(|_, q| q.back().is_some_and(|t| now - t < 3600));
        }
        Ok(())
    }
}

fn too_many(why: &str) -> Response {
    refused(StatusCode::TOO_MANY_REQUESTS, why)
}

impl Portal {
    /// The guest asking: their token and name; 401 without one.
    #[allow(clippy::result_large_err)]
    fn guest(&self, headers: &HeaderMap) -> Result<(String, String), Response> {
        let now = now_secs();
        let mut tokens = self.guests.tokens.lock().unwrap_or_else(|p| p.into_inner());
        tokens.retain(|_, (_, until)| *until > now);
        token_of(headers).and_then(|t| tokens.get(&t).map(|(n, _)| (t.clone(), n.clone()))).ok_or_else(|| refused(StatusCode::UNAUTHORIZED, "Say your name first."))
    }

    /// The host's player now, and the account that answers its requests: the player named (its owner answers), or the
    /// one the user named listens on. A player named whose music was handed over: the one it went to (the owner,
    /// 2026-10-30), while that is shown.
    fn host(&self) -> Option<(String, String)> {
        match (self.setting)().host? {
            GuestHost::Player(name) => {
                let name = self.on.moved_on(&name);
                self.on.view_of(&name)?;
                let owner = self.on.owner_of(&name)?;
                Some((name, owner))
            }
            GuestHost::User(user) => Some((self.on.playing_of(&user)?, user)),
        }
    }
}

async fn page() -> Response {
    crate::web::guest_page()
}

#[derive(Deserialize)]
struct Hello {
    name: String,
}

/// A guest by the name they give (trimmed, 1 to 40 characters): a cookie for 12 hours.
async fn hello(State(p): State<Portal>, Extension(Client(addr)): Extension<Client>, Json(b): Json<Hello>) -> Response {
    let name = b.name.trim().to_string();
    let n = name.chars().count();
    if n == 0 || n > NAME_MOST || name.chars().any(char::is_control) {
        return refused(StatusCode::BAD_REQUEST, "A name of 1 to 40 characters.");
    }
    let now = now_secs();
    if let Err(why) = p.guests.may(&[format!("hello@{addr}")], HELLOS, now) {
        return too_many(&why);
    }
    let token = new_token();
    {
        let mut tokens = p.guests.tokens.lock().unwrap_or_else(|p| p.into_inner());
        tokens.retain(|_, (_, until)| *until > now);
        tokens.insert(token.clone(), (name.clone(), now + GUEST_SECS));
    }
    tracing::info!(target: "server", guest = name.as_str(), client = addr.as_str(), "a guest");
    let mut r = Json(json!({ "name": name })).into_response();
    if let Ok(v) = HeaderValue::from_str(&format!("{COOKIE}={token}; Path=/; Max-Age={GUEST_SECS}; HttpOnly; SameSite=Lax")) {
        r.headers_mut().insert(header::SET_COOKIE, v);
    }
    r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

/// What the host plays: its track (with its cover, signed), playing or not, and where it is.
async fn now(State(p): State<Portal>, headers: HeaderMap) -> Response {
    if let Err(r) = p.guest(&headers) {
        return r;
    }
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let view = p.host().and_then(|(player, _)| p.on.view_of(&player)).filter(|v| v.track.is_some());
    match view {
        Some(v) => {
            let cover = v.track.as_ref().map(|t| track_view(t)["cover"].clone());
            Json(json!({ "track": v.track, "playing": v.playing, "position_ms": v.position_ms, "position_at": v.position_at, "cover": cover })).into_response()
        }
        None => Json(json!({ "track": null, "playing": false, "position_ms": 0, "position_at": at, "cover": null })).into_response(),
    }
}

#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    q: String,
}

/// An album as a guest sees it, with its cover, signed.
fn album_view(a: &super::api::Album) -> Value {
    json!({ "id": a.id, "name": a.name, "artist": a.artist, "year": a.year, "cover": signed(&format!("/api/albums/cratarr/{}/cover", seg(&a.id))) })
}

/// The library's albums and tracks for what was typed, at most twenty of each.
async fn search(State(p): State<Portal>, Extension(Client(addr)): Extension<Client>, headers: HeaderMap, Query(q): Query<Search>) -> Response {
    let (token, _) = match p.guest(&headers) {
        Ok(g) => g,
        Err(r) => return r,
    };
    if let Err(why) = p.guests.may(&[format!("search:{token}"), format!("search@{addr}")], SEARCHES, now_secs()) {
        return too_many(&why);
    }
    let text = q.q.trim().to_string();
    if text.is_empty() {
        return Json(json!({ "albums": [], "tracks": [] })).into_response();
    }
    let lib = p.serve.library.clone();
    let found = tokio::task::spawn_blocking(move || lib.search(&text, (0, 0), (FOUND_MOST, 0), (FOUND_MOST, 0), None)).await;
    let Ok(Ok((_, albums, songs))) = found else { return refused(StatusCode::INTERNAL_SERVER_ERROR, "The library cannot be searched now.") };
    let albums: Vec<Value> = albums.iter().map(album_view).collect();
    let tracks: Vec<Value> = songs.into_iter().map(|s| track_view(&own_track(s))).collect();
    Json(json!({ "albums": albums, "tracks": tracks })).into_response()
}

/// An album found, with its tracks, to ask for one of them; counted as a search.
async fn album(State(p): State<Portal>, Extension(Client(addr)): Extension<Client>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let (token, _) = match p.guest(&headers) {
        Ok(g) => g,
        Err(r) => return r,
    };
    if let Err(why) = p.guests.may(&[format!("search:{token}"), format!("search@{addr}")], SEARCHES, now_secs()) {
        return too_many(&why);
    }
    let lib = p.serve.library.clone();
    match tokio::task::spawn_blocking(move || lib.album(&id)).await {
        Ok(Ok(Some(a))) => {
            let mut v = album_view(&a);
            v["tracks"] = a.song.unwrap_or_default().into_iter().map(|s| track_view(&own_track(s))).collect::<Vec<Value>>().into();
            Json(v).into_response()
        }
        Ok(Ok(None)) => refused(StatusCode::NOT_FOUND, "No such album."),
        _ => refused(StatusCode::INTERNAL_SERVER_ERROR, "The library cannot be read now."),
    }
}

#[derive(Deserialize)]
struct GuestAsk {
    track: QueueTrack,
    #[serde(default)]
    how: Option<String>,
}

/// A library track asked of the host, next or at the end: waiting for the host's answer, never straight in.
async fn ask(State(p): State<Portal>, Extension(Client(addr)): Extension<Client>, headers: HeaderMap, Json(b): Json<GuestAsk>) -> Response {
    let (token, name) = match p.guest(&headers) {
        Ok(g) => g,
        Err(r) => return r,
    };
    if b.track.source != crate::source::cratarr::KIND.id || b.track.id.trim().is_empty() {
        return refused(StatusCode::BAD_REQUEST, "Ask for a track of the library.");
    }
    let how = match super::along::how_of(b.how.as_deref()) {
        Ok(h) => h,
        Err(e) => return crate::api::ApiError::from(e).into_response(),
    };
    // The library's own facts of it, not the guest's words.
    let (lib, id) = (p.serve.library.clone(), b.track.id.clone());
    let track = match tokio::task::spawn_blocking(move || lib.songs_by_id(&[id])).await {
        Ok(Ok(mut songs)) => match songs.pop() {
            Some(s) => own_track(s),
            None => return refused(StatusCode::NOT_FOUND, "No such track."),
        },
        _ => return refused(StatusCode::INTERNAL_SERVER_ERROR, "The library cannot be read now."),
    };
    let Some((player, leader)) = p.host() else { return refused(StatusCode::CONFLICT, "Nothing plays here now.") };
    if p.on.requests.of_guest(&token).iter().filter(|r| r.state == Asked::Pending).count() >= WAITING_MOST {
        return too_many("Three of your requests wait already.");
    }
    if let Err(why) = p.guests.may(&[format!("ask:{token}"), format!("ask@{addr}")], REQUESTS, now_secs()) {
        return too_many(&why);
    }
    match p.on.ask_for(Asker::Guest(name), &player, &leader, track, how, Some(token), false).await {
        Ok(r) => Json(json!({ "id": r.id, "state": r.state })).into_response(),
        Err(e) => crate::api::ApiError::from(e).into_response(),
    }
}

/// The guest's own requests, the newest first, each with its state.
async fn own_requests(State(p): State<Portal>, headers: HeaderMap) -> Response {
    match p.guest(&headers) {
        Ok((token, _)) => Json(p.on.requests.of_guest(&token)).into_response(),
        Err(r) => r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    /// A library of one album, two songs.
    fn served() -> Arc<Serve> {
        let s = Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        let row = |track: i64, n: &str, title: &str| crate::catalogue::ServedRow {
            rel: format!("Miles Davis/Kind of Blue (1959)/{n} {title}.flac"),
            track: Some(track),
            born: Some(1_700_000_000),
            format: Some("FLAC".into()),
            kbps: Some(900),
            duration_ms: Some(540_000),
            size: 50_000_000,
            mtime: 1,
            tags: [("title", title), ("artist", "Miles Davis"), ("albumartist", "Miles Davis"), ("album", "Kind of Blue"), ("tracknumber", n)]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            analysis: None,
        };
        let folders = [super::super::library::FolderRows { folder: 0, root: "/music".into(), rows: vec![row(1, "01", "So What"), row(2, "02", "Freddie Freeloader")] }];
        s.library.refresh(&folders, &crate::catalogue::Rules::default()).unwrap();
        s
    }

    /// A request from `from`, with the guest's cookie when given; its answer's status, cookie and body.
    async fn send(app: &Router, method: &str, uri: &str, from: &str, cookie: Option<&str>, body: Option<Value>) -> (StatusCode, Option<String>, Value) {
        let mut b = axum::http::Request::builder().method(method).uri(uri);
        if let Some(c) = cookie {
            b = b.header("cookie", c);
        }
        let mut req = match body {
            Some(v) => b.header("content-type", "application/json").body(Body::from(v.to_string())).unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        req.extensions_mut().insert(ConnectInfo(SocketAddr::new(from.parse().unwrap(), 50_000)));
        let r = app.clone().oneshot(req).await.unwrap();
        let status = r.status();
        let cookie = r.headers().get(header::SET_COOKIE).and_then(|v| v.to_str().ok()).and_then(|v| v.split(';').next()).map(str::to_string);
        let bytes = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
        (status, cookie, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    const HOME: &str = "192.168.1.20";
    const AWAY: &str = "203.0.113.9";

    #[tokio::test]
    async fn a_guest_at_home_names_themselves_sees_what_plays_searches_and_asks_the_host() {
        let serve = served();
        let so_what = crate::serve::ids::song(1);
        let on = PlayOn::for_tests().await;
        let _salon = on.connect("Salon").unwrap();
        on.own("Salon", "ann");
        let track = QueueTrack { source: "cratarr".into(), id: so_what.clone(), name: "So What".into(), ..QueueTrack::default() };
        on.command("Salon", crate::app::PlayOnCommand::play(vec![track], 0, 0)).await.unwrap();
        let version = on.get("Salon").unwrap().queue_version;
        on.report("Salon", crate::app::PlayerReport { index: 0, version: Some(version), position_ms: 60_000, playing: true, ..Default::default() }).unwrap();
        let setting = Arc::new(Mutex::new(Guest { on: false, host: Some(GuestHost::Player("Salon".into())) }));
        let s = setting.clone();
        let app = router_with(on.clone(), serve, Arc::new(move || s.lock().unwrap().clone()));
        let sam = Some(json!({ "name": " Sam " }));

        // Off, or away from home: nothing is here.
        assert_eq!(send(&app, "POST", "/api/guest/hello", HOME, None, sam.clone()).await.0, StatusCode::NOT_FOUND, "off");
        assert_eq!(send(&app, "GET", "/guest", HOME, None, None).await.0, StatusCode::NOT_FOUND);
        setting.lock().unwrap().on = true;
        assert_eq!(send(&app, "POST", "/api/guest/hello", AWAY, None, sam.clone()).await.0, StatusCode::NOT_FOUND, "away");
        assert_eq!(send(&app, "GET", "/api/guest/now", AWAY, None, None).await.0, StatusCode::NOT_FOUND);

        // A name first.
        assert_eq!(send(&app, "GET", "/api/guest/now", HOME, None, None).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(send(&app, "POST", "/api/guest/hello", HOME, None, Some(json!({ "name": "  " }))).await.0, StatusCode::BAD_REQUEST);
        let (status, cookie, body) = send(&app, "POST", "/api/guest/hello", HOME, None, sam).await;
        assert_eq!((status, body["name"].as_str()), (StatusCode::OK, Some("Sam")));
        let cookie = cookie.expect("the guest's cookie");
        assert!(cookie.starts_with("cratarr-guest="));
        let me = Some(cookie.as_str());

        // What plays.
        let (status, _, now) = send(&app, "GET", "/api/guest/now", HOME, me, None).await;
        assert_eq!((status, now["track"]["id"].as_str(), now["playing"].as_bool()), (StatusCode::OK, Some(so_what.as_str()), Some(true)));
        assert!(now["position_ms"].as_u64().unwrap() >= 60_000);
        assert!(now["cover"].as_str().unwrap().starts_with("/api/cover/cratarr/"));

        // The library.
        let (status, _, found) = send(&app, "GET", "/api/guest/search?q=freddie", HOME, me, None).await;
        assert_eq!(status, StatusCode::OK);
        let freddie = &found["tracks"][0];
        assert_eq!((freddie["source"].as_str(), freddie["id"].as_str(), freddie["album"].as_str()), (Some("cratarr"), Some(crate::serve::ids::song(2).as_str()), Some("Kind of Blue")));
        assert!(freddie["cover"].is_string());

        // A request: the host's, waiting, never straight in.
        let ask = |id: &str, source: &str| Some(json!({ "track": { "source": source, "id": id }, "how": "next" }));
        assert_eq!(send(&app, "POST", "/api/guest/queue-requests", HOME, me, ask("x", "bandcamp")).await.0, StatusCode::BAD_REQUEST, "library tracks only");
        assert_eq!(send(&app, "POST", "/api/guest/queue-requests", HOME, me, ask("tr-nope", "cratarr")).await.0, StatusCode::NOT_FOUND);
        let (status, _, asked) = send(&app, "POST", "/api/guest/queue-requests", HOME, me, ask(&crate::serve::ids::song(2), "cratarr")).await;
        assert_eq!((status, asked["state"].as_str()), (StatusCode::OK, Some("pending")));
        let waiting = on.requests.waiting_for("ann", None);
        assert_eq!(waiting.len(), 1, "the host's owner answers it");
        assert_eq!((&waiting[0].from, waiting[0].player.as_str(), waiting[0].track.name.as_str()), (&Asker::Guest("Sam".into()), "Salon", "Freddie Freeloader"));
        let (_, _, mine) = send(&app, "GET", "/api/guest/queue-requests", HOME, me, None).await;
        assert_eq!(mine.as_array().map(Vec::len), Some(1));

        // One request every 30 seconds.
        let (status, _, again) = send(&app, "POST", "/api/guest/queue-requests", HOME, me, ask(&so_what, "cratarr")).await;
        assert_eq!((status, again["error"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("One request every 30 seconds.")));

        // Thirty searches a minute.
        for _ in 0..29 {
            assert_eq!(send(&app, "GET", "/api/guest/search?q=so", HOME, me, None).await.0, StatusCode::OK);
        }
        assert_eq!(send(&app, "GET", "/api/guest/search?q=so", HOME, me, None).await.0, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn a_host_player_whose_music_was_handed_over_is_where_it_went() {
        let on = PlayOn::for_tests().await;
        let (_salon, _kitchen) = (on.connect("Salon").unwrap(), on.connect("Kitchen").unwrap());
        on.own("Salon", "ann");
        on.own("Kitchen", "ann");
        let track = QueueTrack { source: "cratarr".into(), id: "tr-1".into(), name: "So What".into(), ..QueueTrack::default() };
        on.command("Salon", crate::app::PlayOnCommand::play(vec![track], 0, 0)).await.unwrap();
        let setting = Guest { on: true, host: Some(GuestHost::Player("Salon".into())) };
        let p = Portal { on: on.clone(), serve: served(), guests: Arc::default(), setting: Arc::new(move || setting.clone()) };
        assert_eq!(p.host(), Some(("Salon".to_string(), "ann".to_string())));
        on.command("Salon", crate::app::PlayOnCommand::Handover { to: "Kitchen".into() }).await.unwrap();
        assert_eq!(p.host(), Some(("Kitchen".to_string(), "ann".to_string())), "where the music went");
        // A user host picks the player they listen on: the one the music went to, once it says it plays.
        let version = on.get("Kitchen").unwrap().queue_version;
        on.report("Kitchen", crate::app::PlayerReport { index: 0, version: Some(version), playing: true, ..Default::default() }).unwrap();
        assert_eq!(on.playing_of("ann").as_deref(), Some("Kitchen"));
    }

    #[test]
    fn limits_go_by_guest_and_by_address_and_only_what_passes_counts() {
        let g = Guests::default();
        let keys = |t: &str, a: &str| [format!("ask:{t}"), format!("ask@{a}")];
        assert!(g.may(&keys("t1", "a"), REQUESTS, 1_000).is_ok());
        assert_eq!(g.may(&keys("t2", "a"), REQUESTS, 1_010), Err("One request every 30 seconds.".into()), "another name, the same address");
        assert!(g.may(&keys("t2", "b"), REQUESTS, 1_010).is_ok());
        for i in 1..10 {
            assert!(g.may(&keys("t1", "a"), REQUESTS, 1_000 + i * 30).is_ok());
        }
        assert_eq!(g.may(&keys("t1", "a"), REQUESTS, 1_300), Err("Ten requests an hour at most.".into()));
        assert!(g.may(&keys("t1", "a"), REQUESTS, 4_600).is_ok(), "an hour on");
    }
}
