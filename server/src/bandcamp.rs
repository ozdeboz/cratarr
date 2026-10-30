//! Bandcamp, read to play from (decision 0071): an album's page, its tracks and each one's stream; an artist's list of
//! releases; the search box's own answers (Bandcamp's autocomplete, JSON). Played, never downloaded: nothing is kept on
//! disk. Bandcamp's search page is not read: it answers with a check for browsers (a "Client Challenge"), which Cratarr
//! does not get around.
//!
//! The page is read as yt-dlp's Bandcamp extractor reads it (`yt_dlp/extractor/bandcamp.py`, the Unlicense: public
//! domain): the album's data in the page's `data-tralbum` attribute (HTML-escaped JSON), each track's `file` holding
//! its `mp3-128` stream, its cover the page's `og:image`. Written for Cratarr in Rust.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use crate::app::reach::Sent as _;

/// An album's page read again after this long: its streams' addresses expire.
const KEEP: Duration = Duration::from_secs(5 * 60);
/// The most album pages kept at once.
const KEEP_MOST: usize = 64;
/// At most one page of Bandcamp's read in this long, server-wide (Discover's look, previews, a track's details): the
/// owner asked, 2026-10-25, whether Cratarr is polite to it. A stream played is not paced: it is a listener's.
const PACE: Duration = Duration::from_secs(1);
/// How long Bandcamp is left alone after it says "too many" (429) or "unavailable" (503) without saying how long.
const BACK_OFF: Duration = Duration::from_secs(60);

/// The source a preview's tracks carry in a queue (`QueueTrack::source`): their streams and covers are relayed under
/// Cratarr's own addresses by it (`/api/stream/bandcamp/…`, `/api/file/…`, `/api/cover/…`).
pub const SOURCE: &str = "bandcamp";
/// What a preview's ids start with: no library id does, so listening leaves them out by the id alone.
const ID: &str = "bc.";
/// Bandcamp's MP3 streams are 128 kb/s throughout: a moment's place in the file, for seeking.
pub const BYTES_PER_SEC: u64 = 128_000 / 8;

/// A track's id in a queue: its album's page and its number (0: the album itself, for its cover), so a queue saved or
/// sent to another player still plays it.
pub fn track_id(album: &str, n: u32) -> String {
    use base64::Engine as _;
    format!("{ID}{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{n}|{album}")))
}

/// A preview's id read back: its album's page (on Bandcamp only) and the track's number.
pub fn read_id(id: &str) -> Option<(String, u32)> {
    use base64::Engine as _;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(id.strip_prefix(ID)?).ok()?;
    let (n, url) = std::str::from_utf8(&raw).ok()?.split_once('|')?;
    Some((album_page(url)?, n.parse().ok()?))
}

/// A preview's track (decision 0071): never a play, never in the history, never scrobbled.
pub fn is_preview(id: &str) -> bool {
    id.starts_with(ID)
}

/// Why a preview is refused wherever a track would be kept (a playlist, a download, a copy, a kept item): it is
/// played, never kept, and bought on Bandcamp (decision 0071, amended 2026-10-30).
pub const NOT_KEPT: &str = "A Bandcamp preview is not kept: buy it on Bandcamp.";

/// Why Tape rewind and a wind refuse a preview: they are rendered from a file Cratarr keeps, and it keeps none.
pub const NO_TAPE: &str = "A Bandcamp preview does not wind: Cratarr keeps no file of it.";

/// An album as its page gives it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Album {
    /// Its page, as read (no query, no fragment).
    pub url: String,
    pub title: String,
    pub artist: String,
    pub cover: Option<String>,
    pub tracks: Vec<Track>,
    /// When it came out ("2019-05-17"), as its page says.
    pub released: Option<String>,
    /// Its label: the page's publisher, where that is not the artist themselves.
    pub label: Option<String>,
    /// What the page says about it, and its credits, as written there.
    pub about: Option<String>,
    pub credits: Option<String>,
    /// Bandcamp's tags for it, as the page lists them.
    pub tags: Vec<String>,
}

