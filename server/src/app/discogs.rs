//! Discogs (0.9.5, the owner: "Discogs, with an app key and secret"): albums' masters, their images, genres and
//! styles, and artists' images. The server's own key and secret (Settings › Server; never shown, never in an address)
//! go in the Authorization header; without them Discogs answers at its anonymous pace and gives no images. One pacer
//! for every call, kept under the limit Discogs reports in each reply. An album's master is MusicBrainz's link from its
//! release group first, else Discogs' own search by artist and title (which needs the key).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::Value;

use crate::userdata::UserData;
use super::musicbrainz::Asked as _;
use super::reach::Sent as _;

/// Where the key and secret are kept (user data's server choices, as the server's Last.fm key).
pub const KEY: &str = "discogs:key";
pub const SECRET: &str = "discogs:secret";
const API: &str = "https://api.discogs.com";
const UA: &str = concat!("Cratarr/", env!("CARGO_PKG_VERSION"), " +https://github.com/ozdeboz/cratarr");
/// Discogs' limits a minute, until a reply says.
const ANONYMOUS_PER_MINUTE: u32 = 25;
const KEYED_PER_MINUTE: u32 = 60;
/// Too many asked (429): once more after this.
const TOO_MANY_WAIT: Duration = Duration::from_secs(60);
/// An album's cover found (or not) is kept this long before it is looked for again.
const COVER_FOUND_SECS: i64 = 30 * 86_400;
const COVER_NONE_SECS: i64 = 7 * 86_400;
/// An album without genres asked about again after this.
const GENRES_AGAIN_SECS: i64 = 30 * 86_400;
/// Albums without genres asked about in one pass.
const GENRES_BATCH: usize = 100;

/// The limit a minute Discogs said last (0: not yet).
static PER_MINUTE: AtomicU32 = AtomicU32::new(0);
/// Asked nothing until then (Unix ms): the reply said the minute's calls are used up.
static HOLD_UNTIL: AtomicU64 = AtomicU64::new(0);

/// The limit a minute Discogs reported last, for Settings.
pub fn per_minute() -> Option<u32> {
    Some(PER_MINUTE.load(Ordering::Relaxed)).filter(|n| *n > 0)
}

/// A key or secret as Discogs gives them: letters and digits.
pub fn valid(s: &str) -> bool {
    (8..=64).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// The server's key and secret, where both are saved.
pub fn credentials(ud: &UserData) -> Result<Option<(String, String)>> {
    let get = |k: &str| ud.import_choice(k).map(|v| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
    Ok(match (get(KEY)?, get(SECRET)?) {
        (Some(k), Some(s)) => Some((k, s)),
        _ => None,
    })
}

/// The key and secret as saved now; none without Cratarr's own library.
async fn saved() -> Option<(String, String)> {
    let ud = crate::serve::instance()?.userdata.clone();
    tokio::task::spawn_blocking(move || credentials(&ud)).await.ok()?.ok()?
}

fn client() -> Result<&'static reqwest::Client> {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(c) = C.get() {
        return Ok(c);
    }
    let c = reqwest::Client::builder().timeout(Duration::from_secs(30)).user_agent(UA).build()?;
    Ok(C.get_or_init(|| c))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The time between two calls at `per_minute`, a little over it.
fn gap(per_minute: u32) -> Duration {
    Duration::from_millis(60_000 / u64::from(per_minute.max(1)) + 50)
}

/// Waits for this call's turn: one at a time, `gap` apart, and none while the minute's calls are used up.
async fn pace(keyed: bool) {
    static NEXT: OnceLock<tokio::sync::Mutex<Instant>> = OnceLock::new();
    let mut next = NEXT.get_or_init(|| tokio::sync::Mutex::new(Instant::now())).lock().await;
    let hold = HOLD_UNTIL.load(Ordering::Relaxed).saturating_sub(now_ms());
    if hold > 0 {
        tokio::time::sleep(Duration::from_millis(hold)).await;
    }
    let now = Instant::now();
    if *next > now {
        tokio::time::sleep(*next - now).await;
    }
    let limit = per_minute().unwrap_or(if keyed { KEYED_PER_MINUTE } else { ANONYMOUS_PER_MINUTE });
    *next = Instant::now() + gap(limit);
}

/// What a reply says of the limits: the limit a minute, and a hold of a minute when few calls are left.
fn note_limits(h: &reqwest::header::HeaderMap) {
    let num = |name: &str| h.get(name).and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<u32>().ok());
    if let Some(n) = num("x-discogs-ratelimit") {
        PER_MINUTE.store(n, Ordering::Relaxed);
    }
    if num("x-discogs-ratelimit-remaining").is_some_and(|left| left <= 1) {
        HOLD_UNTIL.store(now_ms() + 60_000, Ordering::Relaxed);
    }
}

/// A master Discogs' search finds for free words (0076, Beyond): its id, title and artist (its "Artist - Title" split,
/// the artist's "(2)" taken off), year, picture, and its page on Discogs (shown with "Data from Discogs").
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FoundMaster {
    pub id: u64,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub cover: Option<String>,
    pub url: String,
}

