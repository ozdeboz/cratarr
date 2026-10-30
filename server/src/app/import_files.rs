//! Importing from files (decision 0067, step 4): a listening history exported elsewhere, or a playlist from another
//! player, read into what the shared import works on. Nothing of the file is kept.
//!
//! - **History:** ListenBrainz's export (its `.zip`, or the `.json`/`.jsonl` in it), Spotify's (the extended streaming
//!   history's `endsong`/`Streaming_History_Audio` JSON, or the account data's `StreamingHistory*.json`; a play under 30
//!   seconds is no listen, as Spotify counts it), and Last.fm's as the usual export tools write it (CSV: artist, album,
//!   track, date).
//! - **Playlists:** `.m3u` and `.m3u8` (each entry's `#EXTINF` "Artist - Title", else its path's folders and file name),
//!   `.xspf` (creator, title, album, length), and Exportify's CSV of a Spotify playlist.
//! - **Spotify's library** (decision 0073): the account data zip's `YourLibrary.json` and `Playlist*.json` (or those
//!   files alone), as the account's Spotify lists (`app::spotify::export`), beside its streaming history.

use std::io::Read as _;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use super::import::{Counter, Listened, Matcher, Tally, Track, into_history, key};

/// A listen read from a file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Listen {
    pub at: i64,
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub length_secs: Option<f64>,
    pub recording: Option<String>,
}

/// A playlist's track as a file names it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Entry {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub length_secs: Option<f64>,
}

/// What a file holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Read {
    /// Listens, and where they came from (`listenbrainz`, `spotify`, `lastfm`): the history's context is `file:` that.
    History { from: &'static str, listens: Vec<Listen> },
    /// A playlist, with the name it gives itself (else its file's).
    Playlist { name: String, entries: Vec<Entry> },
    /// Spotify's data export: its streaming history's listens, and its library as the account's Spotify lists.
    Spotify { listens: Vec<Listen>, library: Option<super::spotify::Lists> },
}

/// What a file brought (or would bring): listens into the history, or a playlist (its name, its undo token).
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Brought {
    History { from: &'static str, tally: Tally },
    Playlist { name: String, tally: Tally, undo: Option<String> },
    /// Spotify's data export (0073): the listens (none: the file held none, or the history is off) and the lists.
    Spotify { history: Option<Tally>, library: Option<super::spotify::Brought> },
}

/// The history's context for listens from a file of `from`.
pub fn context(from: &str) -> String {
    format!("file:{from}")
}

/// Imports a file into `account` (decision 0067, step 4), or previews it: its listens into the history, or its
/// playlist made of the tracks found (named as the file names it, made free among the account's own).
pub async fn import(app: &Arc<super::App>, serve: &Arc<crate::serve::Serve>, account: &str, name: &str, bytes: &[u8], preview: bool, progress: &super::lastfm::Progress) -> Result<Brought> {
    let dir = serve.data_dir.clone().context("an import needs Cratarr's data folder")?;
    progress("Reading the file", 0, None);
    let read = read(name, bytes)?;
    progress("Matching", 0, None);
    let (s, account2) = (serve.clone(), account.to_string());
    let songs = tokio::task::spawn_blocking(move || Matcher::read(&Path::new(&dir).join("library.sqlite"))).await??;
    match read {
        Read::Spotify { listens, library } => {
            // Its play history off (0.9.5): the library still, the listens not.
            let history = if listens.is_empty() || (!preview && !serve.userdata.tracking(account)) {
                None
            } else {
                Some(tokio::task::spawn_blocking(move || into_history(&s, &songs, &account2, &context("spotify"), "Spotify", &listened("spotify", &listens), preview)).await??)
            };
            let library = match library {
                Some(lists) => {
                    let (account, file) = (account.to_string(), name.to_string());
                    Some(tokio::task::spawn_blocking(move || super::spotify::take_export(&account, &file, lists, preview)).await??)
                }
                None => None,
            };
            Ok(Brought::Spotify { history, library })
        }
        // Its play history off (0.9.5): no listens are brought in either.
        Read::History { .. } if !preview && !serve.userdata.tracking(account) => {
            anyhow::bail!("Play history is off for this account (Settings › Tracking).")
        }
        Read::History { from, listens } => {
            let tally = tokio::task::spawn_blocking(move || into_history(&s, &songs, &account2, &context(from), from_name(from), &listened(from, &listens), preview)).await??;
            Ok(Brought::History { from, tally })
        }
        Read::Playlist { name, entries } => {
            let mut c = Counter::default();
            let mut ids = Vec::new();
            for e in &entries {
                let t = Track { recording: None, artist: &e.artist, title: &e.title, album: e.album.as_deref(), length_secs: e.length_secs };
                match songs.find(&t) {
                    Some(id) => {
                        c.found(true);
                        ids.push(id.to_string());
                    }
                    None => c.missed(&t),
                }
            }
            let tally = c.tally();
            if preview || ids.is_empty() {
                return Ok(Brought::Playlist { name, tally, undo: None });
            }
            let free = app.playlists.free_own_name(&name).await.map_err(|e| anyhow::anyhow!("{e}"))?;
            let made = app
                .playing
                .playlist_from_queue(Some(crate::source::cratarr::KIND.id), &free, Some(ids), true)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(Brought::Playlist { name: made.playlist, tally, undo: made.undo })
        }
    }
}

/// Listens as the history takes them, each with its event id (the same listen from a file again is the same).
fn listened<'a>(from: &str, listens: &'a [Listen]) -> Vec<Listened<'a>> {
    let ctx = context(from);
    listens
        .iter()
        .map(|l| Listened {
            track: Track { recording: l.recording.as_deref(), artist: &l.artist, title: &l.title, album: l.album.as_deref(), length_secs: l.length_secs },
            at: l.at,
            event: format!("{ctx}:{}:{}:{}", l.at, key(&l.artist), key(&l.title)),
        })
        .collect()
}