impl Album {
    /// Its tracks that can be heard, as players hold them (`QueueTrack`): each with its own name, artist, album and
    /// length from the page, its album's id `track_id(page, 0)`.
    pub fn queue_tracks(&self) -> Vec<crate::app::QueueTrack> {
        let album_id = track_id(&self.url, 0);
        self.tracks
            .iter()
            .filter(|t| t.stream.is_some())
            .map(|t| crate::app::QueueTrack {
                source: SOURCE.into(),
                id: track_id(&self.url, t.n),
                name: t.title.clone(),
                artist: Some(t.artist.clone().unwrap_or_else(|| self.artist.clone())),
                album: Some(self.title.clone()),
                album_id: Some(album_id.clone()),
                duration_secs: t.duration_secs,
                n: Some(t.n),
                ..Default::default()
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Track {
    /// Its number on the album (1 on).
    pub n: u32,
    pub title: String,
    /// Its artist where the album's is not its own (a compilation's).
    pub artist: Option<String>,
    pub duration_secs: Option<f64>,
    /// Its stream (MP3, 128 kb/s); `None`: not streamed (a track only bought).
    #[serde(skip)]
    pub stream: Option<String>,
    /// Its own page, where the album's has no stream for it.
    #[serde(skip)]
    pub page: Option<String>,
}

/// A page on Bandcamp's own domain only (`bandcamp.com` and its artists' subdomains): no other address is read. Its
/// address made plain (https, no query, no fragment).
pub fn page(url: &str) -> Option<String> {
    let mut u = reqwest::Url::parse(url).ok()?;
    let host = u.host_str()?.to_ascii_lowercase();
    if !(host == "bandcamp.com" || host.ends_with(".bandcamp.com")) {
        return None;
    }
    u.set_scheme("https").ok()?;
    u.set_query(None);
    u.set_fragment(None);
    Some(u.to_string())
}

/// An album's page (or a lone track's, which Bandcamp shows as an album of one).
pub fn album_page(url: &str) -> Option<String> {
    let p = page(url)?;
    let path = reqwest::Url::parse(&p).ok()?.path().to_string();
    (path.starts_with("/album/") || path.starts_with("/track/")).then_some(p)
}

/// Reads Bandcamp: one client, the album pages kept a few minutes.
pub struct Bandcamp {
    http: reqwest::Client,
    kept: Mutex<HashMap<String, (Instant, Album)>>,
    /// When the next page may be read (PACE apart; later after a back-off).
    next: tokio::sync::Mutex<Instant>,
}

impl Bandcamp {
    pub fn new() -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("Cratarr/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { http, kept: Mutex::new(HashMap::new()), next: tokio::sync::Mutex::new(Instant::now()) })
    }

    /// The one reader, for the server's life.
    pub fn get() -> &'static Bandcamp {
        static ONE: std::sync::OnceLock<Bandcamp> = std::sync::OnceLock::new();
        ONE.get_or_init(|| Bandcamp::new().expect("an HTTP client"))
    }

    /// A request of Bandcamp's (a page, a search), in turn: PACE after the one before, none while it asked to be left
    /// alone.
    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let mut next = self.next.lock().await;
        tokio::time::sleep_until((*next).into()).await;
        let resp = req.sent().await;
        *next = Instant::now() + PACE;
        let resp = resp.context("cannot reach Bandcamp")?;
        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
            let wait = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()?.trim().parse::<u64>().ok())
                .map_or(BACK_OFF, |secs| Duration::from_secs(secs.min(15 * 60)));
            *next = Instant::now() + wait;
            tracing::warn!("Bandcamp answered {status}: left alone for {}s", wait.as_secs());
        }
        Ok(resp)
    }

    async fn text(&self, url: &str) -> Result<String> {
        let resp = self.send(self.http.get(url)).await?;
        if !resp.status().is_success() {
            bail!("Bandcamp answered {}", resp.status());
        }
        Ok(resp.text().await?)
    }