/// Discogs' masters for the words, a page of `limit` from `offset` (the key's search: none without it); kept a few
/// minutes per query, so typing back and forth asks nothing again.
pub async fn search_masters(q: &str, limit: usize, offset: usize) -> Result<Vec<FoundMaster>> {
    type Kept = std::collections::HashMap<String, (std::time::Instant, Vec<FoundMaster>)>;
    static KEPT: std::sync::OnceLock<std::sync::Mutex<Kept>> = std::sync::OnceLock::new();
    let q = q.trim();
    if q.is_empty() || saved().await.is_none() {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 50);
    let page = offset / limit + 1;
    let key = format!("{}|{limit}|{page}", super::import::key(q));
    let kept = KEPT.get_or_init(Default::default);
    if let Some((at, found)) = kept.lock().unwrap_or_else(|p| p.into_inner()).get(&key).cloned()
        && at.elapsed() < Duration::from_secs(600)
    {
        return Ok(found);
    }
    let mut u = reqwest::Url::parse("https://x/database/search")?;
    u.query_pairs_mut().append_pair("type", "master").append_pair("q", q).append_pair("per_page", &limit.to_string()).append_pair("page", &page.to_string());
    let Some(v) = get(&format!("/database/search?{}", u.query().unwrap_or_default())).await? else { return Ok(Vec::new()) };
    let found: Vec<FoundMaster> = v["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let id = r["id"].as_u64()?;
            let whole = r["title"].as_str()?;
            let (artist, title) = whole.split_once(" - ").unwrap_or(("", whole));
            let cover = r["cover_image"].as_str().or(r["thumb"].as_str()).filter(|c| !c.is_empty() && !c.contains("spacer.gif")).map(str::to_string);
            let url = r["uri"].as_str().map_or_else(|| format!("https://www.discogs.com/master/{id}"), |p| format!("https://www.discogs.com{p}"));
            Some(FoundMaster { id, title: title.trim().to_string(), artist: clean_name(artist.trim()), year: r["year"].as_str().and_then(|y| y.parse().ok()), cover, url })
        })
        .collect();
    kept.lock().unwrap_or_else(|p| p.into_inner()).insert(key, (std::time::Instant::now(), found.clone()));
    Ok(found)
}

/// One GET of Discogs' API (`path` with its query), paced; none where Discogs has no such thing.
async fn get(path: &str) -> Result<Option<Value>> {
    // Switched off (0.9.6): Discogs is asked nothing; as if it had nothing.
    if !super::switches::is_on(super::switches::DISCOGS) {
        return Ok(None);
    }
    let creds = saved().await;
    let mut again = true;
    loop {
        pace(creds.is_some()).await;
        let mut req = client()?.get(format!("{API}{path}"));
        if let Some((k, s)) = &creds {
            req = req.header(reqwest::header::AUTHORIZATION, format!("Discogs key={k}, secret={s}"));
        }
        let r = req.sent().await?;
        note_limits(r.headers());
        match r.status() {
            reqwest::StatusCode::TOO_MANY_REQUESTS if std::mem::take(&mut again) => {
                tokio::time::sleep(TOO_MANY_WAIT).await;
                continue;
            }
            reqwest::StatusCode::NOT_FOUND => return Ok(None),
            _ => return Ok(Some(r.error_for_status()?.json().await?)),
        }
    }
}

/// An image as Discogs lists it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Image {
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub primary: bool,
}

fn images_of(v: &Value) -> Vec<Image> {
    v["images"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| {
            Some(Image {
                url: i["uri"].as_str().filter(|u| !u.is_empty())?.to_string(),
                width: i["width"].as_u64().map(|w| w as u32),
                height: i["height"].as_u64().map(|h| h as u32),
                primary: i["type"].as_str() == Some("primary"),
            })
        })
        .collect()
}

/// A master as Cratarr uses it: its images (the primary first), genres and styles, and its main release.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Master {
    pub id: u64,
    pub images: Vec<Image>,
    pub genres: Vec<String>,
    pub styles: Vec<String>,
    pub main_release: Option<u64>,
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten().filter_map(|s| s.as_str().map(str::to_string)).collect()
}

pub async fn master(id: u64) -> Result<Option<Master>> {
    let Some(v) = get(&format!("/masters/{id}")).await? else { return Ok(None) };
    let mut images = images_of(&v);
    images.sort_by_key(|i| !i.primary);
    Ok(Some(Master { id, images, genres: strings(&v["genres"]), styles: strings(&v["styles"]), main_release: v["main_release"].as_u64() }))
}

/// A Discogs id of this kind (`master`, `artist`, `release`) among MusicBrainz's links ("…discogs.com/master/123").
pub fn id_in(urls: &[String], kind: &str) -> Option<u64> {
    let mark = format!("discogs.com/{kind}/");
    urls.iter().find_map(|u| u.split(&mark).nth(1)?.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok())
}

