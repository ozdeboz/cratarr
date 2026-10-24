//! Last.fm into Cratarr (decision 0067, step 3): a user's scrobbles into the history, and their loved tracks as
//! favourites. Read from Last.fm's API with the API key the person made there (Last.fm asks one of every caller; it is
//! kept with the import's choices, not a password). Found by the shared matcher (`app::import`), previewed first, safe to
//! run again, undone as a whole.
//!
//! - **Scrobbles** are logged as heard: finished, not counted (counts come from a server's import), context `lastfm`,
//!   their own id its time and names. One already in the history from elsewhere (the same song within a minute, from
//!   ListenBrainz say, which a scrobbler often fed too) is not added twice.
//! - **Loved tracks** are stars, kept as an import's (`userdata::apply_import`, source `lastfm`), so the undo takes
//!   away the ones still as it set them, and a track loved no more there is unstarred here next time.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::import::{Counter, Listened, Matcher, Tally, Track, into_history, key};
use crate::serve::import::{Seen, decide};
use crate::userdata::{ImportChange, ImportKey, Kind};

const API: &str = "https://ws.audioscrobbler.com/2.0/";
/// Last.fm's largest page of recent tracks (and of loved ones).
const PAGE: usize = 200;
/// Between pages: Last.fm asks for no more than about five calls a second.
const PAUSE: Duration = Duration::from_millis(250);
/// What the history calls a scrobble's context: how its undo finds it.
pub const CONTEXT: &str = "lastfm";
/// The import's source, for its stars.
const SOURCE: &str = "lastfm";

/// A scrobble, or a loved track (with when it was loved).
#[derive(Debug, Clone, PartialEq)]
pub struct Scrobble {
    pub at: i64,
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    /// MusicBrainz's recording, where Last.fm has it.
    pub recording: Option<String>,
}

#[derive(Deserialize)]
struct Text {
    #[serde(rename = "#text", default)]
    text: String,
}

#[derive(Deserialize)]
struct Named {
    #[serde(default)]
    name: String,
    #[serde(rename = "#text", default)]
    text: String,
}

#[derive(Deserialize)]
struct Date {
    uts: String,
}

#[derive(Deserialize)]
struct Item {
    #[serde(default)]
    name: String,
    #[serde(default)]
    mbid: String,
    artist: Named,
    album: Option<Text>,
    date: Option<Date>,
    /// `{"nowplaying": "true"}` on the track playing now, which has no date and is no scrobble yet.
    #[serde(rename = "@attr")]
    attr: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Attr {
    #[serde(rename = "totalPages", default)]
    total_pages: String,
}

#[derive(Deserialize)]
struct Recent {
    recenttracks: List,
}

#[derive(Deserialize)]
struct Loved {
    lovedtracks: List,
}

#[derive(Deserialize)]
struct List {
    #[serde(default)]
    track: Vec<Item>,
    #[serde(rename = "@attr")]
    attr: Attr,
}

/// Last.fm's answer when it refuses ("User not found", "Invalid API key").
#[derive(Deserialize)]
struct Refused {
    message: String,
}

fn scrobble(i: Item) -> Option<Scrobble> {
    if i.attr.as_ref().and_then(|a| a.get("nowplaying")).is_some() {
        return None;
    }
    let at = i.date?.uts.parse().ok()?;
    let artist = if i.artist.name.is_empty() { i.artist.text } else { i.artist.name };
    Some(Scrobble {
        at,
        artist,
        title: i.name,
        album: i.album.map(|a| a.text).filter(|a| !a.is_empty()),
        recording: Some(i.mbid).filter(|m| !m.is_empty()),
    })
}

/// Every page of `method` for `user`, read into scrobbles.
async fn read(method: &str, user: &str, api_key: &str) -> Result<Vec<Scrobble>> {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?;
    let mut out = Vec::new();
    let mut page = 1usize;
    loop {
        let q = [
            ("method", method.to_string()),
            ("user", user.to_string()),
            ("api_key", api_key.to_string()),
            ("format", "json".into()),
            ("limit", PAGE.to_string()),
            ("page", page.to_string()),
        ];
        let res = http.get(API).query(&q).send().await.context("Last.fm did not answer")?;
        let ok = res.status().is_success();
        let body = res.text().await?;
        if !ok {
            let why = serde_json::from_str::<Refused>(&body).map(|r| r.message).unwrap_or(body);
            bail!("Last.fm refused: {why}");
        }
        let list = if method == "user.getlovedtracks" {
            serde_json::from_str::<Loved>(&body).context("Last.fm's answer could not be read")?.lovedtracks
        } else {
            serde_json::from_str::<Recent>(&body).context("Last.fm's answer could not be read")?.recenttracks
        };
        let pages: usize = list.attr.total_pages.parse().unwrap_or(1);
        out.extend(list.track.into_iter().filter_map(scrobble));
        if page >= pages {
            return Ok(out);
        }
        page += 1;
        tokio::time::sleep(PAUSE).await;
    }
}

fn track(s: &Scrobble) -> Track<'_> {
    Track { recording: s.recording.as_deref(), artist: &s.artist, title: &s.title, album: s.album.as_deref(), length_secs: None }
}

/// A scrobble's own id in the history: its time and names (Last.fm gives scrobbles none).
fn event(s: &Scrobble) -> String {
    format!("{CONTEXT}:{}:{}:{}", s.at, key(&s.artist), key(&s.title))
}

/// What an import from Last.fm would do or did: its scrobbles and its loved tracks.
#[derive(Debug, Default, Serialize)]
pub struct Imported {
    pub scrobbles: Tally,
    pub loved: Tally,
}

