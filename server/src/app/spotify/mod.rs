//! Spotify as a Library source (decision 0073, 0.9.9): each account's saved albums and tracks, its playlists and the
//! artists it follows, from a live connection with the account's own Spotify app (`web`) or from Spotify's data export
//! (`export`), kept per account (`store`); what Cratarr has of each item (`marks`), and asking for the rest through the
//! requests (0070). Nothing plays on Spotify, and Spotify's sound never plays through Cratarr.

pub mod export;
pub mod marks;
pub mod store;
pub mod web;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use marks::{AlbumView, Answers, ArtistView, Library, Marks, PlaylistView, TrackView};
use store::{IMPORT, LIVE, Store};

/// The live lists are read again after this long.
const AGAIN: Duration = Duration::from_secs(6 * 3600);
/// How often the background looks for lists to read and questions for MusicBrainz.
const LOOK: Duration = Duration::from_secs(10 * 60);
/// Between two questions to MusicBrainz (its rule is one a second; others ask too).
const MB_PACE: Duration = Duration::from_millis(1500);
/// Matching reads the library again after this long.
const LIBRARY_FOR: Duration = Duration::from_secs(60);

/// An album as kept: Spotify's, or as an export names it (no year, cover, UPC nor tracks then).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub year: Option<i32>,
    /// Spotify's picture of it.
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub added_at: Option<i64>,
    #[serde(default)]
    pub upc: Option<String>,
    #[serde(default)]
    pub tracks: Vec<AlbumTrack>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AlbumTrack {
    pub title: String,
    #[serde(default)]
    pub length_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    #[serde(default)]
    pub album_artist: Option<String>,
    #[serde(default)]
    pub album_id: Option<String>,
    #[serde(default)]
    pub length_ms: Option<u64>,
    #[serde(default)]
    pub added_at: Option<i64>,
    #[serde(default)]
    pub isrc: Option<String>,
    /// Its album's picture.
    #[serde(default)]
    pub cover: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub collaborative: bool,
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub photo: Option<String>,
}

/// An account's four lists, as one way brought them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Lists {
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub artists: Vec<Artist>,
}

impl Lists {
    fn counts(&self) -> Counts {
        Counts { albums: self.albums.len(), tracks: self.tracks.len(), playlists: self.playlists.len(), artists: self.artists.len() }
    }
}

static STORE: OnceLock<Arc<Store>> = OnceLock::new();
static DATA: OnceLock<PathBuf> = OnceLock::new();
/// The accounts whose live lists are being read now.
static READING: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// MusicBrainz is being asked (one pass at a time).
static ASKING: Mutex<bool> = Mutex::new(false);

/// Opens the store in Cratarr's data folder and starts the background (reading live lists every 6 hours, asking
/// MusicBrainz). Without a data folder there is no Spotify.
pub fn start(data_dir: Option<&str>) {
    let Some(dir) = data_dir else { return };
    let dir = PathBuf::from(dir);
    match Store::open(Some(&dir.join("spotify.sqlite"))) {
        Ok(s) => {
            let _ = STORE.set(Arc::new(s));
            let _ = DATA.set(dir);
        }
        Err(e) => {
            tracing::warn!("spotify: the store could not be opened: {e:#}");
            return;
        }
    }
    tokio::spawn(async {
        // After the catalogue has caught up.
        tokio::time::sleep(Duration::from_secs(120)).await;
        loop {
            look().await;
            tokio::time::sleep(LOOK).await;
        }
    });
}

pub fn store() -> Result<Arc<Store>> {
    STORE.get().cloned().context("Spotify needs Cratarr's data folder")
}

/// One look: the live lists due read again, then MusicBrainz asked what it has not been.
async fn look() {
    let Ok(s) = store() else { return };
    let now = crate::app::moment::now_secs();
    match s.connected() {
        Ok(accounts) => {
            for (account, at) in accounts {
                if at.is_none_or(|at| now - at >= AGAIN.as_secs() as i64) {
                    read_live(account).await;
                }
            }
        }
        Err(e) => tracing::warn!("spotify: {e:#}"),
    }
    ask_musicbrainz().await;
}

fn reading() -> std::sync::MutexGuard<'static, Option<HashSet<String>>> {
    READING.lock().unwrap_or_else(|p| p.into_inner())
}

