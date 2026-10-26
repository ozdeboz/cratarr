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
//!
//! It runs as a task (`app::tasks`): reading the scrobbles and the loves says how far it is, and what was read is kept
//! for an hour, so the Import after a Preview reads only what came since. A history Last.fm keeps private is read
//! through the account's own Last.fm sign-in (Connections, `app::scrobble`), where it has one.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::import::{Counter, Listened, Matcher, Tally, Track, into_history, key};
use crate::serve::import::{Seen, decide};
use crate::userdata::{ImportChange, ImportKey, Kind};
use super::reach::Sent as _;

const API: &str = "https://ws.audioscrobbler.com/2.0/";
/// Last.fm's largest page of recent tracks (and of loved ones).
const PAGE: usize = 200;
/// Between pages: Last.fm asks for no more than about five calls a second.
const PAUSE: Duration = Duration::from_millis(250);
/// Of the loved tracks with no album from the scrobbles and not in the library, at most this many are asked of
/// Last.fm's track info (one a quarter second).
const TRACK_LOOKUPS: usize = 500;
/// A title this short (as the matcher keys it) is too common to take an album by the title alone: "Intro", "Outro",
/// "Home" (their test).
const TITLE_ALONE_MIN: usize = 6;
/// What the history calls a scrobble's context: how its undo finds it.
pub const CONTEXT: &str = "lastfm";
/// The import's source, for its stars.
const SOURCE: &str = "lastfm";

/// A scrobble, or a loved track (with when it was loved).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// How many there are in all.
    #[serde(default)]
    total: String,
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
    #[serde(default)]
    error: Option<i64>,
    message: String,
}

/// A page is asked this many times before the import gives up (waiting 2, 4, 8 s between).
const TRIES: u32 = 4;
/// Last.fm's errors that say to try again: operation failed, service offline, temporarily unavailable, rate limit.
const TRY_AGAIN: [i64; 4] = [8, 11, 16, 29];

/// Last.fm's error for a history its user keeps private, asked without their sign-in.
const PRIVATE: i64 = 17;

/// How Cratarr asks Last.fm: with an API key, and the account's Last.fm sign-in where it has one (signed with the
/// secret its key came with), which also reads a history kept private.
#[derive(Debug, Clone)]
pub struct Auth {
    pub key: String,
    pub signed: Option<(String, String)>,
}

/// Where a long read is: what it does, how many done, of how many.
pub type Progress = dyn Fn(&str, u64, Option<u64>) + Send + Sync;

/// What was read, per account and Last.fm user, for an hour: the Import after a Preview reads only what is new.
struct Kept {
    at: Instant,
    scrobbles: Vec<Scrobble>,
    loved: Vec<Scrobble>,
}

const KEPT_FOR: Duration = Duration::from_secs(3600);

fn kept() -> &'static Mutex<HashMap<(String, String), Kept>> {
    static KEPT: std::sync::OnceLock<Mutex<HashMap<(String, String), Kept>>> = std::sync::OnceLock::new();
    KEPT.get_or_init(Default::default)
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