    /// An album's page read (kept a few minutes); a track without a stream on it read from its own page.
    pub async fn album(&self, url: &str) -> Result<Album> {
        let url = album_page(url).context("Not an album's page on Bandcamp")?;
        if let Some((at, a)) = self.kept.lock().unwrap_or_else(|p| p.into_inner()).get(&url)
            && at.elapsed() < KEEP
        {
            return Ok(a.clone());
        }
        let mut album = parse_album(&self.text(&url).await?, &url)?;
        for t in album.tracks.iter_mut().filter(|t| t.stream.is_none()) {
            let Some(p) = t.page.clone() else { continue };
            if let Ok(one) = self.text(&p).await.and_then(|html| parse_album(&html, &p)) {
                t.stream = one.tracks.into_iter().next().and_then(|x| x.stream);
            }
        }
        let mut kept = self.kept.lock().unwrap_or_else(|p| p.into_inner());
        if kept.len() >= KEEP_MOST {
            kept.retain(|_, (at, _)| at.elapsed() < KEEP);
            if kept.len() >= KEEP_MOST {
                kept.clear();
            }
        }
        kept.insert(url, (Instant::now(), album.clone()));
        Ok(album)
    }

    /// An album's cover address at 700 px (`_16`), for the covers Cratarr keeps (app::cover_cache): its page read in
    /// turn, not its tracks' pages, and not kept with the albums being played.
    pub async fn cover(&self, url: &str) -> Result<Option<String>> {
        let url = album_page(url).context("Not an album's page on Bandcamp")?;
        let known = self.kept.lock().unwrap_or_else(|p| p.into_inner()).get(&url).map(|(_, a)| a.cover.clone());
        let cover = match known {
            Some(c) => c,
            None => parse_album(&self.text(&url).await?, &url)?.cover,
        };
        Ok(cover.map(|c| sized(&c, 16)))
    }

    /// A track's stream, asked for as it is played: the response, its status and headers as Bandcamp sends them
    /// (`range` passed on, so a player can seek), and the track's length as its page gives it.
    pub async fn stream(&self, url: &str, n: u32, range: Option<&str>) -> Result<(reqwest::Response, Option<f64>)> {
        let a = self.album(url).await?;
        let t = a.tracks.iter().find(|t| t.n == n).context("No such track on that album")?;
        let s = t.stream.as_deref().context("That track does not stream on Bandcamp")?;
        let mut req = self.http.get(s).timeout(Duration::from_secs(60 * 60));
        if let Some(r) = range {
            req = req.header(reqwest::header::RANGE, r);
        }
        let resp = req.sent().await.context("cannot reach Bandcamp")?;
        if !resp.status().is_success() {
            // An address expired early: read the page again next time.
            self.kept.lock().unwrap_or_else(|p| p.into_inner()).remove(&a.url);
            bail!("Bandcamp answered {}", resp.status());
        }
        Ok((resp, t.duration_secs))
    }

    /// An album taken as read, as if its page had just been read (a fake Bandcamp's, in tests).
    #[cfg(test)]
    pub fn keep(&self, album: Album) {
        let url = album_page(&album.url).unwrap_or_else(|| album.url.clone());
        self.kept.lock().unwrap_or_else(|p| p.into_inner()).insert(url, (Instant::now(), album));
    }

    /// Bandcamp's search box asked for albums: (title, artist, album page), its first answers.
    pub async fn search(&self, artist: &str, album: &str) -> Result<Vec<(String, String, String)>> {
        let body = serde_json::json!({ "search_text": format!("{artist} {album}"), "search_filter": "a", "full_page": false, "fan_id": null });
        let resp = self.send(self.http.post("https://bandcamp.com/api/bcsearch_public_api/1/autocomplete_elastic").json(&body)).await?;
        if !resp.status().is_success() {
            bail!("Bandcamp answered {}", resp.status());
        }
        Ok(parse_autocomplete(&resp.text().await?))
    }