/// A MusicBrainz entity's links (`release-group`, `artist`), MusicBrainz asked at its pace.
async fn mb_links(entity: &str, id: &str) -> Result<Vec<String>> {
    if !is_mbid(id) {
        return Ok(Vec::new());
    }
    let http = super::discover::http()?;
    let v: Value = http.get(format!("https://musicbrainz.org/ws/2/{entity}/{id}?inc=url-rels&fmt=json")).asked().await?.error_for_status()?.json().await?;
    tokio::time::sleep(super::discover::MB_PAUSE).await;
    Ok(v["relations"].as_array().into_iter().flatten().filter_map(|r| r["url"]["resource"].as_str().map(str::to_string)).collect())
}

/// A MusicBrainz id's shape (so it can go into an address as it is).
pub fn is_mbid(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// An album's master: MusicBrainz's link from its release group, else Discogs' search by artist and title (with the
/// key only: Discogs' search needs it), the first whose "Artist - Title" is the album's.
pub async fn master_of(release_group: Option<&str>, artist: &str, title: &str) -> Result<Option<u64>> {
    Ok(masters_of(release_group, artist, title).await?.into_iter().next())
}

/// An album's possible masters: MusicBrainz's link from its release group alone, else every one of Discogs' first five
/// search results whose "Artist - Title" is the album's, in the search's order (a same-named single or EP among them).
async fn masters_of(release_group: Option<&str>, artist: &str, title: &str) -> Result<Vec<u64>> {
    if let Some(rg) = release_group
        && let Some(id) = id_in(&mb_links("release-group", rg).await?, "master")
    {
        return Ok(vec![id]);
    }
    if artist.trim().is_empty() || title.trim().is_empty() || saved().await.is_none() {
        return Ok(Vec::new());
    }
    let mut q = reqwest::Url::parse("https://x/database/search")?;
    q.query_pairs_mut().append_pair("type", "master").append_pair("artist", artist).append_pair("release_title", title).append_pair("per_page", "5");
    let Some(v) = get(&format!("/database/search?{}", q.query().unwrap_or_default())).await? else { return Ok(Vec::new()) };
    let want = super::import::loose(&format!("{artist} - {title}"));
    Ok(v["results"].as_array().into_iter().flatten().filter(|r| r["title"].as_str().is_some_and(|t| super::import::loose(t) == want)).filter_map(|r| r["id"].as_u64()).collect())
}

/// A Discogs release's tracklist as counted: its tracks (an index track's parts each one, headings none), and their
/// length (s) where every one has one.
fn tracklist_size(release: &Value) -> (usize, Option<u64>) {
    let (mut n, mut secs) = (0usize, Some(0u64));
    for t in release["tracklist"].as_array().into_iter().flatten() {
        let parts: Vec<&Value> = match t["type_"].as_str() {
            Some("heading") => continue,
            Some("index") => match t["sub_tracks"].as_array() {
                Some(s) if !s.is_empty() => s.iter().collect(),
                _ => vec![t],
            },
            _ => vec![t],
        };
        for p in parts {
            n += 1;
            secs = secs.zip(p["duration"].as_str().and_then(super::renumber::duration)).map(|(a, b)| a + u64::from(b));
        }
    }
    (n, secs.filter(|_| n > 0))
}

/// Whether a Discogs release is the album (0.10, the owner: albums were typed from their same-named single): by length
/// where Discogs gives every track's, within 15%; else by tracks, within two or a fifth, whichever is more. An empty
/// tracklist is never the album.
fn fits((n, length): (usize, Option<u64>), tracks: usize, secs: u64) -> bool {
    if n == 0 || tracks == 0 {
        return false;
    }
    match length {
        Some(l) if secs > 0 => l.abs_diff(secs) * 100 <= secs * 15,
        _ => n.abs_diff(tracks) <= 2 || n.abs_diff(tracks) * 5 <= tracks,
    }
}

/// The main release of the first of the album's masters that `fits` it, each asked in turn (`main_release`); none where
/// none does: the type is then left unknown, never taken from another record.
async fn fitting_release<F, Fut>(masters: Vec<u64>, tracks: usize, secs: u64, mut main_release: F) -> Result<Option<Value>>
where
    F: FnMut(u64) -> Fut,
    Fut: std::future::Future<Output = Result<Option<Value>>>,
{
    for m in masters {
        if let Some(r) = main_release(m).await?
            && fits(tracklist_size(&r), tracks, secs)
        {
            return Ok(Some(r));
        }
    }
    Ok(None)
}

/// A master's main release as Discogs gives it.
async fn main_release_of(master_id: u64) -> Result<Option<Value>> {
    match master(master_id).await?.and_then(|m| m.main_release) {
        Some(id) => release_raw(id).await,
        None => Ok(None),
    }
}

/// Some of a master's releases (its `main` one left out: the master's images are its), each with what it is
/// ("CD · Germany · 2012").
pub async fn versions(master: u64, main: Option<u64>, most: usize) -> Result<Vec<(u64, String)>> {
    let Some(v) = get(&format!("/masters/{master}/versions?per_page={}", most + 1)).await? else { return Ok(Vec::new()) };
    Ok(v["versions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let id = r["id"].as_u64().filter(|id| Some(*id) != main)?;
            let format = r["major_formats"].as_array().and_then(|f| f.first()).and_then(|f| f.as_str()).or(r["format"].as_str()).unwrap_or_default();
            let year = r["released"].as_str().and_then(|d| d.get(..4)).filter(|y| *y != "0000").unwrap_or_default();
            let what: Vec<&str> = [format, r["country"].as_str().unwrap_or_default(), year].into_iter().filter(|s| !s.is_empty()).collect();
            Some((id, if what.is_empty() { "Release".to_string() } else { what.join(" · ") }))
        })
        .take(most)
        .collect())
}