/// Reads the account's live lists now (none while a read of them runs), then asks MusicBrainz about them.
pub async fn read_live(account: String) {
    if !reading().get_or_insert_with(HashSet::new).insert(account.clone()) {
        return;
    }
    let result = read_live_inner(&account).await;
    let now = crate::app::moment::now_secs();
    if let Err(e) = &result {
        tracing::warn!(target: "spotify", account = %account, "reading the lists: {e:#}");
    }
    if let Ok(s) = store() {
        let _ = s.synced(&account, now, result.err().map(|e| format!("{e:#}")).as_deref());
    }
    reading().get_or_insert_with(HashSet::new).remove(&account);
    tokio::spawn(ask_musicbrainz());
}

async fn read_live_inner(account: &str) -> Result<()> {
    let s = store()?;
    let a = s.account(account)?;
    let (Some(client), Some(refresh)) = (a.client_id, a.refresh) else { return Ok(()) };
    let http = web::http()?;
    let tokens = web::refresh(&http, &client, &refresh).await?;
    if tokens.refresh != refresh {
        s.refreshed(account, &tokens.refresh)?;
    }
    let (me, _) = web::me(&http, &tokens.access).await?;
    let lists = web::read(&http, &tokens.access, &me).await?;
    // Disconnected meanwhile: not kept.
    if s.account(account)?.refresh.is_none() {
        return Ok(());
    }
    let (s2, a2) = (s.clone(), account.to_string());
    tokio::task::spawn_blocking(move || s2.put_lists(&a2, LIVE, crate::app::moment::now_secs(), None, &lists)).await??;
    Ok(())
}

/// Asks MusicBrainz, a question at a time, what it has not been asked about the lists (every account's), keeping each
/// answer: the ISRCs' recordings, the UPCs' releases, the release groups of albums by name.
pub async fn ask_musicbrainz() {
    {
        let mut asking = ASKING.lock().unwrap_or_else(|p| p.into_inner());
        if *asking {
            return;
        }
        *asking = true;
    }
    if let Err(e) = ask_musicbrainz_inner().await {
        tracing::warn!("spotify: asking MusicBrainz: {e:#}");
    }
    *ASKING.lock().unwrap_or_else(|p| p.into_inner()) = false;
}

async fn ask_musicbrainz_inner() -> Result<()> {
    let s = store()?;
    let s2 = s.clone();
    let (lists, kept) = tokio::task::spawn_blocking(move || -> Result<_> { Ok((s2.all_lists()?, s2.all_mb()?)) }).await??;
    let mut ask: Vec<String> = lists.iter().flat_map(marks::questions).filter(|q| !kept.contains_key(q)).collect();
    ask.sort();
    ask.dedup();
    if ask.is_empty() {
        return Ok(());
    }
    let http = web::http()?;
    for q in ask {
        if !crate::app::switches::is_on(crate::app::switches::MUSICBRAINZ) {
            break;
        }
        let answer = match ask_one(&http, &q).await {
            Ok(a) => a,
            Err(e) => {
                // MusicBrainz away or busy: the rest next time.
                tracing::info!("spotify: MusicBrainz: {e:#}");
                break;
            }
        };
        let (s2, q2) = (s.clone(), q.clone());
        tokio::task::spawn_blocking(move || s2.put_mb(&q2, &answer, crate::app::moment::now_secs())).await??;
        tokio::time::sleep(MB_PACE).await;
    }
    Ok(())
}

/// One question's answer as kept: recordings (a JSON list), a release (`marks::Release`), or a release group (a
/// JSON string, or null).
async fn ask_one(http: &reqwest::Client, q: &str) -> Result<String> {
    use crate::app::musicbrainz::Asked;
    let get = |url: reqwest::Url| async move {
        let r = http.get(url).asked().await?;
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(serde_json::Value::Null);
        }
        Ok::<_, anyhow::Error>(r.error_for_status()?.json::<serde_json::Value>().await?)
    };
    if let Some(isrc) = q.strip_prefix("isrc:") {
        let v = get(reqwest::Url::parse(&format!("https://musicbrainz.org/ws/2/isrc/{isrc}?fmt=json"))?).await?;
        let ids: Vec<&str> = v["recordings"].as_array().into_iter().flatten().filter_map(|r| r["id"].as_str()).collect();
        return Ok(serde_json::to_string(&ids)?);
    }
    if let Some(upc) = q.strip_prefix("upc:") {
        let mut url = reqwest::Url::parse("https://musicbrainz.org/ws/2/release")?;
        url.query_pairs_mut().append_pair("query", &format!("barcode:{upc} OR barcode:0{upc}")).append_pair("fmt", "json").append_pair("limit", "10");
        let v = get(url).await?;
        let found: Vec<&serde_json::Value> = v["releases"].as_array().into_iter().flatten().filter(|r| r["score"].as_i64().unwrap_or(0) >= 100).collect();
        let release = marks::Release {
            releases: found.iter().filter_map(|r| r["id"].as_str().map(str::to_string)).collect(),
            release_group: found.first().and_then(|r| r["release-group"]["id"].as_str()).map(str::to_string),
        };
        return Ok(serde_json::to_string(&release)?);
    }
    let (artist, album) = q.strip_prefix("name:").and_then(|n| n.split_once('|')).ok_or_else(|| anyhow!("an unknown question: {q}"))?;
    // The keys are lower case, without punctuation: as a search, that is what MusicBrainz compares too.
    let mut url = reqwest::Url::parse("https://musicbrainz.org/ws/2/release-group")?;
    url.query_pairs_mut()
        .append_pair("query", &format!("releasegroup:\"{}\" AND artist:\"{}\"", lucene(album), lucene(artist)))
        .append_pair("fmt", "json")
        .append_pair("limit", "1");
    let v = get(url).await?;
    let best = v["release-groups"].as_array().and_then(|a| a.first()).filter(|g| g["score"].as_i64().unwrap_or(0) >= 90);
    Ok(serde_json::to_string(&best.and_then(|g| g["id"].as_str()))?)
}

