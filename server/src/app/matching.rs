//! An album matched with a MusicBrainz release (0.9.7, the owner: as Picard does): its own release where its files name
//! one, else the releases MusicBrainz finds by its name and artist, each scored on its tracks: as many, each found by its
//! title with the closest length. Certain where every track is within `CERTAIN_SECS` and every title its own; likely where
//! every track is found and most lengths are close. What a match gives: the album artist, the release's and the tracks'
//! MusicBrainz ids (`fixes`: written only when an administrator applies them). Matching asks MusicBrainz a few times an
//! album, so it runs as a task, each album's result kept (`UserData::album_match`) and asked again after `AGAIN`.

use std::collections::HashSet;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::fixes::Fix;
use crate::catalogue::identity::RefTrack;
use super::musicbrainz::Asked as _;

/// The task's kind (`app::tasks`).
pub const KIND: &str = "match";
/// Every track this close to its release's length makes a certain match (s); this close, a likely one.
const CERTAIN_SECS: i64 = 2;
const LIKELY_SECS: i64 = 5;
/// Of the releases a search finds, how many are read (each a request).
const CANDIDATES: usize = 3;
/// A search's score below this is no candidate (MusicBrainz's 0 to 100).
const LEAST_SCORE: i64 = 80;
/// A match is looked for again after this long.
const AGAIN: i64 = 30 * 86_400;

/// An album's match, as kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Match {
    pub release: String,
    pub release_group: Option<String>,
    pub title: String,
    /// Its artist credit as MusicBrainz writes it, and its first artist's id.
    pub artist: String,
    pub artist_id: Option<String>,
    pub year: Option<u32>,
    pub certain: bool,
    pub why: String,
    /// Each of the album's files (by name) with its recording and its first artist's id.
    pub tracks: Vec<(String, String, Option<String>)>,
    /// Each one's length on the release (s), as `tracks` (the intake card's reference lengths, 0.14).
    #[serde(default)]
    pub lengths: Vec<Option<i64>>,
}

/// A release as matching reads it.
#[derive(Debug, Clone, Default)]
struct Release {
    id: String,
    group: Option<String>,
    title: String,
    artist: String,
    artist_id: Option<String>,
    year: Option<u32>,
    format: String,
    /// Each track: title, length (s), recording id, first artist's id.
    tracks: Vec<(String, Option<i64>, String, Option<String>)>,
    /// Its list as a reference keeps it (`reference_tracks`): MusicBrainz's own titles, lengths to the ms, recordings, each
    /// at its medium and place. A match keeps none of it, only its files' names.
    list: Vec<RefTrack>,
}

fn credit(v: &Value) -> (String, Option<String>) {
    let parts = v.as_array().cloned().unwrap_or_default();
    let name = parts.iter().map(|c| format!("{}{}", c["name"].as_str().unwrap_or_default(), c["joinphrase"].as_str().unwrap_or_default())).collect::<String>();
    (name.trim().to_string(), parts.first().and_then(|c| c["artist"]["id"].as_str()).map(str::to_string))
}

fn release_of(v: &Value) -> Release {
    let (artist, artist_id) = credit(&v["artist-credit"]);
    let media = v["media"].as_array().cloned().unwrap_or_default();
    let list = media
        .iter()
        .zip(1..)
        .flat_map(|(m, disc)| {
            let disc = m["position"].as_u64().map_or(disc, |p| p as u32);
            m["tracks"].as_array().cloned().unwrap_or_default().into_iter().zip(1..).map(move |(t, at)| RefTrack {
                disc,
                position: t["position"].as_u64().map_or(at, |p| p as u32),
                title: t["title"].as_str().unwrap_or_default().to_string(),
                length_ms: t["length"].as_i64(),
                recording: t["recording"]["id"].as_str().map(str::to_string),
            })
        })
        .collect();
    Release {
        list,
        id: v["id"].as_str().unwrap_or_default().to_string(),
        group: v["release-group"]["id"].as_str().map(str::to_string),
        title: v["title"].as_str().unwrap_or_default().to_string(),
        artist,
        artist_id,
        year: v["date"].as_str().and_then(|d| d.get(..4)).and_then(|y| y.parse().ok()),
        format: media.first().and_then(|m| m["format"].as_str()).unwrap_or_default().to_string(),
        tracks: media
            .iter()
            .flat_map(|m| m["tracks"].as_array().cloned().unwrap_or_default())
            .map(|t| {
                let (_, by) = credit(&t["artist-credit"]);
                (
                    t["title"].as_str().unwrap_or_default().to_string(),
                    t["length"].as_i64().map(|ms| (ms + 500) / 1000),
                    t["recording"]["id"].as_str().unwrap_or_default().to_string(),
                    by,
                )
            })
            .collect(),
    }
}