/// Every page of `method` for `user` (from `from` on and up to `to`, where given), read into scrobbles; `step` says how
/// far. With a spool, each page is kept in it as it comes (0.9.7: an import that stops resumes from there).
#[allow(clippy::too_many_arguments)]
async fn read(
    method: &str,
    user: &str,
    auth: &Auth,
    from: Option<i64>,
    to: Option<i64>,
    step: &str,
    progress: &Progress,
    mut spool: Option<&mut super::resume::Spool<Scrobble>>,
) -> Result<Vec<Scrobble>> {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?;
    let mut out = Vec::new();
    let mut page = 1usize;
    loop {
        let mut q: BTreeMap<&str, String> = BTreeMap::new();
        q.insert("method", method.to_string());
        q.insert("user", user.to_string());
        q.insert("api_key", auth.key.clone());
        q.insert("limit", PAGE.to_string());
        q.insert("page", page.to_string());
        if let Some(f) = from {
            q.insert("from", f.to_string());
        }
        if let Some(t) = to {
            q.insert("to", t.to_string());
        }
        if let Some((session, secret)) = &auth.signed {
            q.insert("sk", session.clone());
            let sig = super::scrobble::signature(&q, secret);
            q.insert("api_sig", sig);
        }
        q.insert("format", "json".into());
        // A page asked again a few times, waiting longer each time, where Last.fm did not answer, was busy or said to
        // try again (0.9.7: one timeout on page 8 threw away the seven read before it).
        let mut tries = 0u32;
        let (ok, body) = loop {
            tries += 1;
            let sent = http.get(API).query(&q).sent().await;
            let busy = match &sent {
                Err(_) => true,
                Ok(res) => res.status().is_server_error() || res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS,
            };
            if busy && tries < TRIES {
                tokio::time::sleep(Duration::from_secs(2u64.pow(tries))).await;
                continue;
            }
            let res = sent.context("Last.fm did not answer")?;
            let ok = res.status().is_success();
            let body = res.text().await?;
            let again = serde_json::from_str::<Refused>(&body).ok().and_then(|r| r.error).is_some_and(|e| TRY_AGAIN.contains(&e));
            if again && tries < TRIES {
                tokio::time::sleep(Duration::from_secs(2u64.pow(tries))).await;
                continue;
            }
            break (ok, body);
        };
        if let Some(r) = serde_json::from_str::<Refused>(&body).ok().filter(|r| !ok || r.error.is_some()) {
            if r.error == Some(PRIVATE) {
                bail!(
                    "{user}'s Last.fm history is private. Connect Last.fm under Connections › Listening services, or turn off Hide recent listening information on last.fm while it imports."
                );
            }
            bail!("Last.fm refused: {}", r.message);
        }
        if !ok {
            bail!("Last.fm refused: {body}");
        }
        let list = if method == "user.getlovedtracks" {
            serde_json::from_str::<Loved>(&body).context("Last.fm's answer could not be read")?.lovedtracks
        } else {
            serde_json::from_str::<Recent>(&body).context("Last.fm's answer could not be read")?.recenttracks
        };
        let pages: usize = list.attr.total_pages.parse().unwrap_or(1);
        let got: Vec<Scrobble> = list.track.into_iter().filter_map(scrobble).collect();
        if let Some(s) = spool.as_deref_mut() {
            s.add(got.iter().cloned())?;
        }
        out.extend(got);
        progress(step, out.len() as u64, list.attr.total.parse().ok());
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
    /// Every loved track the library does not have (the owner, 2026-10-24: "a list of these tracks and their albums"),
    /// by artist, album and title. Last.fm keeps no album with a love: it is the one the track was most often
    /// scrobbled from, where it was.
    pub loved_missing: Vec<Wanted>,
}

/// What a Last.fm import brings (0.9.2; the owner, 2026-10-25: "choose what comes in", "leave unstarring off"): its
/// scrobbles into the history, its loved tracks as favourites; and, only when asked, a track loved there no more stops
/// being a favourite here (one the import made a favourite).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    #[serde(default = "yes")]
    pub scrobbles: bool,
    #[serde(default = "yes")]
    pub loves: bool,
    #[serde(default)]
    pub unstar: bool,
}

fn yes() -> bool {
    true
}

impl Default for Choice {
    fn default() -> Self {
        Self { scrobbles: true, loves: true, unstar: false }
    }
}

/// A loved track the library does not have.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Wanted {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
}

/// The album each track was most often scrobbled from (by artist and title, as the matcher keys them).
fn albums_of(scrobbles: &[Scrobble]) -> HashMap<(String, String), String> {
    let mut counts: HashMap<(String, String), HashMap<&str, usize>> = HashMap::new();
    for s in scrobbles {
        if let Some(a) = s.album.as_deref() {
            *counts.entry((key(&s.artist), key(&s.title))).or_default().entry(a).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter_map(|(k, by)| by.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0))).map(|(a, _)| (k, a.to_string())))
        .collect()
}

