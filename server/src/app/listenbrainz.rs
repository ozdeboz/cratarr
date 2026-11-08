//! Listening history from ListenBrainz (decision 0064, amended 2026-10-23: the owner chose Jellyfin's history to fill
//! the log; Jellyfin kept none, and its ListenBrainz plugin sent every listen there). An account's listens are read
//! from ListenBrainz's public API (no token: a user's listens are public), each matched to a song of Cratarr's library
//! and logged as heard: finished, not counted (the plays they were reached the counts by Jellyfin's import already),
//! context `listenbrainz`. A listen's own id (its time and recording) makes an import safe to run again. One import
//! (decision 0067): found by the shared matcher (`app::import`), previewed before, undone as a whole (by its context).
//! It goes in by the one routine for listens (`import::into_history`): one the history has from anywhere is not added
//! twice, and that includes every listen Cratarr itself sent to ListenBrainz.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::import::{Listened, Matcher, Tally, Track, into_history};
use super::reach::Sent as _;

const API: &str = "https://api.listenbrainz.org/1/user";
/// ListenBrainz's largest page.
const PAGE: usize = 1000;

/// One listen as ListenBrainz keeps it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Listen {
    pub at: i64,
    pub msid: String,
    /// MusicBrainz's recording, where ListenBrainz mapped the listen to one.
    pub recording: Option<String>,
    pub artist: String,
    pub title: String,
    pub release: Option<String>,
    pub duration_ms: Option<i64>,
    /// The player it was heard on ("Jellyfin").
    pub player: Option<String>,
}

#[derive(Deserialize)]
struct Page {
    payload: Payload,
}

#[derive(Deserialize)]
struct Payload {
    listens: Vec<Raw>,
}

#[derive(Deserialize)]
struct Raw {
    listened_at: i64,
    #[serde(default)]
    recording_msid: String,
    track_metadata: Meta,
}

#[derive(Deserialize)]
struct Meta {
    #[serde(default)]
    artist_name: String,
    #[serde(default)]
    track_name: String,
    release_name: Option<String>,
    mbid_mapping: Option<Mapping>,
    additional_info: Option<Info>,
}

#[derive(Deserialize)]
struct Mapping {
    recording_mbid: Option<String>,
}

#[derive(Deserialize)]
struct Info {
    duration_ms: Option<i64>,
    media_player: Option<String>,
    recording_mbid: Option<String>,
}

/// Every listen of a ListenBrainz user, newest first, a page at a time, each page kept in `spool` as it comes; one that
/// holds a read stopped before goes on from its oldest (0.9.7). `progress` says how many so far.
async fn fetch(user: &str, progress: &super::lastfm::Progress, spool: &mut super::resume::Spool<Listen>) -> Result<Vec<Listen>> {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?;
    let mut out = Vec::new();
    let mut before: Option<i64> = spool.items.iter().map(|l| l.at).min();
    loop {
        let mut url = reqwest::Url::parse(API)?;
        url.path_segments_mut().map_err(|()| anyhow::anyhow!("no path"))?.extend([user, "listens"]);
        url.query_pairs_mut().append_pair("count", &PAGE.to_string());
        if let Some(ts) = before {
            url.query_pairs_mut().append_pair("max_ts", &ts.to_string());
        }
        let page: Page = http.get(url).sent().await?.error_for_status().context("ListenBrainz did not answer")?.json().await?;
        let n = page.payload.listens.len();
        let from = out.len();
        for r in page.payload.listens {
            let m = r.track_metadata;
            let info = m.additional_info.unwrap_or(Info { duration_ms: None, media_player: None, recording_mbid: None });
            out.push(Listen {
                at: r.listened_at,
                msid: r.recording_msid,
                recording: m.mbid_mapping.and_then(|x| x.recording_mbid).or(info.recording_mbid).filter(|x| !x.is_empty()),
                artist: m.artist_name,
                title: m.track_name,
                release: m.release_name.filter(|x| !x.is_empty()),
                duration_ms: info.duration_ms.filter(|d| *d > 0),
                player: info.media_player.filter(|x| !x.is_empty()),
            });
        }
        spool.add(out[from..].iter().cloned())?;
        progress("Reading listens", (spool.items.len()) as u64, None);
        // The next page: listens before the oldest so far (two at one second are rare; one may be missed then).
        match out.last() {
            Some(last) if n == PAGE => before = Some(last.at),
            _ => return Ok(spool.items.clone()),
        }
    }
}

