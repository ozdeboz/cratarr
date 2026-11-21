//! Bandcamp, read to play from (decision 0071): an album's page, its tracks and each one's stream; an artist's list of
//! releases; the search box's own answers (Bandcamp's autocomplete, JSON). Played, never downloaded: nothing is kept on
//! disk. Bandcamp's search page is not read: it answers with a check for browsers (a "Client Challenge"), which Cratarr
//! does not get around. Nor is an account's `/music` page (O4): one polite request met the check; an account's albums are the
//! search box's. After the check, or a search answer that is no JSON, Bandcamp is left alone for a day (`held_until`), said once in
//! the log and on Health; switched off (`switches::BANDCAMP`), it is asked nothing. An album is read from its page alone; a track's
//! own page only as the track is played.
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
/// The most of a page of Bandcamp's read (O1), and of a search box's answer.
const PAGE_MOST: usize = 8 << 20;
const SEARCH_MOST: usize = 1 << 20;
/// Bandcamp's one turn (O4, TN): at most one page of Bandcamp's read a second, server-wide (Discover's look, previews, a track's
/// details: the owner asked, 2026-10-25, whether Cratarr is polite to it); none while it asked to be left alone, after "too many"
/// (429) or "unavailable" (503), or for a day after its bot check (its own hold, `hold_until`). A stream played is not paced: it is a
/// listener's.
pub static TURN: crate::app::turn::Turn = crate::app::turn::Turn::new(crate::app::turn::BANDCAMP);
/// How long Bandcamp is left alone after it says "too many" (429) or "unavailable" (503) without saying how long.
const BACK_OFF: Duration = crate::app::turn::BANDCAMP.first;
/// How long Bandcamp is left alone after its bot check (O4): a day, nothing done to get past it.
pub const CHECK_HOLD_SECS: i64 = crate::app::turn::BANDCAMP.longest.as_secs() as i64;

/// A Bandcamp album page as a view offers it for its preview (G1): none while Bandcamp is switched off, as its preview would only
/// fail. Every view that carries one goes through here: a record not had's page, the missing lists and search's rows from them,
/// Discover's cards and related artists.
pub fn shown(page: Option<String>) -> Option<String> {
    shown_if(page, crate::app::switches::is_on(crate::app::switches::BANDCAMP))
}

/// `shown`, Bandcamp switched `on` or not. Pure.
fn shown_if(page: Option<String>, on: bool) -> Option<String> {
    page.filter(|_| on)
}

/// Until when Cratarr leaves Bandcamp alone after its bot check (O4), where it does at `now`: its turn's own hold. Never longer than
/// the check's own day from now (U11, as V8 has it for the kept one): a hold set while the clock ran ahead is cut to that, and held so.
pub fn held_until(now: i64) -> Option<i64> {
    TURN.own_until(now)
}

/// Bandcamp left alone for a day after its bot check, or a search answer that is no JSON (O4): said in the log once, a failure for
/// Health's connections, nothing done to get past it. Kept where its turn is, the server's user data (Y8, OQ-T3).
pub fn hold() {
    hold_in(None);
}

/// `hold`, its turn holding every reader for the day, kept in `ud` where given (else where the turn is kept: Y8), so a restart the
/// evening of a check still leaves Bandcamp alone until the day is out (`Turn::keep`).
pub fn hold_in(ud: Option<&crate::journal::Journal>) {
    let now = crate::app::moment::now_secs();
    if held_until(now).is_none() {
        tracing::warn!(target: "bandcamp", "Bandcamp asked for a browser check: Cratarr stopped asking it for a day");
    }
    TURN.hold_until(now + CHECK_HOLD_SECS, ud);
    crate::app::reach::failed("bandcamp", "Bandcamp asked for a browser check".into());
}