/// A master's releases with their format as Discogs lists them ("CD", "File", "Vinyl"), at most `most` (Renumber).
pub async fn version_formats(master: u64, most: usize) -> Result<Vec<(u64, String)>> {
    let Some(v) = get(&format!("/masters/{master}/versions?per_page={most}")).await? else { return Ok(Vec::new()) };
    Ok(v["versions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let format = r["major_formats"].as_array().and_then(|f| f.first()).and_then(|f| f.as_str()).or(r["format"].as_str()).unwrap_or_default();
            Some((r["id"].as_u64()?, format.to_string()))
        })
        .collect())
}

/// A release as Discogs gives it: its formats and tracklist (Renumber).
pub async fn release_raw(release: u64) -> Result<Option<Value>> {
    get(&format!("/releases/{release}")).await
}

/// A release's images, the primary first.
pub async fn release_images(release: u64) -> Result<Vec<Image>> {
    let Some(v) = get(&format!("/releases/{release}")).await? else { return Ok(Vec::new()) };
    let mut images = images_of(&v);
    images.sort_by_key(|i| !i.primary);
    Ok(images)
}

/// An artist's primary image on Discogs, by MusicBrainz's link from the artist; and their Discogs page.
pub async fn artist_image(mbid: &str) -> Result<Option<(Image, String)>> {
    let Some(id) = id_in(&mb_links("artist", mbid).await?, "artist") else { return Ok(None) };
    let Some(v) = get(&format!("/artists/{id}")).await? else { return Ok(None) };
    let mut images = images_of(&v);
    images.sort_by_key(|i| !i.primary);
    let page = v["uri"].as_str().map_or_else(|| format!("https://www.discogs.com/artist/{id}"), str::to_string);
    Ok(images.into_iter().next().map(|i| (i, page)))
}

/// An album's cover for a release group not in the library (Discover, missing albums, requests): the Cover Art
/// Archive's where it has one, else its Discogs master's primary image; none otherwise. Kept a month (a week when none).
pub async fn release_group_cover(rg: &str) -> Result<Option<String>> {
    if !is_mbid(rg) {
        return Ok(None);
    }
    let Some(served) = crate::serve::instance() else { return Ok(None) };
    let now = super::moment::now_secs();
    let (ud, key) = (served.userdata.clone(), rg.to_string());
    if let Some((at, url)) = tokio::task::spawn_blocking(move || ud.rg_cover(&key)).await??
        && now - at <= if url.is_some() { COVER_FOUND_SECS } else { COVER_NONE_SECS }
    {
        return Ok(url);
    }
    let caa = format!("https://coverartarchive.org/release-group/{rg}/front-250");
    let found = if super::discover::http()?.head(&caa).sent().await.is_ok_and(|r| r.status().is_success()) {
        Some(caa)
    } else {
        match master_of(Some(rg), "", "").await? {
            Some(id) => master(id).await?.and_then(|m| m.images.into_iter().next()).map(|i| i.url),
            None => None,
        }
    };
    let (ud, key, url) = (served.userdata.clone(), rg.to_string(), found.clone());
    tokio::task::spawn_blocking(move || ud.set_rg_cover(&key, url.as_deref(), now)).await??;
    Ok(found)
}

