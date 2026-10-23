//! Listening history from ListenBrainz (decision 0064, amended 2026-10-23: the owner chose Jellyfin's history to fill
//! the log; Jellyfin kept none, and its ListenBrainz plugin sent every listen there). An account's listens are read
//! from ListenBrainz's public API (no token: a user's listens are public), each matched to a song of Cratarr's library
//! and logged as heard: finished, not counted (the plays they were reached the counts by Jellyfin's import already),
//! context `listenbrainz`. A listen's own id (its time and recording) makes an import safe to run again.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::userdata::{Ended, Heard};

const API: &str = "https://api.listenbrainz.org/1/user";
/// ListenBrainz's largest page.
const PAGE: usize = 1000;

/// One listen as ListenBrainz keeps it.
#[derive(Debug, Clone, PartialEq)]
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

/// Every listen of a ListenBrainz user, newest first, a page at a time.
async fn fetch(user: &str) -> Result<Vec<Listen>> {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?;
    let mut out = Vec::new();
    let mut before: Option<i64> = None;
    loop {
        let mut url = reqwest::Url::parse(API)?;
        url.path_segments_mut().map_err(|()| anyhow::anyhow!("no path"))?.extend([user, "listens"]);
        url.query_pairs_mut().append_pair("count", &PAGE.to_string());
        if let Some(ts) = before {
            url.query_pairs_mut().append_pair("max_ts", &ts.to_string());
        }
        let page: Page = http.get(url).send().await?.error_for_status().context("ListenBrainz did not answer")?.json().await?;
        let n = page.payload.listens.len();
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
        // The next page: listens before the oldest so far (two at one second are rare; one may be missed then).
        match out.last() {
            Some(last) if n == PAGE => before = Some(last.at),
            _ => return Ok(out),
        }
    }
}

/// A name to match by: lower case, letters and digits only.
fn key(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The library's songs, to match listens to: by recording, by artist and title, by album and title.
#[derive(Default)]
struct Songs {
    by_recording: HashMap<String, String>,
    by_artist: HashMap<(String, String), String>,
    by_album: HashMap<(String, String), String>,
}

impl Songs {
    fn read(library: &Path) -> Result<Self> {
        let conn = crate::db::open_read_only(library)?;
        let mut st = conn.prepare("SELECT id, title, artist, album, mbid FROM songs")?;
        let mut s = Songs::default();
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?))
        })?;
        for row in rows {
            let (id, title, artist, album, mbid) = row?;
            s.add(&id, &title, artist.as_deref(), &album, mbid.as_deref());
        }
        Ok(s)
    }

    fn add(&mut self, id: &str, title: &str, artist: Option<&str>, album: &str, mbid: Option<&str>) {
        if let Some(m) = mbid.filter(|m| !m.is_empty()) {
            self.by_recording.entry(m.to_lowercase()).or_insert_with(|| id.to_string());
        }
        if let Some(a) = artist {
            self.by_artist.entry((key(a), key(title))).or_insert_with(|| id.to_string());
        }
        self.by_album.entry((key(album), key(title))).or_insert_with(|| id.to_string());
    }

    /// The song a listen was of: its recording's, else its artist's and title's, else its album's and title's.
    fn of(&self, l: &Listen) -> Option<&String> {
        l.recording
            .as_ref()
            .and_then(|r| self.by_recording.get(&r.to_lowercase()))
            .or_else(|| self.by_artist.get(&(key(&l.artist), key(&l.title))))
            .or_else(|| l.release.as_ref().and_then(|r| self.by_album.get(&(key(r), key(&l.title)))))
    }
}

/// What an import did.
#[derive(Debug, Default, Serialize)]
pub struct Imported {
    /// Listens ListenBrainz has.
    pub listens: usize,
    /// Of those, matched to a song of the library.
    pub matched: usize,
    /// Of those, new to the log (the rest were there from an earlier import).
    pub added: usize,
}

/// Imports a ListenBrainz user's listens into `account`'s log (decision 0064). Blocking parts run apart.
pub async fn import(serve: &Arc<crate::serve::Serve>, account: &str, user: &str) -> Result<Imported> {
    let dir = serve.data_dir.clone().context("the history needs Cratarr's data folder")?;
    let listens = fetch(user).await?;
    let (serve, account) = (serve.clone(), account.to_string());
    tokio::task::spawn_blocking(move || -> Result<Imported> {
        let songs = Songs::read(&Path::new(&dir).join("library.sqlite"))?;
        let mut done = Imported { listens: listens.len(), ..Imported::default() };
        for l in &listens {
            let Some(song) = songs.of(l) else { continue };
            done.matched += 1;
            let Some((pid, album, artists)) = serve.library.play_targets(song)? else { continue };
            let secs = l.duration_ms.map(|d| d as f64 / 1000.0);
            let heard = Heard {
                at: l.at,
                heard_secs: secs,
                length_secs: secs,
                ended: Ended::Finished,
                counts: false,
                player: l.player.clone(),
                output: None,
                context: Some("listenbrainz".into()),
                event: Some(format!("listenbrainz:{}:{}", l.at, l.msid)),
            };
            if serve.userdata.record(&account, (song, &pid), &album, &artists, &heard)? {
                done.added += 1;
            }
        }
        Ok(done)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listen(title: &str, artist: &str, release: Option<&str>, recording: Option<&str>) -> Listen {
        Listen {
            at: 1,
            msid: "m".into(),
            recording: recording.map(str::to_string),
            artist: artist.into(),
            title: title.into(),
            release: release.map(str::to_string),
            duration_ms: None,
            player: None,
        }
    }

    #[test]
    fn a_listen_finds_its_song_by_recording_then_artist_then_album() {
        let mut s = Songs::default();
        s.add("tr-1", "These Fruits, These Vegetables", Some("Photay"), "Two EPs", Some("F4F4"));
        s.add("tr-2", "Untitled", Some("DJ Healer"), "Prisoner of Love", None);
        s.add("tr-3", "Intro", None, "Some Album", None);
        assert_eq!(s.of(&listen("Other name", "Other", None, Some("f4f4"))), Some(&"tr-1".to_string()), "by recording, any case");
        assert_eq!(s.of(&listen("untitled", "DJ  Healer", None, None)), Some(&"tr-2".to_string()), "by artist and title, punctuation aside");
        assert_eq!(s.of(&listen("Intro", "Someone", Some("Some Album"), None)), Some(&"tr-3".to_string()), "by album and title");
        assert_eq!(s.of(&listen("Nothing", "Nobody", None, None)), None);
    }
}