/// Tests: the hold kept before a restart taken up again, as the start does (`Turn::keep`, Y8): nothing is asked of Bandcamp until it
/// is out, and Health says so. Never longer than the check's own day from now (V8): one kept while the clock ran ahead is cut to that,
/// in the user data too, so the next start does not take it up again.
#[cfg(any(test, feature = "test-support"))]
pub fn hold_kept(ud: &crate::journal::Journal) {
    TURN.take_up(ud);
}

/// Bandcamp switched on or off by an administrator (O4): switched on, its bot check's hold is lifted (V8) through its turn, in memory
/// and in the user data `ud` where there is one, so it is asked again at once and a restart takes up nothing. Switched off, the hold
/// stays.
pub fn switched(ud: Option<&crate::journal::Journal>, on: bool) {
    if on {
        TURN.lift(ud);
    }
}

/// Tests: Bandcamp held back after its bot check until `until` (0: not), as its own tests hold it (its turn's own hold, nothing
/// kept); until when it was held so (0: it was not).
#[cfg(any(test, feature = "test-support"))]
pub fn hold_for_tests(until: i64) -> i64 {
    let was = held_until(crate::app::moment::now_secs()).unwrap_or(0);
    TURN.lift(None);
    TURN.hold_until(until, None);
    was
}

/// Why nothing is asked of Bandcamp now (O4): switched off (`SwitchedOff`: no pause, TN.12), or left alone after its bot check
/// (`NotRead`).
fn refused() -> Result<()> {
    if !crate::app::switches::is_on(crate::app::switches::BANDCAMP) {
        return Err(crate::app::references_turn::SwitchedOff("Bandcamp").into());
    }
    if held_until(crate::app::moment::now_secs()).is_some() {
        return Err(NotRead.into());
    }
    Ok(())
}

/// The source a preview's tracks carry, its preview ids, pages and release names: in `bandcamp_ids`, below the stores.
pub use crate::bandcamp_ids::{NOT_KEPT, Named, SOURCE, album_page, is_preview, label_release, page, read_id};
use crate::bandcamp_ids::ID;
/// Bandcamp's MP3 streams are 128 kb/s throughout: a moment's place in the file, for seeking.
pub const BYTES_PER_SEC: u64 = 128_000 / 8;

/// A track's id in a queue: its album's page and its number (0: the album itself, for its cover), so a queue saved or
/// sent to another player still plays it.
pub fn track_id(album: &str, n: u32) -> String {
    use base64::Engine as _;
    format!("{ID}{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{n}|{album}")))
}

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
    /// The account whose page it is (its `og:site_name`): a label's, where a label's own account lists its artists' releases (W1,
    /// `named_by_page`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// What the page says about it, and its credits, as written there.
    pub about: Option<String>,
    pub credits: Option<String>,
    /// Bandcamp's tags for it, as the page lists them.
    pub tags: Vec<String>,
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

impl Track {
    /// Whether it plays: a stream on the album's page, or its own page, read as it is played (O4).
    pub fn plays(&self) -> bool {
        self.stream.is_some() || self.page.is_some()
    }
}

/// Tests: the stream addresses of fake media servers at home (`fake_media_at`), reached plainly.
#[cfg(any(test, feature = "test-support"))]
static FAKE_MEDIA: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Tests: a fake media server's stream address, reached plainly as if it were on Bandcamp's media hosts.
#[cfg(any(test, feature = "test-support"))]
pub fn fake_media_at(stream: &str) {
    FAKE_MEDIA.lock().unwrap_or_else(|p| p.into_inner()).push(stream.to_string());
}