fn from_name(from: &str) -> &'static str {
    match from {
        "spotify" => "Spotify",
        "lastfm" => "Last.fm",
        _ => "ListenBrainz",
    }
}

/// Reads a file by its name and content.
pub fn read(name: &str, bytes: &[u8]) -> Result<Read> {
    let lower = name.to_lowercase();
    let stem = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s).to_string();
    if lower.ends_with(".zip") || bytes.starts_with(b"PK\x03\x04") {
        if let Some(spotify) = spotify_zip(bytes)? {
            return Ok(spotify);
        }
        return Ok(Read::History { from: "listenbrainz", listens: listenbrainz_zip(bytes)? });
    }
    let text = String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).into_owned();
    if lower.ends_with(".m3u") || lower.ends_with(".m3u8") || text.starts_with("#EXTM3U") {
        return Ok(Read::Playlist { name: stem, entries: m3u(&text) });
    }
    if lower.ends_with(".xspf") || text.contains("<playlist") && text.contains("xspf") {
        let (title, entries) = xspf(&text);
        return Ok(Read::Playlist { name: title.unwrap_or(stem), entries });
    }
    if lower.ends_with(".csv") {
        let rows = csv(&text);
        let header = rows.first().map(|r| r.iter().map(|c| c.to_lowercase()).collect::<Vec<_>>()).unwrap_or_default();
        if header.iter().any(|h| h == "track name") {
            return Ok(Read::Playlist { name: stem, entries: exportify(&rows) });
        }
        return Ok(Read::History { from: "lastfm", listens: lastfm_csv(&rows) });
    }
    let t = text.trim_start();
    if t.starts_with('[') {
        let all: Vec<serde_json::Value> = serde_json::from_str(t).context("not a JSON list")?;
        let first = all.first();
        if first.is_some_and(|v| v.get("ts").is_some() || v.get("master_metadata_track_name").is_some()) {
            return Ok(Read::History { from: "spotify", listens: all.iter().filter_map(spotify_extended).collect() });
        }
        if first.is_some_and(|v| v.get("endTime").is_some()) {
            return Ok(Read::History { from: "spotify", listens: all.iter().filter_map(spotify_account).collect() });
        }
        return Ok(Read::History { from: "listenbrainz", listens: all.iter().filter_map(listenbrainz).collect() });
    }
    if t.starts_with('{')
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(t)
        && super::spotify::export::looks_like(&v)
    {
        return Ok(Read::Spotify { listens: Vec::new(), library: super::spotify::export::lists(&[v]) });
    }
    if t.starts_with('{') {
        // JSON lines: ListenBrainz's.
        let listens = t.lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).filter_map(|v| listenbrainz(&v)).collect();
        return Ok(Read::History { from: "listenbrainz", listens });
    }
    bail!("Cratarr doesn't know this kind of file: a ListenBrainz, Spotify or Last.fm export, or an .m3u, .m3u8, .xspf or Exportify playlist")
}