/// The background asker for albums whose files carry no genre (the owner: fill gaps only, files never written):
/// their master's genres and styles, kept by album; the library is built with them (`Library::set_extra_genres`).
pub fn start() {
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_secs(600)).await;
        loop {
            if let Err(e) = genres_pass().await {
                tracing::info!("discogs: genres not asked now: {e:#}");
            }
            if let Err(e) = types_pass().await {
                tracing::info!("discogs: release types not asked now: {e:#}");
            }
            // Classical's (0.10): only through MusicBrainz's links.
            if let Err(e) = super::works::discogs_pass().await {
                tracing::info!("discogs: the works' genres and names not asked now: {e:#}");
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

/// Albums whose type MusicBrainz did not give (0.9.6): the format descriptions ("EP", "Single", "LP", "Compilation")
/// of the main release of the first of their masters whose tracklist fits the album (0.10), kept by album
/// (`dc:<album id>`). An album with a MusicBrainz release waits until MusicBrainz was asked about it; one Discogs typed
/// by an older rule is asked again (`UserData::release_types_asked`).
async fn types_pass() -> Result<()> {
    let Some(served) = crate::serve::instance() else { return Ok(()) };
    let now = super::moment::now_secs();
    let (lib, ud) = (served.library.clone(), served.userdata.clone());
    let due: Vec<(String, String, String, String, usize, u64)> = tokio::task::spawn_blocking(move || -> Result<_> {
        let asked = ud.release_types_asked("dc:", now - GENRES_AGAIN_SECS)?;
        let mb = ud.release_types_asked("mb:", 0)?;
        Ok(lib
            .albums_to_type()?
            .into_iter()
            .filter(|a| !asked.contains(&a.0) && (!is_mbid(&a.1) || mb.contains(&a.1)))
            .take(GENRES_BATCH)
            .collect())
    })
    .await??;
    if due.is_empty() {
        return Ok(());
    }
    let http = super::discover::http()?;
    let mut found = Vec::new();
    for (album, release, artist, title, tracks, duration_ms) in due {
        let rg = match is_mbid(&release) {
            true => release_group_of(&http, &release).await.unwrap_or_default(),
            false => None,
        };
        let masters = match masters_of(rg.as_deref(), &artist, &title).await {
            Ok(m) => m,
            Err(e) => {
                tracing::info!("discogs: {artist} – {title}: {e:#}");
                continue;
            }
        };
        let words: Vec<String> = fitting_release(masters, tracks, duration_ms / 1000, main_release_of)
            .await?
            .map(|v| v["formats"].as_array().into_iter().flatten().flat_map(|f| f["descriptions"].as_array().into_iter().flatten().filter_map(|d| d.as_str().map(str::to_string)).collect::<Vec<_>>()).collect())
            .unwrap_or_default();
        let (primary, secondary) = crate::serve::library::release_words(words.iter().map(String::as_str));
        found.push((format!("dc:{album}"), primary, secondary));
    }
    let n = found.len();
    let ud = served.userdata.clone();
    tokio::task::spawn_blocking(move || ud.set_release_types(&found, super::moment::now_secs())).await??;
    tracing::info!("discogs: release types asked for {n} albums");
    Ok(())
}

async fn genres_pass() -> Result<()> {
    let Some(served) = crate::serve::instance() else { return Ok(()) };
    let now = super::moment::now_secs();
    let (lib, ud) = (served.library.clone(), served.userdata.clone());
    let due: Vec<(String, String, String, String)> = tokio::task::spawn_blocking(move || -> Result<_> {
        let asked = ud.discogs_genres_asked(now - GENRES_AGAIN_SECS)?;
        Ok(lib.albums_without_genres()?.into_iter().filter(|a| !asked.contains(&a.0)).take(GENRES_BATCH).collect())
    })
    .await??;
    let http = super::discover::http()?;
    let mut found: Vec<(String, Vec<String>)> = Vec::new();
    for (album, release, artist, title) in due {
        let rg = match is_mbid(&release) {
            true => release_group_of(&http, &release).await.unwrap_or_default(),
            false => None,
        };
        let genres = match master_of(rg.as_deref(), &artist, &title).await {
            Ok(Some(id)) => master(id).await?.map(|m| m.genres.into_iter().chain(m.styles).collect()).unwrap_or_default(),
            Ok(None) => Vec::new(),
            Err(e) => {
                tracing::info!("discogs: {artist} – {title}: {e:#}");
                continue;
            }
        };
        found.push((album, genres));
    }
    let n = found.len();
    let ud = served.userdata.clone();
    tokio::task::spawn_blocking(move || ud.set_discogs_genres(&found, super::moment::now_secs())).await??;
    if n > 0 {
        tracing::info!("discogs: {n} albums without genres asked");
    }
    Ok(())
}

/// A MusicBrainz release's release group.
pub(super) async fn release_group_of(http: &reqwest::Client, release: &str) -> Result<Option<String>> {
    let v: Value = http.get(format!("https://musicbrainz.org/ws/2/release/{release}?inc=release-groups&fmt=json")).asked().await?.error_for_status()?.json().await?;
    tokio::time::sleep(super::discover::MB_PAUSE).await;
    Ok(v["release-group"]["id"].as_str().map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discogs_ids_are_read_from_musicbrainz_links() {
        let urls = vec!["https://www.wikidata.org/wiki/Q1".to_string(), "https://www.discogs.com/master/12345-Ott-Skylon".to_string(), "https://www.discogs.com/artist/678".to_string()];
        assert_eq!(id_in(&urls, "master"), Some(12345));
        assert_eq!(id_in(&urls, "artist"), Some(678));
        assert_eq!(id_in(&urls, "release"), None);
    }

    #[test]
    fn the_pace_stays_under_the_limit_and_keys_are_letters_and_digits() {
        assert!(gap(60) > Duration::from_secs(1));
        assert!(gap(25) > Duration::from_millis(2400));
        assert!(valid("AbCdEf1234567890"));
        assert!(!valid("short") && !valid("with space 12345") && !valid("a=b&c=d12345"));
        assert!(is_mbid("d21c604f-13a0-3fc7-9965-1ed65fd919d0") && !is_mbid("../x"));
    }

    #[test]
    fn images_put_the_primary_first_and_skip_empty_ones() {
        let v = serde_json::json!({ "images": [
            { "type": "secondary", "uri": "https://i.discogs.com/b.jpg", "width": 500, "height": 500 },
            { "type": "primary", "uri": "https://i.discogs.com/a.jpg", "width": 600, "height": 600 },
            { "type": "primary", "uri": "" }
        ] });
        let mut i = images_of(&v);
        i.sort_by_key(|i| !i.primary);
        assert_eq!(i.iter().map(|i| i.url.as_str()).collect::<Vec<_>>(), ["https://i.discogs.com/a.jpg", "https://i.discogs.com/b.jpg"]);
        assert_eq!(i[0].width, Some(600));
    }

    /// A release as Discogs gives it: one track a length ("" for none), and its format descriptions.
    fn release(lengths: &[&str], descriptions: &[&str]) -> Value {
        let tracks: Vec<Value> = lengths
            .iter()
            .enumerate()
            .map(|(i, d)| serde_json::json!({ "position": (i + 1).to_string(), "type_": "track", "title": format!("Track {}", i + 1), "duration": d }))
            .collect();
        serde_json::json!({ "formats": [{ "name": "CD", "descriptions": descriptions }], "tracklist": tracks })
    }

    #[test]
    fn a_tracklist_counts_its_tracks_and_their_length_where_every_one_has_one() {
        let v = serde_json::json!({ "tracklist": [
            { "type_": "heading", "title": "Act 1" },
            { "type_": "index", "title": "Scene 1", "sub_tracks": [{ "type_": "track", "duration": "10:00" }, { "type_": "track", "duration": "5:00" }] },
            { "type_": "track", "duration": "1:02:03" }
        ] });
        assert_eq!(tracklist_size(&v), (3, Some(15 * 60 + 3723)));
        assert_eq!(tracklist_size(&release(&["4:00", ""], &[])), (2, None), "one without a length: none for all");
        assert_eq!(tracklist_size(&serde_json::json!({})), (0, None));
    }

    #[test]
    fn a_release_fits_the_album_by_length_where_known_else_by_tracks() {
        assert!(fits((14, None), 12, 3000) && fits((24, None), 20, 3000), "within two, or a fifth");
        assert!(!fits((16, None), 12, 3000));
        assert!(fits((13, Some(66 * 60)), 9, 60 * 60), "the length decides where Discogs gives it");
        assert!(!fits((12, Some(75 * 60)), 12, 60 * 60));
        assert!(!fits((0, None), 12, 3000), "an empty tracklist never");
        // Die Walküre, 220 minutes, typed an EP from a four-track release: not the album, with lengths or without.
        assert!(!fits(tracklist_size(&release(&["6:00"; 4], &["EP"])), 40, 220 * 60));
        assert!(!fits(tracklist_size(&release(&[""; 4], &["EP"])), 40, 220 * 60));
    }

    #[tokio::test]
    async fn a_master_whose_release_does_not_fit_is_passed_over_for_a_later_one_that_does() {
        // Since I Left You: the title track's single (four tracks, 22 minutes) found first, the album (18, an hour) after.
        let single = release(&["5:30"; 4], &["Single"]);
        let album = release(&["3:20"; 18], &["Album"]);
        let mains: std::collections::HashMap<u64, Value> = [(1, single), (2, album.clone())].into();
        let ask = |m: u64| {
            let r = mains.get(&m).cloned();
            async move { Ok::<_, anyhow::Error>(r) }
        };
        assert_eq!(fitting_release(vec![1, 2], 18, 3600, &ask).await.unwrap(), Some(album));
        assert_eq!(fitting_release(vec![1], 18, 3600, &ask).await.unwrap(), None, "none fits: no type, never the single's");
    }
}

// ---- What Discogs says of an album, an artist, a release group (0.9.8, the owner: "Missing genres also do not yet get
// Discogs info because there is a lot more there"; labels, catalogue numbers, credits, artist profiles).

/// An album's facts on Discogs: its master's genres, styles and year, and its main release's labels with their
/// catalogue numbers and its credits.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlbumFacts {
    pub master: Option<u64>,
    pub release: Option<u64>,
    pub year: Option<u32>,
    pub genres: Vec<String>,
    pub styles: Vec<String>,
    pub labels: Vec<Label>,
    pub credits: Vec<Credit>,
    /// Its page on Discogs (the master's, else the release's).
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Label {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catno: Option<String>,
}

/// Who did what on a release (`role`: "Producer", "Mastered By"…), on which tracks where not all ("A1 to A3").
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Credit {
    pub name: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracks: Option<String>,
}

/// An artist on Discogs: the profile, plain; other names; members of a group, and the groups of a member.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArtistFacts {
    pub id: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_name: Option<String>,
    pub profile: String,
    pub aliases: Vec<String>,
    pub variations: Vec<String>,
    pub members: Vec<Member>,
    pub groups: Vec<Member>,
    pub urls: Vec<String>,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Member {
    pub name: String,
    pub active: bool,
}

/// A name as Discogs writes one, without its "(2)" for a second artist of the same name.
pub fn clean_name(name: &str) -> String {
    let n = name.trim();
    match n.rsplit_once(" (") {
        Some((base, rest)) if rest.strip_suffix(')').is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit())) => base.trim().to_string(),
        _ => n.to_string(),
    }
}

/// Discogs' text markup made plain: `[a=Name]` and `[l=Label]` as their names, `[url=…]text[/url]` as its text, styles
/// (`[b]`, `[i]`, `[u]`) and links by number alone (`[a123]`, `[r456]`) left out.
pub fn plain_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let tag = &after[..close];
        rest = &after[close + 1..];
        if let Some(name) = tag.strip_prefix("a=").or_else(|| tag.strip_prefix("l=")).or_else(|| tag.strip_prefix("g=")) {
            out.push_str(&clean_name(name));
        }
        // Anything else ([b], [/url], [url=…], [a123], [r456]…): nothing of its own; a link's text is what follows it.
    }
    out.push_str(rest);
    out.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").trim().to_string()
}