/// How the album's tracks (file name, title, length s) meet a release's: each file at the release's track of the same
/// title, the closest length first, each track once. None where a file finds none or the counts differ.
fn score(files: &[(String, String, i64)], r: &Release) -> Option<Match> {
    if files.is_empty() || files.len() != r.tracks.len() {
        return None;
    }
    let mut used = HashSet::new();
    let mut tracks = Vec::with_capacity(files.len());
    let mut lengths = Vec::with_capacity(files.len());
    let (mut close, mut near) = (0, 0);
    for (file, title, secs) in files {
        let best = r
            .tracks
            .iter()
            .enumerate()
            .filter(|(i, t)| !used.contains(i) && super::renumber::same_title(title, &t.0))
            .min_by_key(|(_, t)| t.1.map_or(i64::MAX, |l| (l - secs).abs()))?;
        used.insert(best.0);
        let off = best.1.1.map_or(i64::MAX, |l| (l - secs).abs());
        close += usize::from(off <= CERTAIN_SECS);
        near += usize::from(off <= LIKELY_SECS);
        tracks.push((file.clone(), best.1.2.clone(), best.1.3.clone()));
        lengths.push(best.1.1);
    }
    let n = files.len();
    // Every track found by its title: certain with every length close, likely with most near.
    let certain = close == n;
    if !certain && near * 5 < n * 4 {
        return None;
    }
    let why = if certain {
        format!("{} · all {n} tracks within {CERTAIN_SECS} s", described(r))
    } else {
        format!("{} · {near} of {n} tracks within {LIKELY_SECS} s", described(r))
    };
    Some(Match {
        release: r.id.clone(),
        release_group: r.group.clone(),
        title: r.title.clone(),
        artist: r.artist.clone(),
        artist_id: r.artist_id.clone(),
        year: r.year,
        certain,
        why,
        tracks,
        lengths,
    })
}

fn described(r: &Release) -> String {
    [Some(r.title.clone()), r.year.map(|y| y.to_string()), Some(r.format.clone()).filter(|f| !f.is_empty())].into_iter().flatten().collect::<Vec<_>>().join(", ")
}