/// Tests: every page the client went to read on Bandcamp, in order (a test's fake pages are `keep`'s, read without a request).
#[cfg(any(test, feature = "test-support"))]
static PAGES_ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Tests: the pages the client went to read on Bandcamp so far.
#[cfg(any(test, feature = "test-support"))]
pub fn pages_asked_for_tests() -> Vec<String> {
    PAGES_ASKED.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Reads Bandcamp: one client, the album pages kept a few minutes.
pub struct Bandcamp {
    http: reqwest::Client,
    /// Its streams' client: Bandcamp's media hosts only (O1).
    media: reqwest::Client,
    kept: Mutex<HashMap<String, (Instant, Album)>>,
}

impl Bandcamp {
    pub fn new() -> Result<Self> {
        use crate::app::reach::{Purpose, client};
        // Its pages on Bandcamp's own hosts, its streams on its media hosts (O1): a redirect off them is not followed.
        let http = client(Purpose::BandcampPages).timeout(Duration::from_secs(20)).build()?;
        let media = client(Purpose::BandcampMedia).build()?;
        Ok(Self { http, media, kept: Mutex::new(HashMap::new()) })
    }

    /// The one reader, for the server's life.
    pub fn get() -> &'static Bandcamp {
        static ONE: std::sync::OnceLock<Bandcamp> = std::sync::OnceLock::new();
        ONE.get_or_init(|| Bandcamp::new().expect("an HTTP client"))
    }

    /// A request of Bandcamp's (a page, a search), in its turn (`TURN`): a second after the one before, none while it asked to be left
    /// alone. A back-off is waited out up to a page's patience, else told until when (OQ-T1 (a)).
    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        // Switched off, or left alone after its bot check (O4): nothing asked.
        refused()?;
        let taken = TURN.take_within(false, crate::app::musicbrainz::PATIENCE).await;
        // Looked at again in its turn (Y8): a read that waited in line, or out a back-off, while the check came is not sent after it.
        refused()?;
        taken?;
        let resp = req.sent().await.context("cannot reach Bandcamp")?;
        if let Err(p) = TURN.heard_answer(resp.status(), resp.headers()) {
            tracing::warn!("Bandcamp answered {}: left alone until {}", resp.status(), p.until);
        }
        Ok(resp)
    }

    async fn text(&self, url: &str) -> Result<String> {
        #[cfg(any(test, feature = "test-support"))]
        PAGES_ASKED.lock().unwrap_or_else(|p| p.into_inner()).push(url.to_string());
        let resp = self.send(self.http.get(url)).await?;
        let (status, headers) = (resp.status(), resp.headers().clone());
        let body = String::from_utf8_lossy(&crate::app::reach::read_capped(resp, PAGE_MOST).await?).into_owned();
        // Its bot check in place of the page, whatever the status: not read, nothing is done to get past it, and Bandcamp is left
        // alone for a day (O4).
        if is_check(&body) {
            hold();
            return Err(NotRead.into());
        }
        if !status.is_success() {
            return Err(refusal(status, &headers, crate::app::moment::now_secs()));
        }
        Ok(body)
    }

    /// An album's page read (kept a few minutes), its page alone (O4): a track without a stream on it is read from its own page only
    /// as it is played (`stream`).
    pub async fn album(&self, url: &str) -> Result<Album> {
        let url = album_page(url).context("Not an album's page on Bandcamp")?;
        if let Some((at, a)) = self.kept.lock().unwrap_or_else(|p| p.into_inner()).get(&url)
            && at.elapsed() < KEEP
        {
            return Ok(a.clone());
        }
        let album = parse_album(&self.text(&url).await?, &url)?;
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
        // Switched off, its streams stop too, as the switch says (Y12): an album kept in memory's as well.
        if !crate::app::switches::is_on(crate::app::switches::BANDCAMP) {
            return Err(crate::app::references_turn::SwitchedOff("Bandcamp").into());
        }
        let a = self.album(url).await?;
        let t = a.tracks.iter().find(|t| t.n == n).context("No such track on that album")?;
        // A track the album's page gives no stream for: its own page read now, as it is played (O4).
        let s = match (&t.stream, &t.page) {
            (Some(s), _) => s.clone(),
            (None, Some(p)) => self.track_stream(&a.url, n, p).await?.context("That track does not stream on Bandcamp")?,
            (None, None) => bail!("That track does not stream on Bandcamp"),
        };
        let media = self.media_for(&s).context("That track's stream is not on Bandcamp's own servers")?;
        let mut req = media.get(&s).timeout(Duration::from_secs(60 * 60));
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

    /// The client a stream is fetched with: Bandcamp's own media hosts only (O1), as a page's data could name any address and a stream
    /// goes back to the listener whole, or a first hop on Bandcamp's own stream hosts, followed on to its media hosts only (Y10,
    /// `Purpose::starts_at`); none for any other address. Tests: a fake media server at home (`fake_media_at`), plainly.
    fn media_for(&self, stream: &str) -> Option<reqwest::Client> {
        #[cfg(any(test, feature = "test-support"))]
        if FAKE_MEDIA.lock().unwrap_or_else(|p| p.into_inner()).iter().any(|f| f == stream) {
            return Some(reqwest::Client::new());
        }
        reqwest::Url::parse(stream).is_ok_and(|u| crate::app::reach::Purpose::BandcampMedia.starts_at(&u)).then(|| self.media.clone())
    }

    /// A track's stream from its own page (O4: read as it is played), kept on its album while the album is kept.
    async fn track_stream(&self, album: &str, n: u32, page: &str) -> Result<Option<String>> {
        let stream = parse_album(&self.text(page).await?, page)?.tracks.into_iter().next().and_then(|t| t.stream);
        if let Some(s) = &stream
            && let Some((_, a)) = self.kept.lock().unwrap_or_else(|p| p.into_inner()).get_mut(album)
            && let Some(t) = a.tracks.iter_mut().find(|t| t.n == n)
        {
            t.stream = Some(s.clone());
        }
        Ok(stream)
    }

    /// An album taken as read, as if its page had just been read (a fake Bandcamp's, in tests).
    #[cfg(any(test, feature = "test-support"))]
    pub fn keep(&self, album: Album) {
        let url = album_page(&album.url).unwrap_or_else(|| album.url.clone());
        self.kept.lock().unwrap_or_else(|p| p.into_inner()).insert(url, (Instant::now(), album));
    }

    /// Bandcamp's search box asked for albums: (title, artist, album page), its first answers.
    pub async fn search(&self, artist: &str, album: &str) -> Result<Vec<(String, String, String)>> {
        let body = serde_json::json!({ "search_text": format!("{artist} {album}").trim(), "search_filter": "a", "full_page": false, "fan_id": null });
        Ok(parse_autocomplete(&self.answered(self.http.post("https://bandcamp.com/api/bcsearch_public_api/1/autocomplete_elastic").json(&body)).await?))
    }

    /// Bandcamp's search box asked for artists: (name, their page), its first answers.
    pub async fn search_artist(&self, name: &str) -> Result<Vec<(String, String)>> {
        let body = serde_json::json!({ "search_text": name, "search_filter": "b", "full_page": false, "fan_id": null });
        Ok(parse_artists(&self.answered(self.http.post("https://bandcamp.com/api/bcsearch_public_api/1/autocomplete_elastic").json(&body)).await?))
    }

    /// The search box's answer: JSON, else its bot check or something in its place (O4), which is not read and leaves Bandcamp
    /// alone for a day, never "nothing found".
    async fn answered(&self, req: reqwest::RequestBuilder) -> Result<String> {
        let resp = self.send(req).await?;
        if !resp.status().is_success() {
            return Err(refusal(resp.status(), resp.headers(), crate::app::moment::now_secs()));
        }
        let body = String::from_utf8_lossy(&crate::app::reach::read_capped(resp, SEARCH_MOST).await?).into_owned();
        if is_check(&body) || serde_json::from_str::<serde_json::Value>(&body).is_err() {
            hold();
            return Err(NotRead.into());
        }
        Ok(body)
    }

    /// An account's albums as Bandcamp's search box lists them (O4: its `/music` page is not read): its name asked for, the albums
    /// whose page is on the account's own address; (title, album page). The search box gives its first answers only.
    pub async fn albums_on(&self, account_page: &str, name: &str) -> Result<Vec<(String, String)>> {
        let host = |u: &str| reqwest::Url::parse(u).ok().and_then(|u| u.host_str().map(str::to_ascii_lowercase));
        let own = host(&page(account_page).context("Not a page on Bandcamp")?);
        Ok(self.search(name, "").await?.into_iter().filter(|(_, _, album)| own.is_some() && host(album) == own).map(|(title, _, album)| (title, album)).collect())
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
        account: meta(html, "og:site_name").map(|a| a.trim().to_string()).filter(|a| !a.is_empty()),
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

/// The names a Want with a Bandcamp page names its record by (W1): the album as Bandcamp lists it, its own artist (not a track's,
/// which a track's menu sends), a label's release sorted out with the page's account as the label (`label_release`), its publisher
/// credited where that names none. The page not read (Bandcamp's bot check among the reasons) leaves the names sent, sorted out as
/// names alone are: nothing is done to get past the check. Pure.
pub fn named_by_page(read: Option<&Album>, artist: &str, title: &str) -> Named {
    match read.filter(|a| !a.artist.trim().is_empty() && !a.title.trim().is_empty()) {
        Some(a) => {
            let mut named = label_release(&a.artist, &a.title, a.account.as_deref());
            if named.label.is_none() {
                named.label = a.label.clone();
            }
            named
        }
        None => label_release(artist, title, None),
    }
}

/// How long Bandcamp is left alone after its 429 or 503: its Retry-After where it says (a quarter hour at most), else `BACK_OFF`.
fn back_off(headers: &reqwest::header::HeaderMap, now: i64) -> Duration {
    crate::app::turn::retry_after(headers, now).unwrap_or(BACK_OFF)
}

/// What Bandcamp's answer of `status` comes to (TN.1): its 429 and 503 the pause they ask for, until its back-off is out; any other
/// refusal by its status, in its words ("Bandcamp answered 404 Not Found"). Pure.
pub fn refusal(status: reqwest::StatusCode, headers: &reqwest::header::HeaderMap, now: i64) -> anyhow::Error {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
        let until = now + back_off(headers, now).as_secs() as i64;
        return crate::app::turn::Paused { source: crate::app::turn::BANDCAMP.name, until }.into();
    }
    crate::app::turn::Refused::new(crate::app::turn::BANDCAMP.name, status, format!("Bandcamp answered {status}")).into()
}

/// Bandcamp answered with its bot check (a "Client Challenge" page) in place of the page asked for: the page is not read, and
/// says so. Nothing is done to get past it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotRead;

impl std::fmt::Display for NotRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("not read: Bandcamp answered with its bot check")
    }
}