/// Each loved track's album (`fill_albums`): the one it was most often scrobbled from under its own artist; else, by its
/// title alone, where that title was scrobbled from one album only (0.9.3; the owner, 2026-10-25: a love is often kept
/// under another of the track's artists, Alchemist's "Eggs on the Third Floor" being Action Bronson's on Rare
/// Chandeliers). A short title is too common for that.
fn fill_albums(loved: &mut [Scrobble], scrobbles: &[Scrobble]) {
    let exact = albums_of(scrobbles);
    let mut by_title: HashMap<String, HashMap<String, String>> = HashMap::new();
    for s in scrobbles {
        if let Some(a) = s.album.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
            by_title.entry(key(&s.title)).or_default().entry(key(a)).or_insert_with(|| a.to_string());
        }
    }
    for s in loved.iter_mut().filter(|s| s.album.is_none()) {
        let title = key(&s.title);
        s.album = exact.get(&(key(&s.artist), title.clone())).cloned().or_else(|| {
            let albums = by_title.get(&title).filter(|a| a.len() == 1 && title.chars().count() >= TITLE_ALONE_MIN)?;
            albums.values().next().cloned()
        });
    }
}

/// The album Last.fm gives a track (its track info), where it knows one.
async fn track_album(http: &reqwest::Client, auth: &Auth, artist: &str, title: &str) -> Result<Option<String>> {
    let body: serde_json::Value = http
        .get(API)
        .query(&[("method", "track.getinfo"), ("api_key", auth.key.as_str()), ("artist", artist), ("track", title), ("autocorrect", "1"), ("format", "json")])
        .sent()
        .await
        .context("Last.fm did not answer")?
        .json()
        .await?;
    Ok(body["track"]["album"]["title"].as_str().map(str::trim).filter(|a| !a.is_empty()).map(str::to_string))
}

/// The loved tracks not `found`, each track once (the owner, 2026-10-25): one loved under each of its artists is one row
/// with all of them (by its title and album where it has one, else by its artist and title); by artist, album and title.
fn wanted(loved: &[Scrobble], found: impl Fn(&Scrobble) -> bool) -> Vec<Wanted> {
    let mut out: Vec<Wanted> = Vec::new();
    let mut artists: Vec<Vec<String>> = Vec::new();
    let mut at: HashMap<(String, String), usize> = HashMap::new();
    for s in loved.iter().filter(|s| !found(s)) {
        let k = match &s.album {
            Some(a) => (format!("album {}", key(a)), key(&s.title)),
            None => (format!("artist {}", key(&s.artist)), key(&s.title)),
        };
        match at.get(&k) {
            Some(&i) => {
                if !artists[i].contains(&key(&s.artist)) {
                    artists[i].push(key(&s.artist));
                    out[i].artist = format!("{}, {}", out[i].artist, s.artist);
                }
            }
            None => {
                at.insert(k, out.len());
                artists.push(vec![key(&s.artist)]);
                out.push(Wanted { artist: s.artist.clone(), title: s.title.clone(), album: s.album.clone() });
            }
        }
    }
    out.sort_by_cached_key(|w| (key(&w.artist), w.album.as_deref().map(key), key(&w.title)));
    out
}

/// `user`'s scrobbles and loves: as read within the hour plus what came since, or read now.
async fn scrobbles_and_loves(dir: &Path, account: &str, user: &str, auth: &Auth, progress: &Progress) -> Result<(Vec<Scrobble>, Vec<Scrobble>)> {
    let id = (account.to_string(), user.to_lowercase());
    let before = {
        let mut k = kept().lock().unwrap_or_else(|e| e.into_inner());
        k.retain(|_, v| v.at.elapsed() < KEPT_FOR);
        k.get(&id).map(|v| (v.scrobbles.clone(), v.loved.clone()))
    };
    let (scrobbles, loved) = match before {
        Some((mut scrobbles, loved)) => {
            let newest = scrobbles.iter().map(|s| s.at).max();
            let mut new = read("user.getrecenttracks", user, auth, newest.map(|n| n + 1), None, "Reading new scrobbles", progress, None).await?;
            new.retain(|s| newest.is_none_or(|n| s.at > n));
            new.append(&mut scrobbles);
            (new, loved)
        }
        None => {
            // A read stopped before goes on where it stopped (its spool), after the scrobbles made since; a whole one
            // takes the spool away.
            let mut spool = super::resume::Spool::<Scrobble>::open(dir, SOURCE, account, user);
            let scrobbles = match (spool.items.iter().map(|s| s.at).max(), spool.items.iter().map(|s| s.at).min()) {
                (Some(newest), Some(oldest)) => {
                    let mut new = read("user.getrecenttracks", user, auth, Some(newest + 1), None, "Reading new scrobbles", progress, None).await?;
                    new.retain(|s| s.at > newest);
                    read("user.getrecenttracks", user, auth, None, Some(oldest - 1), "Reading scrobbles on from where the last import stopped", progress, Some(&mut spool))
                        .await?;
                    new.extend(spool.items.iter().filter(|s| s.at <= newest).cloned());
                    new
                }
                _ => read("user.getrecenttracks", user, auth, None, None, "Reading scrobbles", progress, Some(&mut spool)).await?,
            };
            spool.finish();
            let loved = read("user.getlovedtracks", user, auth, None, None, "Reading loved tracks", progress, None).await?;
            (scrobbles, loved)
        }
    };
    kept().lock().unwrap_or_else(|e| e.into_inner()).insert(id, Kept { at: Instant::now(), scrobbles: scrobbles.clone(), loved: loved.clone() });
    Ok((scrobbles, loved))
}

