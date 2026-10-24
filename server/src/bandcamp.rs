//! Bandcamp, read to play from (decision 0071): an album's page, its tracks and each one's stream; an artist's list of
//! releases. Played, never downloaded: nothing is kept on disk. Bandcamp's search is not used: it answers with a check
//! for browsers (a "Client Challenge"), which Cratarr does not get around.
//!
//! The page is read as yt-dlp's Bandcamp extractor reads it (`yt_dlp/extractor/bandcamp.py`, the Unlicense: public
//! domain): the album's data in the page's `data-tralbum` attribute (HTML-escaped JSON), each track's `file` holding
//! its `mp3-128` stream, its cover the page's `og:image`. Written for Cratarr in Rust.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// An album's page read again after this long: its streams' addresses expire.
const KEEP: Duration = Duration::from_secs(5 * 60);
/// The most album pages kept at once.
const KEEP_MOST: usize = 64;

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

/// An album as its page gives it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Album {
    /// Its page, as read (no query, no fragment).
    pub url: String,
    pub title: String,
    pub artist: String,
    pub cover: Option<String>,
    pub tracks: Vec<Track>,
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
}

impl Bandcamp {
    pub fn new() -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("Cratarr/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { http, kept: Mutex::new(HashMap::new()) })
    }

    /// The one reader, for the server's life.
    pub fn get() -> &'static Bandcamp {
        static ONE: std::sync::OnceLock<Bandcamp> = std::sync::OnceLock::new();
        ONE.get_or_init(|| Bandcamp::new().expect("an HTTP client"))
    }

    async fn text(&self, url: &str) -> Result<String> {
        let resp = self.http.get(url).send().await.context("cannot reach Bandcamp")?;
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

    /// A track's stream, asked for as it is played: the response, its status and headers as Bandcamp sends them
    /// (`range` passed on, so a player can seek).
    pub async fn stream(&self, url: &str, n: u32, range: Option<&str>) -> Result<reqwest::Response> {
        let a = self.album(url).await?;
        let t = a.tracks.iter().find(|t| t.n == n).context("No such track on that album")?;
        let s = t.stream.as_deref().context("That track does not stream on Bandcamp")?;
        let mut req = self.http.get(s).timeout(Duration::from_secs(60 * 60));
        if let Some(r) = range {
            req = req.header(reqwest::header::RANGE, r);
        }
        let resp = req.send().await.context("cannot reach Bandcamp")?;
        if !resp.status().is_success() {
            // An address expired early: read the page again next time.
            self.kept.lock().unwrap_or_else(|p| p.into_inner()).remove(&a.url);
            bail!("Bandcamp answered {}", resp.status());
        }
        Ok(resp)
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

// ---- The pages

#[derive(Deserialize)]
struct Tralbum {
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    current: Option<Current>,
    #[serde(default)]
    trackinfo: Vec<TrackInfo>,
}

#[derive(Deserialize)]
struct Current {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    artist: Option<String>,
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
    let current = t.current.unwrap_or(Current { title: None, artist: None });
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
    let cover = meta(html, "og:image");
    Ok(Album { url: url.to_string(), title: current.title.unwrap_or_default(), artist, cover, tracks })
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
    fn entities_are_read_and_a_stray_ampersand_stays() {
        assert_eq!(unescape("a &amp; b &quot;c&quot; &#39;d&#x27; R&B &unknown; x"), "a & b \"c\" 'd' R&B &unknown; x");
        assert_eq!(unescape("Björk &ééééééééééé; Sigur Rós"), "Björk &ééééééééééé; Sigur Rós", "never cut inside a letter");
    }
}