    /// Bandcamp's search box asked for artists: (name, their page), its first answers.
    pub async fn search_artist(&self, name: &str) -> Result<Vec<(String, String)>> {
        let body = serde_json::json!({ "search_text": name, "search_filter": "b", "full_page": false, "fan_id": null });
        let resp = self.send(self.http.post("https://bandcamp.com/api/bcsearch_public_api/1/autocomplete_elastic").json(&body)).await?;
        if !resp.status().is_success() {
            bail!("Bandcamp answered {}", resp.status());
        }
        Ok(parse_artists(&resp.text().await?))
    }

    /// An artist's releases on their own page (`/music`): (title, album page).
    pub async fn releases(&self, artist_page: &str) -> Result<Vec<(String, String)>> {
        let base = page(artist_page).context("Not a page on Bandcamp")?;
        let mut u = reqwest::Url::parse(&base)?;
        u.set_path("/music");
        let html = self.text(u.as_str()).await?;
        Ok(parse_releases(&html, u.as_str()))
    }

}

/// One answer of Bandcamp's search box.
#[derive(Debug, Deserialize)]
struct Found {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    band_name: String,
    #[serde(default)]
    item_url_path: String,
}

/// Bandcamp's search box's answers for albums (its autocomplete, as its own site asks it): (title, artist, album page).
pub fn parse_autocomplete(json: &str) -> Vec<(String, String, String)> {
    #[derive(Deserialize)]
    struct Auto {
        #[serde(default)]
        results: Vec<Found>,
    }
    #[derive(Deserialize)]
    struct Answer {
        auto: Option<Auto>,
    }
    let Ok(a) = serde_json::from_str::<Answer>(json) else { return Vec::new() };
    a.auto
        .map(|x| x.results)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| f.kind == "a")
        .filter_map(|f| Some((f.name, f.band_name, album_page(&f.item_url_path)?)))
        .collect()
}

/// Bandcamp's search box's answers for artists: (name, their page).
pub fn parse_artists(json: &str) -> Vec<(String, String)> {
    #[derive(Deserialize)]
    struct Auto {
        #[serde(default)]
        results: Vec<Band>,
    }
    #[derive(Deserialize)]
    struct Band {
        #[serde(rename = "type", default)]
        kind: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        item_url_path: String,
        #[serde(default)]
        item_url_root: String,
    }
    #[derive(Deserialize)]
    struct Answer {
        auto: Option<Auto>,
    }
    let Ok(a) = serde_json::from_str::<Answer>(json) else { return Vec::new() };
    a.auto
        .map(|x| x.results)
        .unwrap_or_default()
        .into_iter()
        .filter(|b| b.kind == "b")
        .filter_map(|b| Some((b.name, page(&b.item_url_path).or_else(|| page(&b.item_url_root))?)))
        .collect()
}

// ---- The pages

#[derive(Deserialize)]
struct Tralbum {
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    album_release_date: Option<String>,
    #[serde(default)]
    current: Option<Current>,
    #[serde(default)]
    trackinfo: Vec<TrackInfo>,
}

#[derive(Deserialize, Default)]
struct Current {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    about: Option<String>,
    #[serde(default)]
    credits: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
}

#[derive(Deserialize)]
struct TrackInfo {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    track_num: Option<u32>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    file: Option<HashMap<String, Option<String>>>,
    #[serde(default)]
    title_link: Option<String>,
}