impl std::error::Error for NotRead {}

/// Whether a read failed for Bandcamp's bot check (`NotRead`), to show as "not read".
#[cfg_attr(not(test), allow(dead_code))] // a label's page (C56)
pub fn not_read(e: &anyhow::Error) -> bool {
    e.is::<NotRead>()
}

/// Whether a page Bandcamp sent is its bot check, not the page asked for: its title, or the check's own assets, near its top.
/// Pure.
pub fn is_check(html: &str) -> bool {
    ["<title>Client Challenge</title>", "/_fs-ch-"].iter().any(|mark| html.find(mark).is_some_and(|at| at < 4096))
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

    /// Y11: a Bandcamp page written with a port is made plain without it: its own page at Bandcamp's usual address.
    #[test]
    fn a_bandcamp_page_loses_its_port() {
        assert_eq!(album_page("https://air.bandcamp.com:8443/album/moon-safari").as_deref(), Some("https://air.bandcamp.com/album/moon-safari"));
        assert_eq!(page("http://air.bandcamp.com:8080/").as_deref(), Some("https://air.bandcamp.com/"));
        assert_eq!(page("http://air.bandcamp.com:443/music").as_deref(), Some("https://air.bandcamp.com/music"));
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

    /// S12: a release named the way a label's own account names its artists' is sorted out by one rule. The owner's case and
    /// its counter-cases: a real artist whose title has a dash, a closing "(Remastered)" that names no label, a compilation
    /// of the label's own.
    #[test]
    fn a_labels_release_is_named_by_one_rule() {
        let named = |artist: &str, title: &str, label: Option<&str>| {
            let n = label_release(artist, title, label);
            (n.artist, n.title, n.label)
        };
        let up = Some("Up Records".to_string());
        assert_eq!(named("Up Records", "GROUCH - further (Up Records)", None), ("GROUCH".into(), "further".into(), up.clone()), "the label credits itself");
        assert_eq!(named("Up Records", "GROUCH - further (Up Records)", Some("Up Records")), ("GROUCH".into(), "further".into(), up.clone()));
        assert_eq!(named("Up Records", "GROUCH \u{2013} further", Some("up records")), ("GROUCH".into(), "further".into(), up.clone()), "the account known as the label");
        assert_eq!(named("Kite Theory", "Night Walks - Part One", Some("Up Records")), ("Kite Theory".into(), "Night Walks - Part One".into(), None), "a real artist's dash");
        assert_eq!(named("Kite Theory", "Night Walks - Part One", None), ("Kite Theory".into(), "Night Walks - Part One".into(), None));
        assert_eq!(named("Up Records", "GROUCH - further (Remastered)", Some("Up Records")), ("GROUCH".into(), "further (Remastered)".into(), up.clone()), "brackets that name no label stay");
        assert_eq!(named("Up Records", "GROUCH - further (Remastered)", None), ("Up Records".into(), "GROUCH - further (Remastered)".into(), None), "nothing says it is a label's");
        assert_eq!(named("Up Records", "Summer Sampler 2019", Some("Up Records")), ("Up Records".into(), "Summer Sampler 2019".into(), up.clone()), "the label's own compilation");
        assert_eq!(named("Up Records", "Summer Sampler (Up Records)", Some("Up Records")), ("Up Records".into(), "Summer Sampler".into(), up), "its compilation, the label known");
    }

    /// R18 (F10): a title closing on its own artist's name in brackets is no label's release unless it is "Artist - Title": a
    /// band's "Live (Orbit Lane)" stays as named, and so does a compilation crediting itself with no label known; the owner's
    /// case is still sorted out.
    #[test]
    fn a_title_closing_on_its_own_artist_is_no_labels_release() {
        let named = |artist: &str, title: &str| {
            let n = label_release(artist, title, None);
            (n.artist, n.title, n.label)
        };
        assert_eq!(named("Orbit Lane", "Live (Orbit Lane)"), ("Orbit Lane".into(), "Live (Orbit Lane)".into(), None));
        assert_eq!(named("ORBIT LANE", "Live - Encore (Orbit Lane)"), ("Live".into(), "Encore".into(), Some("ORBIT LANE".to_string())), "an \"Artist - Title\" closing on it");
        assert_eq!(named("Up Records", "Summer Sampler (Up Records)"), ("Up Records".into(), "Summer Sampler (Up Records)".into(), None));
        assert_eq!(named("Up Records", "GROUCH - further (Up Records)"), ("GROUCH".into(), "further".into(), Some("Up Records".to_string())));
    }

    /// S12: Bandcamp's bot check in place of a page (a stand-in written here, not a captured one) is not read: told apart from
    /// a page, and said as such; a page that merely mentions a challenge further down is a page.
    /// O4: Bandcamp switched off is asked nothing: a search, an album not kept, a stream. Its switch and its hold, which every read of
    /// Bandcamp's checks, are changed under the one test lock (`switches::TEST_STATE`, Y12).
    #[tokio::test]
    async fn bandcamp_switched_off_is_asked_nothing() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let was = crate::app::switches::set_for_tests(crate::app::switches::BANDCAMP, false);
        let bc = Bandcamp::new().unwrap();
        let said = bc.search("Glass Harbour", "Two Rooms").await.unwrap_err().to_string();
        crate::app::switches::set_for_tests(crate::app::switches::BANDCAMP, was);
        assert_eq!(said, "Bandcamp is switched off");
    }

    /// Y12: Bandcamp switched off, its streams stop too, as the switch says: an album kept in memory plays nothing.
    #[tokio::test]
    async fn with_bandcamp_off_its_streams_stop() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let (page, stream) = ("https://glassharbour.bandcamp.com/album/low-tide", "http://127.0.0.1:9/low-tide/1");
        fake_media_at(stream);
        let bc = Bandcamp::new().unwrap();
        let track = Track { n: 1, title: "Ebb".into(), artist: None, duration_secs: None, stream: Some(stream.into()), page: None };
        bc.keep(Album { url: page.into(), title: "Low Tide".into(), artist: "Glass Harbour".into(), cover: None, tracks: vec![track], released: None, label: None, account: None, about: None, credits: None, tags: Vec::new() });
        let was = crate::app::switches::set_for_tests(crate::app::switches::BANDCAMP, false);
        let played = bc.stream(page, 1, None).await.map(|_| ());
        crate::app::switches::set_for_tests(crate::app::switches::BANDCAMP, was);
        assert_eq!(played.unwrap_err().to_string(), "Bandcamp is switched off");
    }

    /// Y8: the day's hold after the check survives a restart: kept in its row as it is set (RN's R4), taken up again at start.
    #[tokio::test]
    async fn the_bandcamp_hold_survives_a_restart() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let was = TURN.as_held();
        let ud = crate::journal::Journal::open(None).unwrap();
        let now = crate::app::moment::now_secs();
        hold_in(Some(&ud));
        TURN.lift(None);
        hold_kept(&ud);
        let until = held_until(now);
        TURN.put_back(was);
        assert!(until.is_some_and(|u| u >= now + CHECK_HOLD_SECS), "taken up again at start: {until:?}");
    }

    /// V8 (the review of run 8b, finding 8): a kept hold is taken up for a day from now at most: one kept while the clock ran a month
    /// ahead is cut to a day, in its row too.
    #[tokio::test]
    async fn a_kept_hold_beyond_a_day_is_cut_to_a_day() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let was = TURN.as_held();
        let ud = crate::journal::Journal::open(None).unwrap();
        let now = crate::app::moment::now_secs();
        ud.set_pause(&crate::intake::Pause { source: "bandcamp".into(), until: now + 30 * 86_400, why: None, own: true }).unwrap();
        TURN.lift(None);
        hold_kept(&ud);
        let until = held_until(now);
        TURN.put_back(was);
        let kept = ud.pause("bandcamp").unwrap().map(|p| p.until);
        let a_day = now + CHECK_HOLD_SECS..=now + CHECK_HOLD_SECS + 60;
        assert!(until.is_some_and(|u| a_day.contains(&u)), "a day from now: {until:?}");
        assert!(kept.is_some_and(|k| a_day.contains(&k)), "kept cut too: {kept:?}");
    }

    /// U11 (the review of 8c/8d's nits): a hold set in memory while the clock ran ahead is cut to a day from now as it is read, and
    /// held so: a day on, it is over.
    #[tokio::test]
    async fn a_hold_set_while_the_clock_ran_ahead_is_a_day_at_most() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let was = TURN.as_held();
        let now = crate::app::moment::now_secs();
        TURN.lift(None);
        TURN.hold_until(now + 200 * 86_400, None);
        let until = held_until(now);
        let after = held_until(now + CHECK_HOLD_SECS + 1);
        TURN.put_back(was);
        assert_eq!(until, Some(now + CHECK_HOLD_SECS), "a day from now");
        assert_eq!(after, None, "a day on: over");
    }

    /// G1: a view offers Bandcamp's preview only while Bandcamp is switched on.
    #[test]
    fn a_view_offers_no_preview_while_bandcamp_is_off() {
        let page = Some("https://glassharbour.bandcamp.com/album/two-rooms".to_string());
        assert_eq!(shown_if(page.clone(), true), page);
        assert_eq!(shown_if(page, false), None);
        assert_eq!(shown_if(None, true), None);
    }

    /// V8: an administrator switching Bandcamp on again lifts its hold, in memory and in its row: it is asked again at once, and a
    /// restart takes up nothing. Switched off, the hold stays.
    #[tokio::test]
    async fn switching_bandcamp_on_again_lifts_its_hold() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let was = TURN.as_held();
        let ud = crate::journal::Journal::open(None).unwrap();
        let now = crate::app::moment::now_secs();
        hold_in(Some(&ud));
        switched(Some(&ud), false);
        let off = held_until(now);
        switched(Some(&ud), true);
        let on = held_until(now);
        hold_kept(&ud);
        let restarted = held_until(now);
        TURN.put_back(was);
        assert!(off.is_some(), "switched off: held still");
        assert_eq!((on, restarted), (None, None), "switched on: asked again, and a restart takes up nothing");
    }

    /// Y8: a read waiting in line when the check comes is not sent: it looks at the hold again in its turn.
    #[tokio::test]
    async fn a_read_waiting_when_the_check_comes_is_not_sent() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let was = TURN.as_held();
        let bc = Bandcamp::new().unwrap();
        // In line behind a read just taken: the next goes a second on.
        TURN.turn().await;
        let waiting = bc.send(reqwest::Client::new().get("http://127.0.0.1:9/"));
        let check = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            hold_in(None);
        };
        let (sent, ()) = tokio::join!(waiting, check);
        TURN.put_back(was);
        assert!(sent.is_err_and(|e| not_read(&e)), "not sent after the check");
    }

    /// O1: a track whose stream the page names anywhere but Bandcamp's media hosts is not fetched: an address at home, plain http,
    /// another site.
    #[tokio::test]
    async fn a_stream_address_outside_bcbits_is_refused() {
        // A stream reads Bandcamp's switch (Y12).
        let _one = crate::app::switches::TEST_STATE.write().await;
        let bc = Bandcamp::new().unwrap();
        let page = "https://glassharbour.bandcamp.com/album/tide-tables";
        let track = |n: u32, stream: &str| Track { n, title: format!("Tide {n}"), artist: None, duration_secs: None, stream: Some(stream.into()), page: None };
        let tracks = vec![track(1, "http://192.168.1.20/admin"), track(2, "https://lidarr.example.org/initialize.json"), track(3, "http://t4.bcbits.com/stream/3")];
        bc.keep(Album { url: page.into(), title: "Tide Tables".into(), artist: "Glass Harbour".into(), cover: None, tracks, released: None, label: None, account: None, about: None, credits: None, tags: Vec::new() });
        for n in 1..=3 {
            let said = bc.stream(page, n, None).await.unwrap_err().to_string();
            assert_eq!(said, "That track's stream is not on Bandcamp's own servers", "track {n}");
        }
    }

    #[test]
    fn a_check_page_is_not_read() {
        let check = r#"<!DOCTYPE html><html lang="en"><head><link href="/_fs-ch-stand-in/assets/styles.css" rel="stylesheet" /><title>Client Challenge</title></head>
            <body><noscript>JavaScript is needed to go on.</noscript><div id="loading-error">There was a problem.</div><script src="/_fs-ch-stand-in/script.js"></script></body></html>"#;
        assert!(is_check(check));
        let page = format!(r#"<html><head><meta property="og:site_name" content="Up Records"></head><body>{}<p>Client Challenge EP</p></body></html>"#, "<div></div>".repeat(500));
        assert!(!is_check(&page));
        let e = anyhow::Error::from(NotRead);
        assert!(not_read(&e) && !not_read(&anyhow::anyhow!("Bandcamp answered 503")));
        assert_eq!(e.to_string(), "not read: Bandcamp answered with its bot check");
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
