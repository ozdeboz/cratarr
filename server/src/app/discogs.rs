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

/// One GET of Discogs' API (`path` with its query), paced; none where Discogs has no such thing.
async fn get(path: &str) -> Result<Option<Value>> {
    let creds = saved().await;
    let mut again = true;
    loop {
        pace(creds.is_some()).await;
        let mut req = client()?.get(format!("{API}{path}"));
        if let Some((k, s)) = &creds {
            req = req.header(reqwest::header::AUTHORIZATION, format!("Discogs key={k}, secret={s}"));
        }
        let r = req.send().await?;
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
    let v: Value = http.get(format!("https://musicbrainz.org/ws/2/{entity}/{id}?inc=url-rels&fmt=json")).send().await?.error_for_status()?.json().await?;
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
    if let Some(rg) = release_group
        && let Some(id) = id_in(&mb_links("release-group", rg).await?, "master")
    {
        return Ok(Some(id));
    }
    if artist.trim().is_empty() || title.trim().is_empty() || saved().await.is_none() {
        return Ok(None);
    }
    let mut q = reqwest::Url::parse("https://x/database/search")?;
    q.query_pairs_mut().append_pair("type", "master").append_pair("artist", artist).append_pair("release_title", title).append_pair("per_page", "5");
    let Some(v) = get(&format!("/database/search?{}", q.query().unwrap_or_default())).await? else { return Ok(None) };
    let want = super::import::loose(&format!("{artist} - {title}"));
    Ok(v["results"].as_array().into_iter().flatten().find(|r| r["title"].as_str().is_some_and(|t| super::import::loose(t) == want)).and_then(|r| r["id"].as_u64()))
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
    let found = if super::discover::http()?.head(&caa).send().await.is_ok_and(|r| r.status().is_success()) {
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
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
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
    let v: Value = http.get(format!("https://musicbrainz.org/ws/2/release/{release}?inc=release-groups&fmt=json")).send().await?.error_for_status()?.json().await?;
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
}
