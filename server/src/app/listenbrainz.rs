//! Listening history from ListenBrainz (decision 0064, amended 2026-10-23: the owner chose Jellyfin's history to fill
//! the log; Jellyfin kept none, and its ListenBrainz plugin sent every listen there). An account's listens are read
//! from ListenBrainz's public API (no token: a user's listens are public), each matched to a song of Cratarr's library
//! and logged as heard: finished, not counted (the plays they were reached the counts by Jellyfin's import already),
//! context `listenbrainz`. A listen's own id (its time and recording) makes an import safe to run again. One import
//! (decision 0067): found by the shared matcher (`app::import`), previewed before, undone as a whole (by its context).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::import::{Counter, Matcher, Tally, Track};
use crate::userdata::{Ended, Heard};
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
        let there = serve.userdata.heard_events(&account, CONTEXT)?;
        let mut c = Counter::default();
        for l in &listens {
            match songs.find(&track(l)) {
                Some(_) => c.found(!there.contains(&event(l))),
                None => c.missed(&track(l)),
            }
        }
        Ok(c.tally())
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
        let mut c = Counter::default();
        for l in &listens {
            let Some(song) = songs.find(&track(l)) else {
                c.missed(&track(l));
                continue;
            };
            let Some((pid, album, artists)) = serve.library.play_targets(song)? else {
                c.missed(&track(l));
                continue;
            };
            let secs = l.duration_ms.map(|d| d as f64 / 1000.0);
            let heard = Heard {
                at: l.at,
                heard_secs: secs,
                length_secs: secs,
                ended: Ended::Finished,
                counts: false,
                player: l.player.clone(),
                output: None,
                context: Some(CONTEXT.into()),
                event: Some(event(l)),
                followed: false,
            };
            c.found(serve.userdata.record(&account, (song, &pid), &album, &artists, &heard)?);
        }
        Ok(c.tally())
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