/// An album's page: its `data-tralbum`, its cover.
pub fn parse_album(html: &str, url: &str) -> Result<Album> {
    let data = attribute(html, "data-tralbum").context("Bandcamp's page has no album data")?;
    let t: Tralbum = serde_json::from_str(&unescape(&data)).context("Bandcamp's album data was not understood")?;
    let current = t.current.unwrap_or_default();
    let artist = current.artist.or(t.artist).unwrap_or_default();
    let base = reqwest::Url::parse(url).ok();
    let tracks = t
        .trackinfo
        .into_iter()
        .enumerate()
        .map(|(i, x)| {
            let stream = x.file.and_then(|f| f.get("mp3-128").cloned().flatten()).map(|s| if s.starts_with("//") { format!("https:{s}") } else { s });
            Track {
                n: x.track_num.filter(|&n| n > 0).unwrap_or(i as u32 + 1),
                title: x.title.unwrap_or_default(),
                artist: x.artist.filter(|a| !a.is_empty() && *a != artist),
                duration_secs: x.duration.filter(|&d| d > 0.0),
                stream,
                page: x.title_link.and_then(|l| base.as_ref()?.join(&l).ok()).and_then(|u| page(u.as_str())),
            }
        })
        .collect();
    let cover = meta(html, "og:image").map(|c| large(&c));
    // The rest of what the page says of the album (0.9.2, the owner: its panel "as much as our own panel"): the date, its
    // label and tags from its schema.org description, its about and credits from its own data.
    let ld = ld_json(html);
    let released = current
        .release_date
        .as_deref()
        .or(t.album_release_date.as_deref())
        .and_then(iso_date)
        .or_else(|| ld.as_ref().and_then(|l| l["datePublished"].as_str()).and_then(iso_date));
    let label = ld
        .as_ref()
        .and_then(|l| l["publisher"]["name"].as_str())
        .map(str::trim)
        .filter(|p| !p.is_empty() && !p.eq_ignore_ascii_case(&artist))
        .map(str::to_string);
    let mut tags: Vec<String> = Vec::new();
    let words: Vec<String> = match ld.as_ref().map(|l| &l["keywords"]) {
        Some(serde_json::Value::Array(a)) => a.iter().filter_map(|k| k.as_str().map(str::to_string)).collect(),
        Some(serde_json::Value::String(s)) => s.split(',').map(str::to_string).collect(),
        _ => Vec::new(),
    };
    for w in words.iter().map(|w| w.trim()).filter(|w| !w.is_empty()) {
        if !tags.iter().any(|t| t.eq_ignore_ascii_case(w)) {
            tags.push(w.to_string());
        }
    }
    let text = |s: Option<String>| s.map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    Ok(Album {
        url: url.to_string(),
        title: current.title.unwrap_or_default(),
        artist,
        cover,
        tracks,
        released,
        label,
        about: text(current.about),
        credits: text(current.credits),
        tags,
    })
}

/// The page's own description of the album (schema.org's, its ld+json script).
fn ld_json(html: &str) -> Option<serde_json::Value> {
    let at = html.find("application/ld+json")?;
    let start = at + html[at..].find('>')? + 1;
    let end = start + html[start..].find("</script>")?;
    serde_json::from_str(html[start..end].trim()).ok()
}

/// A date as Bandcamp writes it ("17 May 2019 00:00:00 GMT", or already "2019-05-17…") as "2019-05-17".
fn iso_date(s: &str) -> Option<String> {
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let s = s.trim();
    if s.len() >= 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' {
        return Some(s[..10].to_string());
    }
    let mut w = s.split_whitespace();
    let day: u32 = w.next()?.parse().ok()?;
    let month = w.next()?.get(..3)?.to_ascii_lowercase();
    let m = MONTHS.iter().position(|x| *x == month)? + 1;
    let year: u32 = w.next()?.parse().ok()?;
    (1..=31).contains(&day).then(|| format!("{year:04}-{m:02}-{day:02}"))
}

/// A cover's address at Bandcamp's large size (`_10`, 1200 px), from any size's.
fn large(cover: &str) -> String {
    sized(cover, 10)
}