/// A Lucene phrase, its quotes and backslashes escaped.
fn phrase(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

async fn lookup(http: &reqwest::Client, release: &str) -> Result<Release> {
    let v: Value = http
        .get(format!("https://musicbrainz.org/ws/2/release/{release}?inc=recordings+artist-credits+release-groups&fmt=json"))
        .asked()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(release_of(&v))
}

/// What `find` found: the match, the release's own list, and whether the files name that release.
type Found = (Match, Vec<RefTrack>, bool);

/// An album's best match: its own release where its files name one, else the best of what a search by its name and
/// artist finds (certain before likely, the first found among equals).
async fn find(http: &reqwest::Client, album: &str) -> Result<Option<Found>> {
    let a = album.to_string();
    let (tracks, release, artist, name) = tokio::task::spawn_blocking(move || super::renumber::tracks_of(&a)).await??;
    let files: Vec<(String, String, i64)> = tracks.iter().map(|t| (t.file.clone(), t.title.clone(), i64::from(t.secs))).collect();
    if let Some(r) = release {
        let read = lookup(http, &r).await?;
        return Ok(score(&files, &read).map(|m| (m, read.list, true)));
    }
    let query = format!("release:{} AND artist:{}", phrase(&name), phrase(&artist));
    let mut url = reqwest::Url::parse("https://musicbrainz.org/ws/2/release")?;
    url.query_pairs_mut().append_pair("query", &query).append_pair("limit", "10").append_pair("fmt", "json");
    let v: Value = http.get(url).asked().await?.error_for_status()?.json().await?;
    let ids: Vec<String> = v["releases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["score"].as_i64().unwrap_or(0) >= LEAST_SCORE && r["track-count"].as_u64().is_none_or(|n| n as usize == files.len()))
        .filter_map(|r| r["id"].as_str().map(str::to_string))
        .take(CANDIDATES)
        .collect();
    let mut best: Option<Found> = None;
    for id in ids {
        let read = lookup(http, &id).await?;
        if let Some(m) = score(&files, &read)
            && best.as_ref().is_none_or(|(b, ..)| m.certain && !b.certain)
        {
            let certain = m.certain;
            best = Some((m, read.list, false));
            if certain {
                break;
            }
        }
    }
    Ok(best)
}

/// How the matching task finds an album's match (`find`; the tests' stand in for it, V6).
type Finder = fn(reqwest::Client, String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Found>>> + Send>>;

/// The matches of these albums as kept; those never matched, or matched long ago, matched in a task (one at a time for
/// the account), each kept as it is found. Blocking parts apart.
pub async fn kept_or_find(served: &std::sync::Arc<crate::serve::Serve>, albums: Vec<String>) -> Result<Vec<(String, Match)>> {
    kept_or_find_by(served, albums, |http, album| Box::pin(async move { find(&http, &album).await })).await
}

/// `kept_or_find`, each album's match found by `finder`.
async fn kept_or_find_by(served: &std::sync::Arc<crate::serve::Serve>, albums: Vec<String>, finder: Finder) -> Result<Vec<(String, Match)>> {
    let ud = served.userdata.clone();
    let wanted = albums.clone();
    let kept = tokio::task::spawn_blocking(move || ud.album_matches(&wanted)).await??;
    let now = super::moment::now_secs();
    let due: Vec<String> = albums.iter().filter(|a| kept.get(a.as_str()).is_none_or(|(at, _)| now - at >= AGAIN)).cloned().collect();
    if !due.is_empty()
        && let Some(account) = served.current_user()
    {
        let (ud, served) = (served.userdata.clone(), served.clone());
        // One running for the account already: it is left to finish (asked again later, it takes the rest).
        // A page started it (an intake card, Health's fixes): its questions are a page's, never the background asker's (V6), so
        // "Pause background reading" does not refuse them (Y7).
        let _ = super::tasks::spawn(KIND, &account, false, move |h| async move {
            let http = super::discover::http()?;
            let total = due.len() as u64;
            for (i, album) in due.into_iter().enumerate() {
                h.step("Matching albums with MusicBrainz", i as u64, Some(total));
                let found = match finder(http.clone(), album.clone()).await {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::debug!("matching: {album}: {e:#}");
                        continue;
                    }
                };
                let (ud, s, json) = (ud.clone(), served.clone(), found.as_ref().map(|(m, ..)| serde_json::to_string(m)).transpose()?);
                tokio::task::spawn_blocking(move || -> Result<()> {
                    ud.set_album_match(&album, json.as_deref(), super::moment::now_secs())?;
                    // In the references too (C43b), with the release's own list.
                    if let Some((m, list, named)) = &found {
                        super::references::keep_match(&s, &album, m, list, *named)?;
                    }
                    Ok(())
                })
                .await??;
            }
            anyhow::Ok(())
        });
    }
    Ok(kept.into_iter().filter_map(|(a, (_, json))| Some((a, serde_json::from_str(&json?).ok()?))).collect())
}

/// The fixes a match gives a finding's albums (`library:album-artist`: the album artist; `library:musicbrainz`: the
/// release's and the tracks' ids). `at`: the copy written to, where they are one copy's and not the record's.
pub async fn fixes(served: &std::sync::Arc<crate::serve::Serve>, finding: &str, albums: Vec<String>, at: Option<(i64, String)>) -> Result<Vec<Fix>> {
    let matched = kept_or_find(served, albums).await?;
    let (finding, served) = (finding.to_string(), served.clone());
    tokio::task::spawn_blocking(move || -> Result<Vec<Fix>> {
        let mut out = Vec::new();
        for (album, m) in matched {
            out.extend(fix_of(&served, &finding, &album, &m, at.as_ref())?);
        }
        Ok(out)
    })
    .await?
}

/// The files a matched file's fields go to. A match names the files of the copy it was made on, and copies name their files
/// as they like, so what links them is the track: every file of it among `written`. A file on a track of its own there goes
/// by its name, as nothing else says what it is.
fn files_of_match(record: &[super::renumber::Track], written: &[super::renumber::Track], file: &str) -> Vec<std::path::PathBuf> {
    let track = record.iter().find(|t| t.file == file).map(|t| t.id.as_str());
    let same: Vec<std::path::PathBuf> = written.iter().filter(|t| Some(t.id.as_str()) == track).map(|t| t.path.clone()).collect();
    if same.is_empty() { written.iter().find(|t| t.file == file).map(|t| t.path.clone()).into_iter().collect() } else { same }
}

/// A match's fix of one album, where `finding` has one. Blocking.
pub(super) fn fix_of(served: &crate::serve::Serve, finding: &str, album: &str, m: &Match, at: Option<&(i64, String)>) -> Result<Option<Fix>> {
    let record = super::renumber::files_in(served, album, None)?;
    let written = if at.is_some() { super::renumber::files_in(served, album, at)? } else { record.clone() };
    let certainty = if m.certain { "certain" } else { "likely" };
    Ok(Some(match finding {
        "library:album-artist" if !m.artist.is_empty() => Fix {
            id: format!("albumartist:{album}"),
            album: album.to_string(),
            field: "albumartist",
            from: None,
            to: m.artist.clone(),
            source: "MusicBrainz".into(),
            certainty,
            why: m.why.clone(),
            writes: written.iter().map(|t| (t.path.clone(), vec![("albumartist", Some(m.artist.clone()))])).collect(),
            candidates: Vec::new(),
        },
        "library:musicbrainz" => Fix {
            id: format!("musicbrainz:{album}"),
            album: album.to_string(),
            field: "musicbrainz",
            from: None,
            to: m.release.clone(),
            source: "MusicBrainz".into(),
            certainty,
            why: m.why.clone(),
            writes: m
                .tracks
                .iter()
                .flat_map(|(file, recording, artist)| {
                    files_of_match(&record, &written, file).into_iter().map(move |path| {
                        (
                            path,
                            vec![
                                ("musicbrainz_albumid", Some(m.release.clone())),
                                ("musicbrainz_releasegroupid", m.release_group.clone()),
                                ("musicbrainz_albumartistid", m.artist_id.clone()),
                                ("musicbrainz_trackid", Some(recording.clone()).filter(|r| !r.is_empty())),
                                ("musicbrainz_artistid", artist.clone()),
                            ],
                        )
                    })
                })
                .collect(),
            candidates: Vec::new(),
        },
        _ => return Ok(None),
    }))
}

/// Whether the account's matching runs now.
pub fn running(account: &str) -> bool {
    super::tasks::tasks().of(account).iter().any(|t| t.kind == KIND && t.finished.is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tracks: &[(&str, i64)]) -> Release {
        Release {
            id: "r1".into(),
            title: "Mezzanine".into(),
            artist: "Massive Attack".into(),
            tracks: tracks.iter().enumerate().map(|(i, (t, s))| (t.to_string(), Some(*s), format!("rec{i}"), Some("a1".into()))).collect(),
            ..Release::default()
        }
    }

    /// V6 (the review of run 8b, finding 6): the matching an intake card or Health's fixes start is a page's: while background reading
    /// is paused its questions to MusicBrainz still go, and what it found is kept.
    #[tokio::test]
    async fn an_intake_cards_match_goes_while_paused() {
        use crate::app::switches::{BACKGROUND, MUSICBRAINZ, TEST_STATE, set_for_tests};
        static AT: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
        static WENT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        fn asking(_: reqwest::Client, _: String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Found>>> + Send>> {
            use crate::app::musicbrainz::Asked as _;
            Box::pin(async {
                let at = AT.lock().unwrap().clone();
                reqwest::Client::new().get(at).asked().await?.error_for_status()?;
                WENT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                anyhow::Ok(None::<Found>)
            })
        }
        let _one = TEST_STATE.lock().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        *AT.lock().unwrap() = format!("http://{}/ws/2/release", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, axum::Router::new().route("/ws/2/release", axum::routing::get(|| async { "{}" }))).await.unwrap() });
        let serve = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(std::sync::Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
        const ACCOUNT: &str = "matching-while-paused";
        let who = crate::serve::who::Who { user: ACCOUNT.into(), admin: true, scope: crate::serve::who::Scope::Full, home: true };
        let (musicbrainz, background) = (set_for_tests(MUSICBRAINZ, true), set_for_tests(BACKGROUND, false));
        let kept = crate::serve::who::acting_as(who, kept_or_find_by(&serve, vec!["al-1".to_string()], asking)).await;
        for _ in 0..500 {
            if !running(ACCOUNT) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let matched = serve.userdata.album_matches(&["al-1".to_string()]).unwrap();
        set_for_tests(BACKGROUND, background);
        set_for_tests(MUSICBRAINZ, musicbrainz);
        assert!(kept.unwrap().is_empty(), "none kept before");
        assert_eq!(WENT.load(std::sync::atomic::Ordering::SeqCst), 1, "its question went while paused");
        assert!(matched.contains_key("al-1"), "what it found kept");
    }

    /// A match keeps its files' names; the release's list keeps MusicBrainz's own titles, to the ms, at their medium and place
    /// (a track with no place says its order), each with its recording.
    #[test]
    fn a_releases_list_is_its_own_titles() {
        let v = serde_json::json!({ "id": "r1", "media": [
            { "position": 1, "tracks": [{ "position": 1, "title": "Slack Water", "length": 181_234, "recording": { "id": "rec1" } }] },
            { "position": 2, "tracks": [{ "position": 1, "title": "Neap", "recording": { "id": "rec2" } }, { "title": "Ebb", "length": 90_000 }] }
        ]});
        let list = release_of(&v).list;
        let rec = |disc: u32, position: u32, title: &str, length_ms: Option<i64>, recording: Option<&str>| RefTrack { disc, position, title: title.into(), length_ms, recording: recording.map(str::to_string) };
        assert_eq!(list, [rec(1, 1, "Slack Water", Some(181_234), Some("rec1")), rec(2, 1, "Neap", None, Some("rec2")), rec(2, 2, "Ebb", Some(90_000), None)]);
    }

    #[test]
    fn a_match_is_certain_with_every_track_close_likely_with_most() {
        let files = vec![("01.flac".to_string(), "Angel".to_string(), 379), ("02.flac".into(), "Risingson".into(), 298)];
        let m = score(&files, &release(&[("Risingson", 299), ("Angel", 380)])).unwrap();
        assert!(m.certain, "by title, not order: {m:?}");
        assert_eq!(m.tracks[0], ("01.flac".to_string(), "rec1".to_string(), Some("a1".to_string())));
        let far = score(&files, &release(&[("Angel", 384), ("Risingson", 302)])).unwrap();
        assert!(!far.certain, "both within 5 s, not 2");
        assert!(score(&files, &release(&[("Angel", 400), ("Risingson", 330)])).is_none(), "most lengths far");
        assert!(score(&files, &release(&[("Angel", 379)])).is_none(), "another number of tracks");
        assert!(score(&files, &release(&[("Angel", 379), ("Teardrop", 298)])).is_none(), "a title not found");
    }

    #[test]
    fn a_search_phrase_is_escaped() {
        assert_eq!(phrase(r#"He said "hi"\"#), r#""He said \"hi\"\\""#);
    }
}