fn credits_of(v: &Value) -> Vec<Credit> {
    let one = |c: &Value, tracks: Option<String>| -> Option<Credit> {
        let name = c["anv"].as_str().filter(|a| !a.is_empty()).or(c["name"].as_str())?;
        let role = c["role"].as_str().unwrap_or_default().trim();
        (!role.is_empty()).then(|| Credit { name: clean_name(name), role: role.to_string(), tracks: tracks.filter(|t| !t.is_empty()) })
    };
    let mut out: Vec<Credit> = v["extraartists"].as_array().into_iter().flatten().filter_map(|c| one(c, c["tracks"].as_str().map(str::to_string))).collect();
    for t in v["tracklist"].as_array().into_iter().flatten() {
        let at = t["position"].as_str().map(str::to_string);
        for c in t["extraartists"].as_array().into_iter().flatten() {
            if let Some(credit) = one(c, at.clone()) {
                out.push(credit);
            }
        }
    }
    out
}

fn labels_of(v: &Value) -> Vec<Label> {
    let mut out: Vec<Label> = Vec::new();
    for l in v["labels"].as_array().into_iter().flatten() {
        let Some(name) = l["name"].as_str() else { continue };
        let catno = l["catno"].as_str().map(str::trim).filter(|c| !c.is_empty() && !c.eq_ignore_ascii_case("none")).map(str::to_string);
        let label = Label { name: clean_name(name), catno };
        if !out.contains(&label) {
            out.push(label);
        }
    }
    out
}