/// Reads Last.fm and does the work, writing nothing for a preview. Blocking parts run apart.
async fn run(serve: &Arc<crate::serve::Serve>, account: &str, user: &str, api_key: &str, preview: bool) -> Result<Imported> {
    let dir = serve.data_dir.clone().context("the history needs Cratarr's data folder")?;
    let scrobbles = read("user.getrecenttracks", user, api_key).await?;
    let loved = read("user.getlovedtracks", user, api_key).await?;
    let (serve, account, user) = (serve.clone(), account.to_string(), user.to_string());
    tokio::task::spawn_blocking(move || -> Result<Imported> {
        let songs = Matcher::read(&Path::new(&dir).join("library.sqlite"))?;
        let listens: Vec<Listened> = scrobbles.iter().map(|s| Listened { track: track(s), at: s.at, event: event(s) }).collect();
        let scrobbled = into_history(&serve, &songs, &account, CONTEXT, "Last.fm", &listens, preview)?;
        // Loved: stars, as an import keeps them; one loved there no more is unstarred here (where it is still ours).
        let key = ImportKey { account: &account, source: SOURCE, server_user: &user };
        let mut l = Counter::default();
        let mut found: HashMap<String, Option<i64>> = HashMap::new();
        for s in &loved {
            match songs.find(&track(s)) {
                Some(song) => {
                    found.entry(song.to_string()).or_insert(Some(s.at));
                }
                None => l.missed(&track(s)),
            }
        }
        let items: Vec<String> = found.keys().cloned().collect();
        let last = serve.userdata.imported(key, Kind::Song, &items)?;
        let mut changes = Vec::new();
        for (song, at) in &found {
            let before = last.get(song);
            let d = decide(before, &Seen { server_item: song.clone(), star: Some((true, *at)), ..Seen::default() });
            l.found(d.star.is_some());
            changes.push(ImportChange { kind: Kind::Song, item: song.clone(), star: d.star, now: d.now, ..ImportChange::default() });
        }
        for (song, was) in serve.userdata.imported_starred(key, Kind::Song)? {
            if !found.contains_key(&song) {
                let d = decide(Some(&was), &Seen { server_item: song.clone(), star: Some((false, None)), ..Seen::default() });
                changes.push(ImportChange { kind: Kind::Song, item: song, star: d.star, now: d.now, ..ImportChange::default() });
            }
        }
        if !preview {
            serve.userdata.apply_import(key, &changes)?;
        }
        Ok(Imported { scrobbles: scrobbled, loved: l.tally() })
    })
    .await?
}

/// What importing `user`'s Last.fm into `account` would bring: nothing written.
pub async fn preview(serve: &Arc<crate::serve::Serve>, account: &str, user: &str, api_key: &str) -> Result<Imported> {
    run(serve, account, user, api_key, true).await
}

/// Imports `user`'s scrobbles and loved tracks into `account` (decision 0067).
pub async fn import(serve: &Arc<crate::serve::Serve>, account: &str, user: &str, api_key: &str) -> Result<Imported> {
    run(serve, account, user, api_key, false).await
}

/// What an undo took back: the scrobbles out of the history, the loves' stars.
#[derive(Debug, Default, Serialize)]
pub struct Undone {
    pub scrobbles: usize,
    pub unstarred: usize,
}

/// Every scrobble imported from Last.fm out of `account`'s history, and the stars its loved tracks set taken away where
/// they are still as it set them (decision 0067).
pub fn undo(serve: &crate::serve::Serve, account: &str, user: &str) -> Result<Undone> {
    let scrobbles = serve.userdata.forget_heard(account, CONTEXT)?;
    let key = ImportKey { account, source: SOURCE, server_user: user };
    let stars = serve.userdata.undo_import(key, |_| None)?;
    Ok(Undone { scrobbles, unstarred: stars.unstarred })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_fms_answer_reads_into_scrobbles_without_the_one_playing_now() {
        let body = r##"{"recenttracks":{"track":[
            {"artist":{"mbid":"","#text":"Photay"},"name":"Kingdom","mbid":"ab12","album":{"#text":"Onism"},"@attr":{"nowplaying":"true"}},
            {"artist":{"mbid":"","#text":"Photay"},"name":"Kingdom","mbid":"ab12","album":{"#text":"Onism"},"date":{"uts":"1700000000","#text":"x"}},
            {"artist":{"#text":"DJ Healer"},"name":"Untitled","mbid":"","album":{"#text":""},"date":{"uts":"1700000300","#text":"x"}}
        ],"@attr":{"user":"me","totalPages":"1","page":"1","perPage":"200","total":"2"}}}"##;
        let list = serde_json::from_str::<Recent>(body).unwrap().recenttracks;
        let s: Vec<Scrobble> = list.track.into_iter().filter_map(scrobble).collect();
        assert_eq!(s.len(), 2, "the one playing now is no scrobble yet");
        assert_eq!(s[0], Scrobble { at: 1_700_000_000, artist: "Photay".into(), title: "Kingdom".into(), album: Some("Onism".into()), recording: Some("ab12".into()) });
        assert_eq!((s[1].album.as_deref(), s[1].recording.as_deref()), (None, None));
        let loved = r##"{"lovedtracks":{"track":[{"artist":{"name":"Photay","mbid":"","url":""},"name":"Kingdom","mbid":"","date":{"uts":"1600000000","#text":"x"}}],
            "@attr":{"user":"me","totalPages":"1","page":"1","perPage":"200","total":"1"}}}"##;
        let l: Vec<Scrobble> = serde_json::from_str::<Loved>(loved).unwrap().lovedtracks.track.into_iter().filter_map(scrobble).collect();
        assert_eq!((l[0].artist.as_str(), l[0].at), ("Photay", 1_600_000_000), "a loved track's artist is its name");
    }
}