/// A cover's address at one of Bandcamp's sizes (`_10` 1200 px, `_16` 700 px), from any size's.
fn sized(cover: &str, n: u32) -> String {
    match cover.strip_suffix(".jpg").and_then(|c| c.rsplit_once('_')) {
        Some((base, size)) if !size.is_empty() && size.chars().all(|c| c.is_ascii_digit()) => format!("{base}_{n}.jpg"),
        _ => cover.to_string(),
    }
}

/// An artist's `/music` page: each release's title and page, from its grid (and the rest of it, which the page keeps in
/// `data-client-items`).
pub fn parse_releases(html: &str, url: &str) -> Vec<(String, String)> {
    let base = reqwest::Url::parse(url).ok();
    let full = |href: &str| base.as_ref().and_then(|b| b.join(href).ok()).and_then(|u| album_page(u.as_str()));
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |title: String, href: &str| {
        if let Some(u) = full(href)
            && !title.is_empty()
            && !out.iter().any(|(_, x)| *x == u)
        {
            out.push((title, u));
        }
    };
    for item in html.split("<li data-item-id=").skip(1) {
        let item = item.split("</li>").next().unwrap_or(item);
        let (Some(href), Some(title)) = (between(item, "href=\"", "\""), between(item, "class=\"title\">", "<")) else { continue };
        add(text(&title), &href);
    }
    #[derive(Deserialize)]
    struct Client {
        #[serde(default)]
        title: String,
        #[serde(default)]
        page_url: String,
    }
    if let Some(more) = attribute(html, "data-client-items")
        && let Ok(items) = serde_json::from_str::<Vec<Client>>(&unescape(&more))
    {
        for c in items {
            add(c.title, &c.page_url);
        }
    }
    out
}

/// An attribute's value (`name="…"` or `name='…'`), as it is written (still escaped).
fn attribute(html: &str, name: &str) -> Option<String> {
    let at = html.find(&format!("{name}="))? + name.len() + 1;
    let rest = &html[at..];
    let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &rest[1..];
    Some(rest[..rest.find(q)?].to_string())
}

/// A `<meta property="…" content="…">`'s content.
fn meta(html: &str, property: &str) -> Option<String> {
    let at = html.find(&format!("property=\"{property}\""))?;
    let tag_start = html[..at].rfind('<')?;
    let tag = &html[tag_start..tag_start + html[tag_start..].find('>')?];
    between(tag, "content=\"", "\"").map(|c| unescape(&c)).filter(|c| !c.is_empty())
}

fn between(s: &str, from: &str, to: &str) -> Option<String> {
    let a = s.find(from)? + from.len();
    let b = s[a..].find(to)?;
    Some(s[a..a + b].to_string())
}