fn text(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// One of ListenBrainz's listens, as its API and its export write them.
fn listenbrainz(v: &serde_json::Value) -> Option<Listen> {
    let m = v.get("track_metadata")?;
    let info = m.get("additional_info");
    let recording = m
        .get("mbid_mapping")
        .and_then(|x| text(x, "recording_mbid"))
        .or_else(|| info.and_then(|i| text(i, "recording_mbid")));
    Some(Listen {
        at: v.get("listened_at")?.as_i64()?,
        artist: text(m, "artist_name")?,
        title: text(m, "track_name")?,
        album: text(m, "release_name"),
        length_secs: info.and_then(|i| i.get("duration_ms")).and_then(|d| d.as_f64()).filter(|d| *d > 0.0).map(|d| d / 1000.0),
        recording,
    })
}

/// ListenBrainz's export: a zip of JSON-lines files (`listens/2024/1.jsonl`); deflated entries inflated here.
fn listenbrainz_zip(bytes: &[u8]) -> Result<Vec<Listen>> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("not a zip file Cratarr can read")?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index_raw(i)?;
        let name = f.name().to_lowercase();
        if !(name.ends_with(".jsonl") || name.ends_with(".json")) || name.contains("feedback") || name.contains("pinned") {
            continue;
        }
        let deflated = f.compression() == zip::CompressionMethod::DEFLATE;
        let mut raw = Vec::new();
        f.read_to_end(&mut raw)?;
        let data = if deflated {
            let mut d = Vec::new();
            flate2::read::DeflateDecoder::new(&raw[..]).read_to_end(&mut d).context("a file in the zip could not be unpacked")?;
            d
        } else {
            raw
        };
        let text = String::from_utf8_lossy(&data);
        let t = text.trim_start();
        if t.starts_with('[') {
            let all: Vec<serde_json::Value> = serde_json::from_str(t).unwrap_or_default();
            out.extend(all.iter().filter_map(listenbrainz));
        } else {
            out.extend(t.lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).filter_map(|v| listenbrainz(&v)));
        }
    }
    Ok(out)
}

/// A zip's files, unpacked: (name, content).
fn unzip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("not a zip file Cratarr can read")?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index_raw(i)?;
        let name = f.name().to_string();
        let deflated = f.compression() == zip::CompressionMethod::DEFLATE;
        let mut raw = Vec::new();
        f.read_to_end(&mut raw)?;
        let data = if deflated {
            let mut d = Vec::new();
            flate2::read::DeflateDecoder::new(&raw[..]).read_to_end(&mut d).context("a file in the zip could not be unpacked")?;
            d
        } else {
            raw
        };
        out.push((name, data));
    }
    Ok(out)
}

/// Spotify's data export as a zip (0073): its library files and its streaming history; `None` for another zip.
fn spotify_zip(bytes: &[u8]) -> Result<Option<Read>> {
    let files = unzip(bytes)?;
    let base = |n: &str| n.rsplit(['/', '\\']).next().unwrap_or(n).to_lowercase();
    let history = |n: &str| {
        let b = base(n);
        b.ends_with(".json") && (b.starts_with("streaminghistory") || b.starts_with("streaming_history") || b.starts_with("endsong"))
    };
    if !files.iter().any(|(n, _)| super::spotify::export::is_library(n) || history(n)) {
        return Ok(None);
    }
    let (mut listens, mut library) = (Vec::new(), Vec::new());
    for (name, data) in &files {
        let text = String::from_utf8_lossy(data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data)).into_owned();
        if super::spotify::export::is_library(name) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                library.push(v);
            }
        } else if history(name) {
            let all: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap_or_default();
            listens.extend(all.iter().filter_map(|v| if v.get("ts").is_some() { spotify_extended(v) } else { spotify_account(v) }));
        }
    }
    Ok(Some(Read::Spotify { listens, library: super::spotify::export::lists(&library) }))
}

