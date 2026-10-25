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

/// Every page of `method` for `user` (from `from` on, where given), read into scrobbles; `step` says how far.
async fn read(method: &str, user: &str, auth: &Auth, from: Option<i64>, step: &str, progress: &Progress) -> Result<Vec<Scrobble>> {
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
        if let Some((session, secret)) = &auth.signed {
            q.insert("sk", session.clone());
            let sig = super::scrobble::signature(&q, secret);
            q.insert("api_sig", sig);
        }
        q.insert("format", "json".into());
        let res = http.get(API).query(&q).send().await.context("Last.fm did not answer")?;
        let ok = res.status().is_success();
        let body = res.text().await?;
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
        out.extend(list.track.into_iter().filter_map(scrobble));
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

/// The loved tracks not `found`, each once, with the album they were scrobbled from; by artist, album and title.
fn wanted(loved: &[Scrobble], found: impl Fn(&Scrobble) -> bool, albums: &HashMap<(String, String), String>) -> Vec<Wanted> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Wanted> = loved
        .iter()
        .filter(|s| !found(s) && seen.insert((key(&s.artist), key(&s.title))))
        .map(|s| Wanted { artist: s.artist.clone(), title: s.title.clone(), album: albums.get(&(key(&s.artist), key(&s.title))).cloned() })
        .collect();
    out.sort_by_cached_key(|w| (key(&w.artist), w.album.as_deref().map(key), key(&w.title)));
    out
}

/// `user`'s scrobbles and loves: as read within the hour plus what came since, or read now.
async fn scrobbles_and_loves(account: &str, user: &str, auth: &Auth, progress: &Progress) -> Result<(Vec<Scrobble>, Vec<Scrobble>)> {
    let id = (account.to_string(), user.to_lowercase());
    let before = {
        let mut k = kept().lock().unwrap_or_else(|e| e.into_inner());
        k.retain(|_, v| v.at.elapsed() < KEPT_FOR);
        k.get(&id).map(|v| (v.scrobbles.clone(), v.loved.clone()))
    };
    let (scrobbles, loved) = match before {
        Some((mut scrobbles, loved)) => {
            let newest = scrobbles.iter().map(|s| s.at).max();
            let mut new = read("user.getrecenttracks", user, auth, newest.map(|n| n + 1), "Reading new scrobbles", progress).await?;
            new.retain(|s| newest.is_none_or(|n| s.at > n));
            new.append(&mut scrobbles);
            (new, loved)
        }
        None => {
            let scrobbles = read("user.getrecenttracks", user, auth, None, "Reading scrobbles", progress).await?;
            let loved = read("user.getlovedtracks", user, auth, None, "Reading loved tracks", progress).await?;
            (scrobbles, loved)
        }
    };
    kept().lock().unwrap_or_else(|e| e.into_inner()).insert(id, Kept { at: Instant::now(), scrobbles: scrobbles.clone(), loved: loved.clone() });
    Ok((scrobbles, loved))
}

/// Reads Last.fm and does the work, writing nothing for a preview. Blocking parts run apart.
async fn run(serve: &Arc<crate::serve::Serve>, account: &str, user: &str, auth: &Auth, choice: Choice, preview: bool, progress: &Progress) -> Result<Imported> {
    let dir = serve.data_dir.clone().context("the history needs Cratarr's data folder")?;
    let (scrobbles, loved) = scrobbles_and_loves(account, user, auth, progress).await?;
    progress("Matching", 0, None);
    let (serve, account, user) = (serve.clone(), account.to_string(), user.to_string());
    tokio::task::spawn_blocking(move || -> Result<Imported> {
        let songs = Matcher::read(&Path::new(&dir).join("library.sqlite"))?;
        let listens: Vec<Listened> = scrobbles.iter().map(|s| Listened { track: track(s), at: s.at, event: event(s) }).collect();
        // The scrobbles only when chosen (they are still read: the loved tracks' albums come from them).
        let scrobbled = if choice.scrobbles { into_history(&serve, &songs, &account, CONTEXT, "Last.fm", &listens, preview)? } else { Tally::default() };
        // Loved: stars, as an import keeps them; one loved there no more is unstarred here (where it is still ours).
        let key = ImportKey { account: &account, source: SOURCE, server_user: &user };
        let mut l = Counter::default();
        let mut found: HashMap<String, Option<i64>> = HashMap::new();
        let loved_missing = wanted(&loved, |s| songs.find(&track(s)).is_some(), &albums_of(&scrobbles));
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
        let loved = [s("Bonobo", "Kerala", None), s("bonobo", "kerala", None), s("Air", "Alone in Kyoto", None), s("Air", "Venus", None)];
        let got = wanted(&loved, |x| x.title == "Venus", &albums_of(&scrobbles));
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