/// Reads Last.fm and does the work, writing nothing for a preview. Blocking parts run apart.
async fn run(serve: &Arc<crate::serve::Serve>, account: &str, user: &str, auth: &Auth, choice: Choice, preview: bool, progress: &Progress) -> Result<Imported> {
    let dir = serve.data_dir.clone().context("the history needs Cratarr's data folder")?;
    let (scrobbles, loved) = match scrobbles_and_loves(Path::new(&dir), account, user, auth, progress).await {
        Ok(read) => read,
        // Stopped part of the way (0.9.7): the scrobbles read so far go in now (an import, not a preview), and the next
        // import goes on from where this one stopped.
        Err(e) => {
            let spool = super::resume::Spool::<Scrobble>::open(Path::new(&dir), SOURCE, account, user);
            if preview || !choice.scrobbles || spool.items.is_empty() {
                return Err(e);
            }
            let n = spool.items.len();
            let partial: Vec<Scrobble> = spool.items;
            let (serve, account) = (serve.clone(), account.to_string());
            tokio::task::spawn_blocking(move || -> Result<Tally> {
                let songs = Matcher::read(&Path::new(&dir).join("library.sqlite"))?;
                let listens: Vec<Listened> = partial.iter().map(|s| Listened { track: track(s), at: s.at, event: event(s) }).collect();
                into_history(&serve, &songs, &account, CONTEXT, "Last.fm", &listens, false)
            })
            .await??;
            return Err(e.context(format!("the {n} scrobbles read before it stopped are in; the next import goes on from where it stopped")));
        }
    };
    progress("Matching", 0, None);
    // The loved tracks' albums: from the scrobbles (`fill_albums`); then, for those still without one and not in the
    // library, from Last.fm's track info. An album found also matches the library's track by album and title, under
    // whichever artist it was loved.
    let at = dir.clone();
    let (songs, mut loved, scrobbles, asks) = tokio::task::spawn_blocking(move || -> Result<(Matcher, Vec<Scrobble>, Vec<Scrobble>, Vec<usize>)> {
        let songs = Matcher::read(&Path::new(&at).join("library.sqlite"))?;
        let mut loved = loved;
        fill_albums(&mut loved, &scrobbles);
        let asks = loved.iter().enumerate().filter(|(_, s)| s.album.is_none() && songs.find(&track(s)).is_none()).map(|(i, _)| i).take(TRACK_LOOKUPS).collect();
        Ok((songs, loved, scrobbles, asks))
    })
    .await??;
    let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?;
    for (n, &i) in asks.iter().enumerate() {
        progress("Finding the loved tracks' albums", n as u64, Some(asks.len() as u64));
        if let Ok(Some(album)) = track_album(&http, auth, &loved[i].artist, &loved[i].title).await {
            loved[i].album = Some(album);
        }
        tokio::time::sleep(PAUSE).await;
    }
    progress("Matching", 0, None);
    let (serve, account, user) = (serve.clone(), account.to_string(), user.to_string());
    tokio::task::spawn_blocking(move || -> Result<Imported> {
        let listens: Vec<Listened> = scrobbles.iter().map(|s| Listened { track: track(s), at: s.at, event: event(s) }).collect();
        // The scrobbles only when chosen (they are still read: the loved tracks' albums come from them).
        let scrobbled = if choice.scrobbles { into_history(&serve, &songs, &account, CONTEXT, "Last.fm", &listens, preview)? } else { Tally::default() };
        // Loved: stars, as an import keeps them; one loved there no more is unstarred here (where it is still ours).
        let key = ImportKey { account: &account, source: SOURCE, server_user: &user };
        let mut l = Counter::default();
        let mut found: HashMap<String, Option<i64>> = HashMap::new();
        let loved_missing = wanted(&loved, |s| songs.find(&track(s)).is_some());
        for s in loved.iter().filter(|_| choice.loves) {
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
        // Unstarred only when asked (the owner: "leave unstarring off"), and only where the loves were imported.
        let unstarring = if choice.loves && choice.unstar { serve.userdata.imported_starred(key, Kind::Song)? } else { Default::default() };
        for (song, was) in unstarring {
            if !found.contains_key(&song) {
                let d = decide(Some(&was), &Seen { server_item: song.clone(), star: Some((false, None)), ..Seen::default() });
                changes.push(ImportChange { kind: Kind::Song, item: song, star: d.star, now: d.now, ..ImportChange::default() });
            }
        }
        if !preview {
            serve.userdata.apply_import(key, &changes)?;
        }
        Ok(Imported { scrobbles: scrobbled, loved: l.tally(), loved_missing })
    })
    .await?
}

/// What importing `user`'s Last.fm into `account` would bring (nothing written), or imports it (decision 0067).
pub async fn bring(serve: &Arc<crate::serve::Serve>, account: &str, user: &str, auth: &Auth, choice: Choice, preview: bool, progress: &Progress) -> Result<Imported> {
    run(serve, account, user, auth, choice, preview, progress).await
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
    fn a_loved_track_not_in_the_library_is_listed_once_with_the_album_it_was_scrobbled_from() {
        let s = |artist: &str, title: &str, album: Option<&str>| Scrobble { at: 0, artist: artist.into(), title: title.into(), album: album.map(Into::into), recording: None };
        let scrobbles = [s("Bonobo", "Kerala", Some("Migration")), s("Bonobo", "Kerala", Some("Migration")), s("Bonobo", "Kerala", Some("Kerala (Remixes)")), s("Air", "Alone in Kyoto", None)];
        let mut loved = [s("Bonobo", "Kerala", None), s("bonobo", "kerala", None), s("Air", "Alone in Kyoto", None), s("Air", "Venus", None)];
        fill_albums(&mut loved, &scrobbles);
        let got = wanted(&loved, |x| x.title == "Venus");
        assert_eq!(
            got,
            [
                Wanted { artist: "Air".into(), title: "Alone in Kyoto".into(), album: None },
                Wanted { artist: "Bonobo".into(), title: "Kerala".into(), album: Some("Migration".into()) },
            ],
            "the found one left out, the same track once, the album most scrobbled from"
        );
    }

    #[test]
    fn a_track_loved_under_each_of_its_artists_is_one_row_with_the_album_its_title_was_scrobbled_from() {
        let s = |artist: &str, title: &str, album: Option<&str>| Scrobble { at: 0, artist: artist.into(), title: title.into(), album: album.map(Into::into), recording: None };
        let scrobbles = [
            s("Action Bronson", "Eggs on the Third Floor", Some("Rare Chandeliers (Extended Version)")),
            s("Some Band", "Intro", Some("First")),
            s("Other Band", "Ruins", Some("One")),
            s("Third Band", "Ruins", Some("Two")),
        ];
        let mut loved = [
            s("Action Bronson", "Eggs on the Third Floor", None),
            s("Alchemist", "Eggs On The Third Floor", None),
            s("Nobody", "Intro", None),
            s("Somebody", "Ruins", None),
        ];
        fill_albums(&mut loved, &scrobbles);
        assert_eq!(loved[1].album.as_deref(), Some("Rare Chandeliers (Extended Version)"), "by its title, scrobbled from one album");
        assert_eq!(loved[2].album, None, "too short a title to go by alone");
        assert_eq!(loved[3].album, None, "a title from two albums is not guessed");
        let got = wanted(&loved, |_| false);
        assert_eq!(got[0], Wanted { artist: "Action Bronson, Alchemist".into(), title: "Eggs on the Third Floor".into(), album: Some("Rare Chandeliers (Extended Version)".into()) });
        assert_eq!(got.len(), 3, "the other two as they were");
    }

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