/// Spotify counts a play from 30 seconds on.
const SPOTIFY_PLAY_MS: f64 = 30_000.0;

/// One play of Spotify's extended streaming history; a podcast's or a short one is none.
fn spotify_extended(v: &serde_json::Value) -> Option<Listen> {
    if v.get("ms_played").and_then(|m| m.as_f64()).is_some_and(|ms| ms < SPOTIFY_PLAY_MS) {
        return None;
    }
    Some(Listen {
        at: crate::nowplaying::parse_iso_epoch(&text(v, "ts")?)?,
        artist: text(v, "master_metadata_album_artist_name")?,
        title: text(v, "master_metadata_track_name")?,
        album: text(v, "master_metadata_album_album_name"),
        ..Listen::default()
    })
}

/// One play of Spotify's account data (`StreamingHistory0.json`: `endTime` "2023-01-31 14:05", UTC).
fn spotify_account(v: &serde_json::Value) -> Option<Listen> {
    if v.get("msPlayed").and_then(|m| m.as_f64()).is_some_and(|ms| ms < SPOTIFY_PLAY_MS) {
        return None;
    }
    let end = text(v, "endTime")?;
    let (d, t) = end.split_once(' ')?;
    Some(Listen {
        at: crate::nowplaying::parse_iso_epoch(&format!("{d}T{t}:00Z"))?,
        artist: text(v, "artistName")?,
        title: text(v, "trackName")?,
        ..Listen::default()
    })
}

/// A CSV's rows, quotes and doubled quotes as RFC 4180 has them.
fn csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let (mut row, mut cell, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => row.push(std::mem::take(&mut cell)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                row.push(std::mem::take(&mut cell));
                if row.iter().any(|c| !c.is_empty()) {
                    rows.push(std::mem::take(&mut row));
                }
                row.clear();
            }
            c => cell.push(c),
        }
    }
    row.push(cell);
    if row.iter().any(|c| !c.is_empty()) {
        rows.push(row);
    }
    rows
}

/// Last.fm's history as the export tools write it: artist, album, track, date ("31 Jan 2021 14:05", UTC), a header
/// row or none.
fn lastfm_csv(rows: &[Vec<String>]) -> Vec<Listen> {
    rows.iter()
        .filter(|r| r.len() >= 4)
        .filter_map(|r| {
            Some(Listen {
                at: lastfm_date(&r[3])?,
                artist: r[0].trim().to_string(),
                title: r[2].trim().to_string(),
                album: Some(r[1].trim().to_string()).filter(|a| !a.is_empty()),
                ..Listen::default()
            })
        })
        .filter(|l| !l.artist.is_empty() && !l.title.is_empty())
        .collect()
}

/// "31 Jan 2021 14:05" (UTC), else an epoch, else ISO 8601.
fn lastfm_date(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Ok(n) = s.parse::<i64>() {
        return Some(n);
    }
    let parts: Vec<&str> = s.split([' ', ',']).filter(|p| !p.is_empty()).collect();
    if let [day, month, year, time] = parts[..] {
        let m = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
            .iter()
            .position(|x| month.to_lowercase().starts_with(x))?
            + 1;
        let (h, min) = time.split_once(':')?;
        return crate::nowplaying::parse_iso_epoch(&format!("{year}-{m:02}-{:02}T{:02}:{:02}:00Z", day.parse::<u32>().ok()?, h.parse::<u32>().ok()?, min.parse::<u32>().ok()?));
    }
    crate::nowplaying::parse_iso_epoch(s)
}

/// An `.m3u`'s entries: "Artist - Title" from `#EXTINF` (with its length), else the path: its file name without a
/// track number for the title, its folder for the album, the one above for the artist.
fn m3u(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut info: Option<(Option<f64>, String)> = None;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let (secs, name) = rest.split_once(',').unwrap_or(("", rest));
            info = Some((secs.trim().parse::<f64>().ok().filter(|s| *s > 0.0), name.trim().to_string()));
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
        let file = parts.last().copied().unwrap_or(line);
        let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
        let album = (parts.len() >= 2).then(|| parts[parts.len() - 2].to_string());
        let folder_artist = (parts.len() >= 3).then(|| parts[parts.len() - 3].to_string());
        let (length, named) = info.take().unwrap_or((None, String::new()));
        let entry = match named.split_once(" - ") {
            Some((artist, title)) => Entry { artist: artist.trim().into(), title: title.trim().into(), album, length_secs: length },
            None => Entry { artist: folder_artist.unwrap_or_default(), title: untracked(stem), album, length_secs: length },
        };
        out.push(entry);
    }
    out
}