/// A listen's track, for the matcher.
fn track(l: &Listen) -> Track<'_> {
    Track {
        recording: l.recording.as_deref(),
        artist: &l.artist,
        title: &l.title,
        album: l.release.as_deref(),
        length_secs: l.duration_ms.map(|d| d as f64 / 1000.0),
    }
}

/// The history's own id for a listen: its time and ListenBrainz's id for it (safe to import again).
fn event(l: &Listen) -> String {
    format!("{CONTEXT}:{}:{}", l.at, l.msid)
}

/// What the history calls an imported listen's context: how its undo finds it.
const CONTEXT: &str = "listenbrainz";

/// The listens through the one routine for listens (decision 0067): what they bring to `account`'s history, written
/// unless `preview`. Blocking.
fn into_log(serve: &crate::serve::Serve, songs: &Matcher, account: &str, listens: &[Listen], preview: bool) -> Result<Tally> {
    let listened: Vec<Listened> = listens.iter().map(|l| Listened { track: track(l), at: l.at, event: event(l), player: l.player.as_deref() }).collect();
    into_history(serve, songs, account, CONTEXT, "ListenBrainz", &listened, preview)
}

/// What importing a ListenBrainz user's listens into `account`'s history would bring (decision 0067): nothing written.
pub async fn preview(serve: &Arc<crate::serve::Serve>, account: &str, user: &str, progress: &super::lastfm::Progress) -> Result<Tally> {
    let dir = serve.data_dir.clone().context("the history needs Cratarr's data folder")?;
    // Its read kept too: an import after it goes on from there.
    let mut spool = super::resume::Spool::<Listen>::open(Path::new(&dir), CONTEXT, account, user);
    let listens = fetch(user, progress, &mut spool).await?;
    // Read whole: the import after it reads again (and the new listens with it).
    spool.finish();
    progress("Matching", 0, None);
    let (serve, account) = (serve.clone(), account.to_string());
    tokio::task::spawn_blocking(move || -> Result<Tally> {
        let songs = Matcher::read(&Path::new(&dir).join("library.sqlite"))?;
        into_log(&serve, &songs, &account, &listens, true)
    })
    .await?
}

/// Takes out every listen imported from ListenBrainz into `account`'s history (decision 0067): how many.
pub fn undo(serve: &crate::serve::Serve, account: &str) -> Result<usize> {
    serve.userdata.forget_heard(account, CONTEXT)
}

/// Imports a ListenBrainz user's listens into `account`'s history (decisions 0064 and 0067): what it brought. Blocking
/// parts run apart.
pub async fn import(serve: &Arc<crate::serve::Serve>, account: &str, user: &str, progress: &super::lastfm::Progress) -> Result<Tally> {
    let dir = serve.data_dir.clone().context("the history needs Cratarr's data folder")?;
    let mut spool = super::resume::Spool::<Listen>::open(Path::new(&dir), CONTEXT, account, user);
    // Stopped part of the way (0.9.7): the listens read so far go in now, and the next import goes on from there.
    let (listens, stopped) = match fetch(user, progress, &mut spool).await {
        Ok(listens) => (listens, None),
        Err(e) => (spool.items.clone(), Some(e)),
    };
    if stopped.is_none() {
        spool.finish();
    }
    progress("Matching", 0, None);
    let (serve, account) = (serve.clone(), account.to_string());
    let tally = tokio::task::spawn_blocking(move || -> Result<Tally> {
        let songs = Matcher::read(&Path::new(&dir).join("library.sqlite"))?;
        into_log(&serve, &songs, &account, &listens, false)
    })
    .await?;
    match stopped {
        None => tally,
        Some(e) => {
            let n = listens_in(&tally);
            Err(e.context(format!("the {n} listens read before it stopped are in; the next import goes on from where it stopped")))
        }
    }
}