/// An album's facts: its master (MusicBrainz's link from its release group, else Discogs' search), and the master's
/// main release. None where Discogs has no master for it.
pub async fn album_facts(release_group: Option<&str>, artist: &str, title: &str) -> Result<Option<AlbumFacts>> {
    let Some(id) = master_of(release_group, artist, title).await? else { return Ok(None) };
    let Some(v) = get(&format!("/masters/{id}")).await? else { return Ok(None) };
    let mut facts = AlbumFacts {
        master: Some(id),
        release: v["main_release"].as_u64(),
        year: v["year"].as_u64().filter(|y| *y > 0).map(|y| y as u32),
        genres: strings(&v["genres"]),
        styles: strings(&v["styles"]),
        url: v["uri"].as_str().map(str::to_string),
        ..AlbumFacts::default()
    };
    if let Some(release) = facts.release
        && let Some(r) = get(&format!("/releases/{release}")).await?
    {
        facts.labels = labels_of(&r);
        facts.credits = credits_of(&r);
        if facts.year.is_none() {
            facts.year = r["year"].as_u64().filter(|y| *y > 0).map(|y| y as u32);
        }
    }
    Ok(Some(facts))
}

/// A release group's genres, styles and year (Discover's suggestions): its master's, by MusicBrainz's link alone.
pub async fn release_group_facts(release_group: &str) -> Result<Option<AlbumFacts>> {
    if !is_mbid(release_group) {
        return Ok(None);
    }
    let Some(id) = master_of(Some(release_group), "", "").await? else { return Ok(None) };
    Ok(master(id).await?.map(|m| AlbumFacts { master: Some(id), release: m.main_release, genres: m.genres, styles: m.styles, ..AlbumFacts::default() }))
}

/// An artist's facts: by MusicBrainz's link from their id, else Discogs' search by name (the key's; an exact name
/// only).
pub async fn artist_facts(mbid: Option<&str>, name: &str) -> Result<Option<ArtistFacts>> {
    let mut id = match mbid {
        Some(m) => id_in(&mb_links("artist", m).await?, "artist"),
        None => None,
    };
    if id.is_none() && !name.trim().is_empty() && saved().await.is_some() {
        let mut q = reqwest::Url::parse("https://x/database/search")?;
        q.query_pairs_mut().append_pair("type", "artist").append_pair("q", name).append_pair("per_page", "5");
        if let Some(v) = get(&format!("/database/search?{}", q.query().unwrap_or_default())).await? {
            let want = super::import::loose(name);
            id = v["results"].as_array().into_iter().flatten().find(|r| r["title"].as_str().is_some_and(|t| super::import::loose(&clean_name(t)) == want)).and_then(|r| r["id"].as_u64());
        }
    }
    let Some(id) = id else { return Ok(None) };
    let Some(v) = get(&format!("/artists/{id}")).await? else { return Ok(None) };
    Ok(Some(artist_of(id, &v)))
}