/// A file name's title: without a leading track number ("01 - ", "1. ", "01 ").
fn untracked(stem: &str) -> String {
    let t = stem.trim_start_matches(|c: char| c.is_ascii_digit());
    let title = t.trim_start_matches([' ', '.', '-', '_']).trim();
    // No number before it, or nothing but a number ("1979"): the name is the title.
    if t.len() == stem.len() || title.is_empty() { stem.trim().to_string() } else { title.to_string() }
}

/// An `.xspf`'s title and its tracks.
fn xspf(text: &str) -> (Option<String>, Vec<Entry>) {
    let tag = |block: &str, name: &str| -> Option<String> {
        let open = format!("<{name}>");
        let start = block.find(&open)? + open.len();
        let end = block[start..].find(&format!("</{name}>"))? + start;
        Some(unescape(block[start..end].trim())).filter(|s| !s.is_empty())
    };
    let head = text.split("<trackList").next().unwrap_or("");
    let mut out = Vec::new();
    for block in text.split("<track>").skip(1) {
        let block = block.split("</track>").next().unwrap_or(block);
        let (Some(artist), Some(title)) = (tag(block, "creator"), tag(block, "title")) else { continue };
        out.push(Entry {
            artist,
            title,
            album: tag(block, "album"),
            length_secs: tag(block, "duration").and_then(|d| d.parse::<f64>().ok()).filter(|d| *d > 0.0).map(|d| d / 1000.0),
        });
    }
    (tag(head, "title"), out)
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&#39;", "'").replace("&amp;", "&")
}