/// A name inside a quoted Lucene phrase.
fn lucene(s: &str) -> String {
    s.chars().filter(|c| *c != '"' && *c != '\\').collect()
}

// ---- What the pages read ----

#[derive(Debug, Serialize)]
pub struct State {
    pub client_id: Option<String>,
    pub live: Option<Live>,
    pub imported: Option<Imported>,
    /// Which lists the Library shows: the live ones once read, else the export's.
    pub shown: Option<&'static str>,
    pub counts: Counts,
}

#[derive(Debug, Serialize)]
pub struct Live {
    pub user: Option<String>,
    pub name: Option<String>,
    pub since: Option<i64>,
    pub sync: Sync,
}

#[derive(Debug, Serialize)]
pub struct Sync {
    pub state: &'static str,
    pub at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Imported {
    pub at: i64,
    pub file: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Counts {
    pub albums: usize,
    pub tracks: usize,
    pub playlists: usize,
    pub artists: usize,
}

/// The lists the account's Library shows, and which way they came.
pub fn shown(s: &Store, account: &str) -> Result<Option<(&'static str, Lists)>> {
    if s.account(account)?.refresh.is_some()
        && let Some(l) = s.lists(account, LIVE)?
    {
        return Ok(Some((LIVE, l)));
    }
    Ok(s.lists(account, IMPORT)?.map(|l| (IMPORT, l)))
}

pub fn state(account: &str) -> Result<State> {
    let s = store()?;
    let a = s.account(account)?;
    let running = reading().as_ref().is_some_and(|r| r.contains(account));
    let live = a.refresh.is_some().then(|| Live {
        user: a.user.clone(),
        name: a.name.clone(),
        since: a.since,
        sync: Sync { state: if running { "running" } else if a.sync_error.is_some() { "failed" } else { "idle" }, at: a.sync_at, error: a.sync_error.clone() },
    });
    let imported = s.lists_at(account, IMPORT)?.map(|(at, file)| Imported { at, file });
    let shown = shown(&s, account)?;
    Ok(State { client_id: a.client_id, live, imported, shown: shown.as_ref().map(|(w, _)| *w), counts: shown.map(|(_, l)| l.counts()).unwrap_or_default() })
}

static LIBRARY: Mutex<Option<(std::time::Instant, Arc<Library>)>> = Mutex::new(None);

/// The library as matching reads it, read again once a minute old. Blocking.
pub(crate) fn library() -> Result<Arc<Library>> {
    let mut kept = LIBRARY.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((at, l)) = kept.as_ref()
        && at.elapsed() < LIBRARY_FOR
    {
        return Ok(l.clone());
    }
    let dir = DATA.get().context("Spotify needs Cratarr's data folder")?;
    // What Cratarr's own library's tracks play by, as an import's playlist names them.
    let l = Arc::new(Library::read(&dir.join("library.sqlite"), crate::source::cratarr::KIND.id)?);
    *kept = Some((std::time::Instant::now(), l.clone()));
    Ok(l)
}

/// Matching an account's lists: the library, MusicBrainz's answers, the account's requests. Blocking.
fn with_marks<T>(account: &str, f: impl FnOnce(&Marks, &Lists) -> T) -> Result<Option<T>> {
    let s = store()?;
    let Some((_, lists)) = shown(&s, account)? else { return Ok(None) };
    let library = library()?;
    let answers = Answers(s.all_mb()?);
    let serve = crate::serve::instance().context("Cratarr's own library is not open")?;
    let requests = marks::requests_by_key(serve.userdata.requests(Some(account), None, 10_000)?);
    let m = Marks { library: &library, answers: &answers, requests };
    Ok(Some(f(&m, &lists)))
}

/// Whether a view's status passes `?status=` and its names `?q=`.
fn wanted(status: Option<&str>, q: Option<&str>, item: &str, names: &[&str]) -> bool {
    let by_status = status.is_none_or(|s| s.is_empty() || s.split(',').any(|s| s == item));
    let q = q.map(|q| crate::app::import::loose(q)).filter(|q| !q.is_empty());
    by_status && q.is_none_or(|q| names.iter().any(|n| crate::app::import::loose(n).contains(&q)))
}

pub fn albums(account: &str, status: Option<&str>, q: Option<&str>) -> Result<Vec<AlbumView>> {
    Ok(with_marks(account, |m, l| {
        l.albums
            .iter()
            .map(|a| m.album(a, Some(cover("album", &a.id))))
            .filter(|v| wanted(status, q, v.status, &[&v.title, &v.artist]))
            .collect()
    })?
    .unwrap_or_default())
}

pub fn tracks(account: &str, status: Option<&str>, q: Option<&str>) -> Result<Vec<TrackView>> {
    Ok(with_marks(account, |m, l| l.tracks.iter().map(|t| m.track(t)).filter(|v| wanted(status, q, v.status, &[&v.title, &v.artist, &v.album])).collect())?
        .unwrap_or_default())
}

pub fn playlists(account: &str) -> Result<Vec<PlaylistView>> {
    Ok(with_marks(account, |m, l| l.playlists.iter().map(|p| m.playlist(p, p.cover.as_ref().map(|_| cover("playlist", &p.id)), false)).collect())?
        .unwrap_or_default())
}

pub fn playlist(account: &str, id: &str) -> Result<Option<PlaylistView>> {
    Ok(with_marks(account, |m, l| l.playlists.iter().find(|p| p.id == id).map(|p| m.playlist(p, p.cover.as_ref().map(|_| cover("playlist", &p.id)), true)))?
        .flatten())
}

pub fn artists(account: &str, status: Option<&str>) -> Result<Vec<ArtistView>> {
    Ok(with_marks(account, |m, l| {
        l.artists.iter().map(|a| m.artist(a, a.photo.as_ref().map(|_| cover("artist", &a.id)))).filter(|v| wanted(status, None, v.status, &[])).collect()
    })?
    .unwrap_or_default())
}

/// A picture's address on Cratarr (`/api/spotify/cover/{kind}/{id}`).
fn cover(kind: &str, id: &str) -> String {
    format!("/api/spotify/cover/{kind}/{}", url_part(id))
}

fn url_part(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// Where a picture comes from: Spotify's own, Cratarr's cover of the album it matched (its source and id), or Cover Art
/// Archive's by the release group MusicBrainz found.
pub enum Picture {
    Remote(String),
    Own(String, String),
}

/// The picture of an item of the account's lists. Blocking.
pub fn picture(account: &str, kind: &str, id: &str) -> Result<Option<Picture>> {
    let s = store()?;
    let Some((_, lists)) = shown(&s, account)? else { return Ok(None) };
    let remote = match kind {
        "album" => lists.albums.iter().find(|a| a.id == id).and_then(|a| a.cover.clone()).or_else(|| lists.tracks.iter().find(|t| t.album_id.as_deref() == Some(id)).and_then(|t| t.cover.clone())),
        "playlist" => lists.playlists.iter().find(|p| p.id == id).and_then(|p| p.cover.clone()),
        "artist" => lists.artists.iter().find(|a| a.id == id).and_then(|a| a.photo.clone()),
        _ => None,
    };
    if let Some(url) = remote {
        return Ok(Some(Picture::Remote(url)));
    }
    if kind != "album" {
        return Ok(None);
    }
    // An export's album: the library's cover where it is here, else Cover Art Archive's.
    let Some(a) = lists.albums.iter().find(|a| a.id == id) else { return Ok(None) };
    let view = with_marks(account, |m, _| m.album(a, None))?;
    Ok(view.and_then(|v| match (v.album, v.release_group) {
        (Some(own), _) => Some(Picture::Own(own.source, own.id)),
        (None, Some(rg)) => Some(Picture::Remote(format!("https://coverartarchive.org/release-group/{rg}/front-250"))),
        _ => None,
    }))
}

/// Whether a picture's address is one Cratarr relays: Spotify's image hosts and Cover Art Archive only.
pub fn relayable(url: &str) -> bool {
    reqwest::Url::parse(url).ok().is_some_and(|u| {
        let host = u.host_str().unwrap_or_default();
        u.scheme() == "https" && (host.ends_with(".scdn.co") || host.ends_with(".spotifycdn.com") || host == "coverartarchive.org")
    })
}

// ---- Connecting ----

pub fn set_client(account: &str, client_id: &str) -> Result<()> {
    let id = client_id.trim();
    if !id.is_empty() && !(id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit())) {
        anyhow::bail!("That is not a Spotify Client ID: 32 letters and digits, from the app's page in Spotify's dashboard.");
    }
    store()?.set_client(account, Some(id).filter(|i| !i.is_empty()))
}

/// Where to send the browser to sign in to Spotify with the account's own app.
pub fn connect(account: &str, callback: &str, app: bool) -> Result<String> {
    let a = store()?.account(account)?;
    let client = a.client_id.context("Enter your Spotify app's Client ID first.")?;
    let callback = callback.trim();
    let is_https = callback.starts_with("https://") || callback.starts_with("http://127.0.0.1") || callback.starts_with("http://[::1]");
    if !is_https || !callback.ends_with("/api/spotify/callback") {
        anyhow::bail!("Spotify comes back only to an https address: open Cratarr by its https address to connect.");
    }
    web::sign_in(account, &client, callback, app)
}

/// Spotify's way back: its code traded for the sign-in, kept for the account that began it, its lists read. Answers
/// the account and whether it began in the app (to send the browser back to the right place) and what went wrong.
pub async fn come_back(state: &str, code: Option<&str>, error: Option<&str>) -> (bool, Option<String>) {
    let Some(p) = web::take_pending(state) else { return (false, Some("That sign-in has expired: connect again".into())) };
    let app = p.app;
    if let Some(e) = error {
        return (app, Some(if e == "access_denied" { "Not allowed on Spotify".into() } else { format!("Spotify said: {e}") }));
    }
    let Some(code) = code else { return (app, Some("Spotify gave no code".into())) };
    let result = async {
        let http = web::http()?;
        let tokens = web::trade(&http, &p, code).await?;
        let (user, name) = web::me(&http, &tokens.access).await?;
        store()?.signed_in(&p.account, &tokens.refresh, &user, &name, crate::app::moment::now_secs())?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    match result {
        Ok(()) => {
            tokio::spawn(read_live(p.account));
            (app, None)
        }
        Err(e) => (app, Some(format!("{e:#}"))),
    }
}

pub fn disconnect(account: &str) -> Result<()> {
    store()?.disconnect(account)
}

// ---- The export ----

/// What an export brought to the lists (or would): how many of each, and how many items are in, partly or missing.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Brought {
    pub albums: usize,
    pub tracks: usize,
    pub playlists: usize,
    pub artists: usize,
    #[serde(rename = "in")]
    pub here: usize,
    pub partly: usize,
    pub missing: usize,
}

/// An export's lists matched (a preview) or kept as the account's, replacing the export before; MusicBrainz asked
/// about them afterwards. Blocking but for the questions' start.
pub fn take_export(account: &str, file: &str, lists: Lists, preview: bool) -> Result<Brought> {
    let s = store()?;
    let library = library()?;
    let answers = Answers(s.all_mb()?);
    let m = Marks { library: &library, answers: &answers, requests: Default::default() };
    let mut b = Brought { albums: lists.albums.len(), tracks: lists.tracks.len(), playlists: lists.playlists.len(), artists: lists.artists.len(), ..Default::default() };
    let statuses = lists
        .albums
        .iter()
        .map(|a| m.album(a, None).status)
        .chain(lists.tracks.iter().map(|t| m.track(t).status))
        .chain(lists.playlists.iter().map(|p| m.playlist(p, None, false).status))
        .chain(lists.artists.iter().map(|a| m.artist(a, None).status));
    for st in statuses {
        match st {
            marks::IN => b.here += 1,
            marks::PARTLY => b.partly += 1,
            _ => b.missing += 1,
        }
    }
    if !preview {
        s.put_lists(account, IMPORT, crate::app::moment::now_secs(), Some(file), &lists)?;
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            rt.spawn(ask_musicbrainz());
        }
    }
    Ok(b)
}

pub fn forget_export(account: &str) -> Result<bool> {
    store()?.forget_lists(account, IMPORT)
}