/// Discogs' genres and styles of a release or master MusicBrainz links (0.10, `app::works`: `release:<id>`,
/// `master:<id>`); empty where Discogs has none. Never a search: the link is MusicBrainz's.
pub async fn linked_genres(link: &str) -> Result<Vec<String>> {
    let path = match link.split_once(':') {
        Some(("release", id)) if id.chars().all(|c| c.is_ascii_digit()) => format!("/releases/{id}"),
        Some(("master", id)) if id.chars().all(|c| c.is_ascii_digit()) => format!("/masters/{id}"),
        _ => return Ok(Vec::new()),
    };
    let Some(v) = get(&path).await? else { return Ok(Vec::new()) };
    Ok(strings(&v["genres"]).into_iter().chain(strings(&v["styles"])).collect())
}

/// A Discogs artist's real name and aliases, by the id MusicBrainz links (0.10, `app::works`); none where Discogs has no
/// such artist.
pub async fn artist_names(id: u64) -> Result<Option<(Option<String>, Vec<String>)>> {
    let Some(v) = get(&format!("/artists/{id}")).await? else { return Ok(None) };
    let a = artist_of(id, &v);
    Ok(Some((a.real_name, a.aliases)))
}

fn artist_of(id: u64, v: &Value) -> ArtistFacts {
    let people = |k: &str| -> Vec<Member> {
        v[k].as_array().into_iter().flatten().filter_map(|m| Some(Member { name: clean_name(m["name"].as_str()?), active: m["active"].as_bool().unwrap_or(true) })).collect()
    };
    ArtistFacts {
        id,
        name: clean_name(v["name"].as_str().unwrap_or_default()),
        real_name: v["realname"].as_str().map(plain_text).filter(|r| !r.is_empty()),
        profile: plain_text(v["profile"].as_str().unwrap_or_default()),
        aliases: v["aliases"].as_array().into_iter().flatten().filter_map(|a| a["name"].as_str().map(clean_name)).collect(),
        variations: strings(&v["namevariations"]),
        members: people("members"),
        groups: people("groups"),
        urls: strings(&v["urls"]),
        url: v["uri"].as_str().map_or_else(|| format!("https://www.discogs.com/artist/{id}"), str::to_string),
    }
}

#[cfg(test)]
mod facts_tests {
    use super::*;

    #[test]
    fn discogs_markup_and_names_are_made_plain() {
        assert_eq!(clean_name("Jon Hopkins (2)"), "Jon Hopkins");
        assert_eq!(clean_name("The Orb (UK)"), "The Orb (UK)");
        assert_eq!(
            plain_text("[b]Swedish[/b] producer, one half of [a=Studio (3)] with [a123456]. Released on [l=Kompakt]. See [url=https://x.y]his site[/url]."),
            "Swedish producer, one half of Studio with . Released on Kompakt. See his site."
        );
    }

    #[test]
    fn a_release_gives_its_labels_and_credits() {
        let v = serde_json::json!({
            "labels": [{ "name": "Studio Barnhus", "catno": "BARN 042" }, { "name": "Studio Barnhus", "catno": "BARN 042" }, { "name": "Self Released", "catno": "none" }],
            "extraartists": [{ "name": "Axel Boman", "anv": "", "role": "Producer", "tracks": "" }, { "name": "Mandy Parnell (2)", "role": "Mastered By" }],
            "tracklist": [{ "position": "A2", "extraartists": [{ "name": "Kornél Kovács", "anv": "Kornél", "role": "Vocals" }] }]
        });
        assert_eq!(labels_of(&v), [Label { name: "Studio Barnhus".into(), catno: Some("BARN 042".into()) }, Label { name: "Self Released".into(), catno: None }]);
        let c = credits_of(&v);
        assert_eq!(c[0], Credit { name: "Axel Boman".into(), role: "Producer".into(), tracks: None });
        assert_eq!(c[1].name, "Mandy Parnell");
        assert_eq!(c[2], Credit { name: "Kornél".into(), role: "Vocals".into(), tracks: Some("A2".into()) });
    }

    #[test]
    fn an_artist_gives_profile_names_and_members() {
        let v = serde_json::json!({
            "name": "Studio (3)", "realname": "", "profile": "Swedish duo of [a=Dan Lissvik] and Rasmus Hägg.",
            "aliases": [{ "name": "Studio Barnhus (2)" }], "namevariations": ["Studio Sweden"],
            "members": [{ "name": "Dan Lissvik", "active": true }, { "name": "Rasmus Hägg", "active": false }],
            "urls": ["https://studio.example"], "uri": "https://www.discogs.com/artist/1-Studio"
        });
        let a = artist_of(1, &v);
        assert_eq!((a.name.as_str(), a.real_name.as_deref(), a.profile.as_str()), ("Studio", None, "Swedish duo of Dan Lissvik and Rasmus Hägg."));
        assert_eq!(a.aliases, ["Studio Barnhus"]);
        assert_eq!(a.members[1], Member { name: "Rasmus Hägg".into(), active: false });
    }
}