/// How many of a tally's listens were found in the library (what went in).
fn listens_in(t: &Result<Tally>) -> u64 {
    t.as_ref().map_or(0, |t| t.found as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::{Catalogue, Rules, ServedRow};
    use crate::serve::library::FolderRows;
    use crate::userdata::Heard;

    /// One song served, Hunter by Björk; its id and a matcher that knows it.
    fn one_song(name: &str) -> (std::path::PathBuf, Arc<crate::serve::Serve>, String, Matcher) {
        let d = std::env::temp_dir().join(format!("cratarr-listenbrainz-{name}-{}", std::process::id()));
        let file = "Björk/Homogenic (1997)/01 Hunter.mp3";
        std::fs::create_dir_all(d.join(file).parent().unwrap()).unwrap();
        std::fs::write(d.join(file), file).unwrap();
        let cat = Arc::new(Catalogue::open(None, vec![d.clone()]).unwrap());
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        let track = cat.track_of(0, file).unwrap();
        let serve = crate::serve::Serve::open(None, Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(cat);
        let tags = [("title", "Hunter"), ("artist", "Björk"), ("albumartist", "Björk"), ("album", "Homogenic"), ("date", "1997"), ("tracknumber", "1")];
        let row = ServedRow { rel: file.into(), track: Some(track), size: 1, mtime: 1, tags: tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), ..ServedRow::default() };
        serve.library.refresh(&[FolderRows { folder: 0, root: d.display().to_string(), rows: vec![row] }], &Rules::default()).unwrap();
        let song = crate::serve::ids::song(track);
        let mut songs = Matcher::default();
        songs.add(&song, "Hunter", Some("Björk"), "Homogenic", None, Some(250.0));
        (d, serve, song, songs)
    }

    fn listen(at: i64, msid: &str, player: Option<&str>) -> Listen {
        Listen { at, msid: msid.into(), recording: None, artist: "Björk".into(), title: "Hunter".into(), release: None, duration_ms: Some(250_000), player: player.map(Into::into) }
    }

    #[test]
    fn a_listen_cratarr_sent_comes_back_as_the_play_it_was_not_a_second_one() {
        let (d, serve, song, songs) = one_song("sent");
        let (pid, album, artists) = serve.library.play_targets(&song).unwrap().unwrap();
        // A live play: no event of ListenBrainz's to know it by, as `plays` is unique on nothing else.
        serve.userdata.record("me", (&song, &pid), &album, &artists, &Heard { event: Some("the-players-own".into()), ..Heard::counted(1_000) }).unwrap();
        let back = [listen(1_000, "m1", Some("Cratarr"))];
        let said = into_log(&serve, &songs, "me", &back, true).unwrap();
        assert_eq!((said.found, said.new), (1, 0), "the preview says it is there");
        let brought = into_log(&serve, &songs, "me", &back, false).unwrap();
        assert_eq!((brought.found, brought.new), (1, 0));
        let log = serve.userdata.heard_since("me", 0).unwrap();
        assert_eq!(log.len(), 1, "one row, Insights' plays and the taste as they were");
        assert_eq!(undo(&serve, "me").unwrap(), 0, "and nothing an Undo would take");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_listen_the_history_has_not_comes_in_once_however_often_it_is_imported() {
        let (d, serve, _, songs) = one_song("new");
        let old = [listen(1_000, "m1", Some("Jellyfin")), listen(9_000, "m2", None)];
        assert_eq!(into_log(&serve, &songs, "me", &old, true).unwrap().new, 2, "a preview writes nothing");
        assert!(serve.userdata.heard_since("me", 0).unwrap().is_empty());
        assert_eq!(into_log(&serve, &songs, "me", &old, false).unwrap().new, 2);
        assert_eq!(into_log(&serve, &songs, "me", &old, true).unwrap().new, 0, "run again it brings nothing");
        assert_eq!(into_log(&serve, &songs, "me", &old, false).unwrap().new, 0);
        let log = serve.userdata.heard_since("me", 0).unwrap();
        assert_eq!(log.iter().map(|l| (l.at, l.counted, l.context.as_deref())).collect::<Vec<_>>(), [(1_000, false, Some(CONTEXT)), (9_000, false, Some(CONTEXT))], "finished, not counted, its context how its Undo finds it");
        assert_eq!(undo(&serve, "me").unwrap(), 2);
        std::fs::remove_dir_all(&d).ok();
    }
}