/// Exportify's CSV of a Spotify playlist: by its header's names.
fn exportify(rows: &[Vec<String>]) -> Vec<Entry> {
    let Some(head) = rows.first() else { return Vec::new() };
    let at = |name: &str| head.iter().position(|h| h.trim().eq_ignore_ascii_case(name));
    let (Some(t), Some(a)) = (at("Track Name"), at("Artist Name(s)").or_else(|| at("Artist Name"))) else { return Vec::new() };
    let (al, ms) = (at("Album Name"), at("Duration (ms)").or_else(|| at("Track Duration (ms)")));
    rows[1..]
        .iter()
        .filter_map(|r| {
            let title = r.get(t)?.trim().to_string();
            // Several artists are one cell, comma-separated: the first is the track's.
            let artist = r.get(a)?.split(',').next()?.trim().to_string();
            (!title.is_empty()).then(|| Entry {
                artist,
                title,
                album: al.and_then(|i| r.get(i)).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                length_secs: ms.and_then(|i| r.get(i)).and_then(|s| s.trim().parse::<f64>().ok()).map(|ms| ms / 1000.0),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(r: Read) -> (&'static str, Vec<Listen>) {
        match r {
            Read::History { from, listens } => (from, listens),
            other => panic!("not a history: {other:?}"),
        }
    }

    fn playlist(r: Read) -> (String, Vec<Entry>) {
        match r {
            Read::Playlist { name, entries } => (name, entries),
            other => panic!("not a playlist: {other:?}"),
        }
    }

    #[test]
    fn spotify_histories_both_kinds_and_short_plays_left_out() {
        let ext = br#"[{"ts":"2023-01-31T14:05:00Z","ms_played":200000,"master_metadata_track_name":"Kingdom","master_metadata_album_artist_name":"Photay","master_metadata_album_album_name":"Onism"},
                      {"ts":"2023-01-31T14:09:00Z","ms_played":5000,"master_metadata_track_name":"Skip","master_metadata_album_artist_name":"X"},
                      {"ts":"2023-01-31T15:00:00Z","ms_played":900000,"master_metadata_track_name":null,"episode_name":"A podcast"}]"#;
        let (from, l) = history(read("Streaming_History_Audio_2023.json", ext).unwrap());
        assert_eq!(from, "spotify");
        assert_eq!(l, vec![Listen { at: 1_675_173_900, artist: "Photay".into(), title: "Kingdom".into(), album: Some("Onism".into()), ..Listen::default() }]);
        let acc = br#"[{"endTime":"2023-01-31 14:05","artistName":"Photay","trackName":"Kingdom","msPlayed":200000}]"#;
        assert_eq!(history(read("StreamingHistory0.json", acc).unwrap()).1[0].at, 1_675_173_900);
    }

    #[test]
    fn listenbrainz_exports_as_json_lines_or_a_zip() {
        let line = r#"{"listened_at":1700000000,"track_metadata":{"artist_name":"Photay","track_name":"Kingdom","release_name":"Onism","additional_info":{"duration_ms":240000},"mbid_mapping":{"recording_mbid":"ab12"}}}"#;
        let (from, l) = history(read("1.jsonl", format!("{line}\n{line}\n").as_bytes()).unwrap());
        assert_eq!((from, l.len()), ("listenbrainz", 2));
        assert_eq!((l[0].recording.as_deref(), l[0].length_secs), (Some("ab12"), Some(240.0)));
        // A zip, as ListenBrainz's export: deflated.
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            use std::io::Write;
            let mut z = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            z.start_file("listens/2023/11.jsonl", opts).unwrap();
            z.write_all(line.as_bytes()).unwrap();
            z.start_file("feedback.jsonl", opts).unwrap();
            z.write_all(line.as_bytes()).unwrap();
            z.finish().unwrap();
        }
        assert_eq!(history(read("export.zip", buf.get_ref()).unwrap()).1.len(), 1, "the listens, not the feedback");
    }

    #[test]
    fn last_fms_csv_with_its_dates() {
        let csv = "Photay,Onism,Kingdom,31 Jan 2023 14:05\n\"Simon & Garfunkel\",\"Bookends\",\"America, Again\",01 Feb 2023 09:00\n";
        let (from, l) = history(read("scrobbles.csv", csv.as_bytes()).unwrap());
        assert_eq!(from, "lastfm");
        assert_eq!(l[0].at, 1_675_173_900);
        assert_eq!((l[1].artist.as_str(), l[1].title.as_str()), ("Simon & Garfunkel", "America, Again"));
    }

    #[test]
    fn playlists_m3u_xspf_and_exportify() {
        let m3u = "#EXTM3U\n#EXTINF:240,Photay - Kingdom\n/home/x/Music/Photay/Onism/01 - Kingdom.flac\nD:\\Music\\DJ Healer\\Prisoner of Love\\03 Untitled.mp3\n";
        let (name, e) = playlist(read("Evening.m3u8", m3u.as_bytes()).unwrap());
        assert_eq!(name, "Evening");
        assert_eq!(e[0], Entry { artist: "Photay".into(), title: "Kingdom".into(), album: Some("Onism".into()), length_secs: Some(240.0) });
        assert_eq!((e[1].artist.as_str(), e[1].title.as_str(), e[1].album.as_deref()), ("DJ Healer", "Untitled", Some("Prisoner of Love")), "from the path");
        let xspf = r#"<?xml version="1.0"?><playlist version="1" xmlns="http://xspf.org/ns/0/"><title>Rainy &amp; Slow</title><trackList>
            <track><creator>Photay</creator><title>Kingdom</title><album>Onism</album><duration>240000</duration></track></trackList></playlist>"#;
        let (name, e) = playlist(read("x.xspf", xspf.as_bytes()).unwrap());
        assert_eq!((name.as_str(), e[0].length_secs), ("Rainy & Slow", Some(240.0)));
        let ex = "\"Track URI\",\"Track Name\",\"Artist Name(s)\",\"Album Name\",\"Duration (ms)\"\n\"spotify:track:1\",\"Kingdom\",\"Photay, Someone\",\"Onism\",\"240000\"\n";
        let (_, e) = playlist(read("Evening.csv", ex.as_bytes()).unwrap());
        assert_eq!(e[0], Entry { artist: "Photay".into(), title: "Kingdom".into(), album: Some("Onism".into()), length_secs: Some(240.0) });
    }

    #[test]
    fn a_track_number_leaves_the_title() {
        assert_eq!(untracked("01 - Kingdom"), "Kingdom");
        assert_eq!(untracked("3. Untitled"), "Untitled");
        assert_eq!(untracked("1979"), "1979", "a title that is a number stays");
    }
}