/// Text in a page: its tags left out, its entities read, its spaces made one.
fn text(html: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => tag = true,
            '>' if tag => {
                tag = false;
                out.push(' ');
            }
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    unescape(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// HTML's entities read: the named few a page escapes with, and numbered ones.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.char_indices().take(12).find(|&(_, c)| c == ';').map(|(i, _)| i) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let c = match name {
            "quot" => Some('"'),
            "amp" => Some('&'),
            "apos" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "nbsp" => Some(' '),
            _ if name.starts_with("#x") || name.starts_with("#X") => u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32),
            _ if name.starts_with('#') => name[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_previews_id_holds_its_album_and_number() {
        let id = track_id("https://air.bandcamp.com/album/moon-safari", 3);
        assert!(is_preview(&id) && !is_preview("tr-1"));
        assert!(id[3..].chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "safe in a path");
        assert_eq!(read_id(&id), Some(("https://air.bandcamp.com/album/moon-safari".into(), 3)));
        assert_eq!(read_id(&track_id("https://evil.example/album/x", 1)), None, "only Bandcamp's own pages");
        assert_eq!(read_id("bc.!!"), None);
    }

    #[test]
    fn only_bandcamps_own_pages_are_read() {
        assert_eq!(album_page("http://air.bandcamp.com/album/moon-safari?from=search#x").as_deref(), Some("https://air.bandcamp.com/album/moon-safari"));
        assert!(album_page("https://air.bandcamp.com/music").is_none(), "not an album");
        assert!(album_page("https://bandcamp.com.evil.example/album/x").is_none());
        assert!(album_page("https://music.airband.example/album/x").is_none(), "an artist's own domain is left to its link");
        assert!(page("https://air.bandcamp.com").is_some());
    }

    #[test]
    fn an_albums_page_gives_its_tracks_and_their_streams() {
        let data = r#"{"artist":"Air","current":{"title":"Moon Safari","artist":"Air"},"trackinfo":[
            {"title":"La Femme d'Argent","track_num":1,"duration":428.5,"file":{"mp3-128":"https://t4.bcbits.com/stream/a/mp3-128/1?p=0&t=x"},"title_link":"/track/la-femme"},
            {"title":"Sexy Boy","track_num":2,"duration":298.0,"file":null,"title_link":"/track/sexy-boy"},
            {"title":"Kelly","artist":"Guest","track_num":3,"duration":0,"file":{"mp3-128":"//t4.bcbits.com/stream/c"}}]}"#;
        let esc = data.replace('&', "&amp;").replace('"', "&quot;");
        let html = format!(r#"<html><head><meta property="og:image" content="https://f4.bcbits.com/img/a1_10.jpg"></head><body><script data-tralbum="{esc}"></script></body></html>"#);
        let a = parse_album(&html, "https://air.bandcamp.com/album/moon-safari").unwrap();
        assert_eq!((a.title.as_str(), a.artist.as_str()), ("Moon Safari", "Air"));
        assert_eq!(a.cover.as_deref(), Some("https://f4.bcbits.com/img/a1_10.jpg"));
        assert_eq!(a.tracks.len(), 3);
        assert_eq!(a.tracks[0].stream.as_deref(), Some("https://t4.bcbits.com/stream/a/mp3-128/1?p=0&t=x"));
        assert_eq!(a.tracks[0].duration_secs, Some(428.5));
        assert_eq!(a.tracks[1].stream, None);
        assert_eq!(a.tracks[1].page.as_deref(), Some("https://air.bandcamp.com/track/sexy-boy"));
        assert_eq!(a.tracks[2].stream.as_deref(), Some("https://t4.bcbits.com/stream/c"), "a page's own scheme");
        assert_eq!(a.tracks[2].artist.as_deref(), Some("Guest"));
        assert_eq!(a.tracks[2].duration_secs, None);
        assert!(parse_album("<html></html>", "https://air.bandcamp.com/album/x").is_err());
        assert_eq!((a.released, a.label, a.about, a.tags.len()), (None, None, None, 0), "a page that says no more");
    }

    #[test]
    fn an_albums_page_says_when_it_came_out_its_label_its_tags_and_about() {
        let data = r#"{"artist":"Air","album_release_date":"01 Jan 1998 00:00:00 GMT","current":{"title":"Moon Safari","release_date":"16 Jan 1998 00:00:00 GMT","about":"  Their first album.  ","credits":"Produced by Air"},"trackinfo":[]}"#;
        let esc = data.replace('"', "&quot;");
        let ld = r#"{"@type":"MusicAlbum","publisher":{"name":"Source"},"keywords":["electronic","Electronic","downtempo"," Paris "]}"#;
        let html = format!(r#"<meta property="og:image" content="https://f4.bcbits.com/img/a1_16.jpg"><script type="application/ld+json">{ld}</script><script data-tralbum="{esc}"></script>"#);
        let a = parse_album(&html, "https://air.bandcamp.com/album/moon-safari").unwrap();
        assert_eq!(a.released.as_deref(), Some("1998-01-16"), "the album's own date first");
        assert_eq!(a.label.as_deref(), Some("Source"));
        assert_eq!(a.about.as_deref(), Some("Their first album."));
        assert_eq!(a.credits.as_deref(), Some("Produced by Air"));
        assert_eq!(a.tags, ["electronic", "downtempo", "Paris"], "each once, trimmed");
        assert_eq!(a.cover.as_deref(), Some("https://f4.bcbits.com/img/a1_10.jpg"), "the large size");
        assert_eq!(iso_date("2019-05-17T00:00:00Z").as_deref(), Some("2019-05-17"));
        assert_eq!(iso_date("5 Sept 2003 00:00:00 GMT").as_deref(), Some("2003-09-05"));
        assert_eq!(iso_date("someday"), None);
        let own = format!(r#"<script type="application/ld+json">{{"publisher":{{"name":"Air"}}}}</script><script data-tralbum="{esc}"></script>"#);
        assert_eq!(parse_album(&own, "https://air.bandcamp.com/album/x").unwrap().label, None, "the artist's own page is no label");
    }

    #[test]
    fn an_artists_releases_are_read() {
        let more = r#"[{"title":"Talkie Walkie","page_url":"/album/talkie-walkie"}]"#.replace('"', "&quot;");
        let html = format!(
            r#"<ol id="music-grid" data-client-items="{more}"><li data-item-id="album-1" class="music-grid-item"><a href="/album/moon-safari"><div class="art"></div><p class="title">
                Moon Safari &amp; More
            </p></a></li><li data-item-id="track-2"><a href="/track/kelly"><p class="title">Kelly</p></a></li></ol>"#
        );
        let r = parse_releases(&html, "https://air.bandcamp.com/music");
        assert_eq!(
            r,
            vec![
                ("Moon Safari & More".to_string(), "https://air.bandcamp.com/album/moon-safari".to_string()),
                ("Kelly".to_string(), "https://air.bandcamp.com/track/kelly".to_string()),
                ("Talkie Walkie".to_string(), "https://air.bandcamp.com/album/talkie-walkie".to_string()),
            ]
        );
    }

    #[test]
    fn the_search_boxs_albums_are_read() {
        let json = r#"{"auto":{"results":[
            {"type":"a","name":"Dive","band_name":"Tycho","item_url_path":"https://tycho.bandcamp.com/album/dive"},
            {"type":"a","name":"Dive (Mashed Up by wolves.)","band_name":"Tycho","item_url_path":"https://wolvesperiod.bandcamp.com/album/dive-mashed-up-by-wolves"},
            {"type":"b","name":"Tycho","band_name":"Tycho","item_url_path":"https://tycho.bandcamp.com"},
            {"type":"a","name":"Elsewhere","band_name":"X","item_url_path":"https://elsewhere.example/album/x"}]},"tag":{},"genre":{}}"#;
        let r = parse_autocomplete(json);
        assert_eq!(r.len(), 2, "albums on Bandcamp only");
        assert_eq!(r[0], ("Dive".into(), "Tycho".into(), "https://tycho.bandcamp.com/album/dive".into()));
        assert!(parse_autocomplete("<html>challenge</html>").is_empty());
    }

    #[test]
    fn the_search_box_finds_artists_by_their_own_page() {
        let json = r#"{"auto":{"results":[
            {"type":"a","name":"Dive","band_name":"Tycho","item_url_path":"https://tycho.bandcamp.com/album/dive"},
            {"type":"b","name":"Tycho","band_name":"Tycho","item_url_path":"https://tycho.bandcamp.com"},
            {"type":"b","name":"Elsewhere","item_url_path":"https://elsewhere.example"}]}}"#;
        let found = parse_artists(json);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "Tycho");
        assert!(found[0].1.starts_with("https://tycho.bandcamp.com"));
    }

    #[test]
    fn entities_are_read_and_a_stray_ampersand_stays() {
        assert_eq!(unescape("a &amp; b &quot;c&quot; &#39;d&#x27; R&B &unknown; x"), "a & b \"c\" 'd' R&B &unknown; x");
        assert_eq!(unescape("Björk &ééééééééééé; Sigur Rós"), "Björk &ééééééééééé; Sigur Rós", "never cut inside a letter");
    }
}
