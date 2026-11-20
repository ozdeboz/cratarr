//! An album matched with a MusicBrainz release (0.9.7, the owner: as Picard does): its own release where its files name
//! one, else the releases MusicBrainz finds by its name and artist, each scored on its tracks: as many, each found by its
//! title with the closest length. Certain where every track is within `CERTAIN_SECS` and every title its own; likely where
//! every track is found and most lengths are close. What a match gives: the album artist, the release's and the tracks'
//! MusicBrainz ids (`fixes`: written only when an administrator applies them). Matching asks MusicBrainz a few times an
//! album, so it runs as a task, each album's result kept (`UserData::album_match`) and asked again after `AGAIN`.

use std::collections::HashSet;

use anyhow::Result;
use serde_json::Value;

use super::fixes_base::Fix;
use crate::catalogue::identity::RefTrack;
use super::musicbrainz::Asked as _;
pub use super::matching_base::Match;

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
    let mut differ = Vec::new();
    let (mut close, mut near) = (0, 0);
    for (n, (file, title, secs)) in files.iter().enumerate() {
        let best = r
            .tracks
            .iter()
            .enumerate()
            .filter(|(i, t)| !used.contains(i) && super::renumber_base::same_title(title, &t.0))
            .min_by_key(|(_, t)| t.1.map_or(i64::MAX, |l| (l - secs).abs()))?;
        used.insert(best.0);
        let off = best.1.1.map_or(i64::MAX, |l| (l - secs).abs());
        close += usize::from(off <= CERTAIN_SECS);
        near += usize::from(off <= LIKELY_SECS);
        if off > CERTAIN_SECS {
            differ.push((n + 1, *secs, best.1.1));
        }
        tracks.push((file.clone(), best.1.2.clone(), best.1.3.clone()));
        lengths.push(best.1.1);
    }
    let n = files.len();
    // Every track found by its title: certain with every length close, likely with most near.
    let certain = close == n;
    if !certain && near * 5 < n * 4 {
        return None;
    }
    let fits = usize::from(near == n);
    let why = if certain { agrees_words(&r.title, n) } else { likely_words(&r.title, r.year, &r.format, &differ, false) };
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
        format: r.format.clone(),
        fits,
    })
}

/// An edition as a match's words name it (the owner, 2026-11-19): its title, with its year and format where known,
/// "Down the Stairs (2002 CD)". Pure.
fn described(title: &str, year: Option<u32>, format: &str) -> String {
    let more: Vec<String> = [year.map(|y| y.to_string()), Some(format.trim().to_string()).filter(|f| !f.is_empty())].into_iter().flatten().collect();
    if more.is_empty() { title.to_string() } else { format!("{title} ({})", more.join(" ")) }
}

/// A length as the card shows it: "2:00", "10:38".
fn clock(secs: i64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// The tracks whose lengths keep a match from exact (`CERTAIN_SECS`): each `(number, the file's s, the edition's s)` of `tracks`
/// whose lengths differ by more, or that the edition gives no length. Pure.
pub fn differing(tracks: impl IntoIterator<Item = (usize, i64, Option<i64>)>) -> Vec<(usize, i64, Option<i64>)> {
    tracks.into_iter().filter(|(_, secs, theirs)| theirs.is_none_or(|t| (t - secs).abs() > CERTAIN_SECS)).collect()
}

/// A certain match in words (the intake card's Reference line, the fix sheet; the owner, 2026-11-19: what was compared, never
/// MusicBrainz as the authority): "Matches a MusicBrainz edition: <title>, all N tracks". Pure.
pub fn agrees_words(title: &str, n: usize) -> String {
    format!("Matches a MusicBrainz edition: {title}, all {n} tracks")
}

/// A likely match in words (the owner, 2026-11-19: "unreadable"; what was compared, which edition, which tracks differ, the file's
/// length against the edition's, up to three, then how many more): "Compared with MusicBrainz's editions: probably Down the Stairs
/// (2002 CD); track 18's length differs (2:00 against 10:38)". Where several editions fit with every track within a few seconds
/// (`several`), "Compared with MusicBrainz's editions: several fit within a few seconds; none exactly". No "likely", no tolerance in
/// seconds. Pure.
pub fn likely_words(title: &str, year: Option<u32>, format: &str, differ: &[(usize, i64, Option<i64>)], several: bool) -> String {
    const COMPARED: &str = "Compared with MusicBrainz's editions";
    let near = differ.iter().all(|(_, secs, theirs)| theirs.is_some_and(|t| (t - secs).abs() <= LIKELY_SECS));
    if several && near {
        return format!("{COMPARED}: several fit within a few seconds; none exactly");
    }
    let probably = format!("{COMPARED}: probably {}", described(title, year, format));
    let one = |(n, secs, theirs): &(usize, i64, Option<i64>)| match theirs {
        Some(t) => format!("track {n} ({} against {})", clock(*secs), clock(*t)),
        None => format!("track {n} ({}, none at MusicBrainz)", clock(*secs)),
    };
    match differ {
        [] => probably,
        [(n, secs, Some(t))] => format!("{probably}; track {n}'s length differs ({} against {})", clock(*secs), clock(*t)),
        [(n, secs, None)] => format!("{probably}; track {n} has no length at MusicBrainz ({})", clock(*secs)),
        _ => {
            let shown: Vec<String> = differ.iter().take(3).map(one).collect();
            let more = differ.len().saturating_sub(3);
            let listed = if more > 0 { format!("{} and {more} more", shown.join(", ")) } else { format!("{} and {}", shown[..shown.len() - 1].join(", "), shown[shown.len() - 1]) };
            format!("{probably}; lengths differ: {listed}")
        }
    }
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
    let (tracks, release, artist, name) = crate::run::spawn_blocking(move || super::renumber_base::tracks_of(&a)).await??;
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
    // How many editions fit with every track within a few seconds (the words' "several fit").
    let mut fits = 0;
    for id in ids {
        let read = lookup(http, &id).await?;
        let Some(m) = score(&files, &read) else { continue };
        fits += m.fits;
        if best.as_ref().is_none_or(|(b, ..)| m.certain && !b.certain) {
            let certain = m.certain;
            best = Some((m, read.list, false));
            if certain {
                break;
            }
        }
    }
    Ok(best.map(|(mut m, list, named)| {
        m.fits = fits;
        if !m.certain && fits > 1 {
            let differ = differing(m.tracks.iter().zip(&m.lengths).zip(&files).enumerate().map(|(i, ((_, theirs), (_, _, secs)))| (i + 1, *secs, *theirs)));
            m.why = likely_words(&m.title, m.year, &m.format, &differ, true);
        }
        (m, list, named)
    }))
}

/// How the matching task finds an album's match (`find`; the tests' stand in for it, V6).
type Finder = fn(reqwest::Client, String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Found>>> + Send>>;

/// The matches of these albums as kept; those never matched, or matched long ago, matched in a task (one at a time for
/// the account), each kept as it is found. One album (an intake card, a one-album fix) is a page's question (V6); a whole finding's
/// list goes behind the pages, as the background reads do, and stops at "Pause background reading" (U8). Blocking parts apart.
pub async fn kept_or_find(served: &std::sync::Arc<crate::serve::Serve>, albums: Vec<String>) -> Result<Vec<(String, Match)>> {
    kept_or_find_by(served, albums, |http, album| Box::pin(async move { find(&http, &album).await })).await
}

/// `kept_or_find`, each album's match found by `finder`.
async fn kept_or_find_by(served: &std::sync::Arc<crate::serve::Serve>, albums: Vec<String>, finder: Finder) -> Result<Vec<(String, Match)>> {
    let ud = served.userdata.clone();
    let wanted = albums.clone();
    let kept = crate::run::spawn_blocking(move || ud.album_matches(&wanted)).await??;
    let now = super::moment::now_secs();
    let due: Vec<String> = albums.iter().filter(|a| kept.get(a.as_str()).is_none_or(|(at, _)| now - at >= AGAIN)).cloned().collect();
    if !due.is_empty()
        && let Some(account) = served.current_user()
    {
        let (ud, served) = (served.userdata.clone(), served.clone());
        // One running for the account already: it is left to finish (asked again later, it takes the rest).
        // A page started it: one album's questions (an intake card, a one-album fix) are a page's, never the background asker's (V6),
        // so "Pause background reading" does not refuse them (Y7). A whole finding's list (Health's "library:musicbrainz", every album
        // lacking ids) asks behind the pages, as the background reads do, and the pause holds it (U8). One album asked, not one left
        // of many by the matches kept (T13.4): a finding whose other albums are matched already is still the background's.
        let page = albums.len() == 1;
        let _ = crate::runner::spawn(KIND, &account, false, move |h| async move {
            let matching = match_each(h, due, ud, served, finder);
            if page { matching.await } else { super::musicbrainz::behind(matching).await }
        });
    }
    Ok(kept.into_iter().filter_map(|(a, (_, json))| Some((a, serde_json::from_str(&json?).ok()?))).collect())
}

/// The matching task's work (`kept_or_find_by`): each album's match found by `finder` and kept, with the release's own list in its
/// references, as the task's steps say.
async fn match_each(h: std::sync::Arc<crate::runner::Handle>, due: Vec<String>, ud: std::sync::Arc<crate::userdata::UserData>, served: std::sync::Arc<crate::serve::Serve>, finder: Finder) -> Result<()> {
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
        crate::run::spawn_blocking(move || -> Result<()> {
            ud.set_album_match(&album, json.as_deref(), super::moment::now_secs())?;
            // In the references too (C43b), with the release's own list.
            if let Some((m, list, named)) = &found {
                super::references::keep_match(&s, &album, m, list, *named)?;
            }
            Ok(())
        })
        .await??;
    }
    Ok(())
}

/// The fixes a match gives a finding's albums (`library:album-artist`: the album artist; `library:musicbrainz`: the
/// release's and the tracks' ids). `at`: the copy written to, where they are one copy's and not the record's.
pub async fn fixes(served: &std::sync::Arc<crate::serve::Serve>, finding: &str, albums: Vec<String>, at: Option<(i64, String)>) -> Result<Vec<Fix>> {
    let matched = kept_or_find(served, albums).await?;
    let (finding, served) = (finding.to_string(), served.clone());
    crate::run::spawn_blocking(move || -> Result<Vec<Fix>> {
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
fn files_of_match(record: &[super::renumber_base::Track], written: &[super::renumber_base::Track], file: &str) -> Vec<std::path::PathBuf> {
    let track = record.iter().find(|t| t.file == file).map(|t| t.id.as_str());
    let same: Vec<std::path::PathBuf> = written.iter().filter(|t| Some(t.id.as_str()) == track).map(|t| t.path.clone()).collect();
    if same.is_empty() { written.iter().find(|t| t.file == file).map(|t| t.path.clone()).into_iter().collect() } else { same }
}

/// A match's fix of one album, where `finding` has one. Blocking.
pub fn fix_of(served: &crate::serve::Serve, finding: &str, album: &str, m: &Match, at: Option<&(i64, String)>) -> Result<Option<Fix>> {
    let record = super::renumber_base::files_in(served, album, None)?;
    let written = if at.is_some() { super::renumber_base::files_in(served, album, at)? } else { record.clone() };
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
    crate::runner::runner().of(account).iter().any(|t| t.kind == KIND && t.finished.is_none())
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
        let _one = TEST_STATE.write().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        *AT.lock().unwrap() = format!("http://{}/ws/2/release", listener.local_addr().unwrap());
        crate::run::spawn(async move { axum::serve(listener, axum::Router::new().route("/ws/2/release", axum::routing::get(|| async { "{}" }))).await.unwrap() });
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

    /// U8 (the review of 8c/8d, finding 8): a whole finding's matching (Health's "library:musicbrainz", every album lacking ids) asks
    /// behind the pages: while background reading is paused none of its questions go, and nothing is kept. One album alone goes (V6).
    #[tokio::test]
    async fn a_whole_findings_matching_waits_while_paused() {
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
        let _one = TEST_STATE.write().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        *AT.lock().unwrap() = format!("http://{}/ws/2/release", listener.local_addr().unwrap());
        crate::run::spawn(async move { axum::serve(listener, axum::Router::new().route("/ws/2/release", axum::routing::get(|| async { "{}" }))).await.unwrap() });
        let serve = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(std::sync::Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
        const ACCOUNT: &str = "a-findings-matching-while-paused";
        let who = crate::serve::who::Who { user: ACCOUNT.into(), admin: true, scope: crate::serve::who::Scope::Full, home: true };
        let (musicbrainz, background) = (set_for_tests(MUSICBRAINZ, true), set_for_tests(BACKGROUND, false));
        let albums = vec!["al-1".to_string(), "al-2".to_string(), "al-3".to_string()];
        let kept = crate::serve::who::acting_as(who, kept_or_find_by(&serve, albums.clone(), asking)).await;
        for _ in 0..500 {
            if !running(ACCOUNT) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let matched = serve.userdata.album_matches(&albums).unwrap();
        set_for_tests(BACKGROUND, background);
        set_for_tests(MUSICBRAINZ, musicbrainz);
        assert!(kept.unwrap().is_empty());
        assert_eq!(WENT.load(std::sync::atomic::Ordering::SeqCst), 0, "no question went while paused");
        assert!(matched.is_empty(), "nothing kept");
    }

    /// T13.4 (run 10's review, the nits, U8): a whole finding asks behind the pages however many of its albums are matched already:
    /// three asked, two matched a moment ago, one left to ask; while background reading is paused its question does not go, and
    /// nothing is kept of it.
    #[tokio::test]
    async fn a_finding_with_one_album_left_to_match_still_waits_while_paused() {
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
        let _one = TEST_STATE.write().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        *AT.lock().unwrap() = format!("http://{}/ws/2/release", listener.local_addr().unwrap());
        crate::run::spawn(async move { axum::serve(listener, axum::Router::new().route("/ws/2/release", axum::routing::get(|| async { "{}" }))).await.unwrap() });
        let serve = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(std::sync::Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
        const ACCOUNT: &str = "a-findings-last-album-while-paused";
        let who = crate::serve::who::Who { user: ACCOUNT.into(), admin: true, scope: crate::serve::who::Scope::Full, home: true };
        let albums = vec!["al-1".to_string(), "al-2".to_string(), "al-3".to_string()];
        for a in &albums[..2] {
            serve.userdata.set_album_match(a, None, crate::app::moment::now_secs()).unwrap();
        }
        let (musicbrainz, background) = (set_for_tests(MUSICBRAINZ, true), set_for_tests(BACKGROUND, false));
        let kept = crate::serve::who::acting_as(who, kept_or_find_by(&serve, albums.clone(), asking)).await;
        for _ in 0..500 {
            if !running(ACCOUNT) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let matched = serve.userdata.album_matches(&albums).unwrap();
        set_for_tests(BACKGROUND, background);
        set_for_tests(MUSICBRAINZ, musicbrainz);
        assert!(kept.unwrap().is_empty(), "none kept with a release");
        assert_eq!(WENT.load(std::sync::atomic::Ordering::SeqCst), 0, "the last album's question did not go while paused");
        assert!(!matched.contains_key("al-3"), "nothing kept of it");
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

    /// The owner (2026-11-19): the Reference words say what was compared (MusicBrainz's editions), never MusicBrainz as the
    /// authority; a likely match names its edition and the tracks whose lengths differ, the file's against the edition's, up to
    /// three, then how many more; several editions fitting within a few seconds say so. No "likely", no "ref", no tolerance in seconds.
    #[test]
    fn a_matchs_words_say_what_was_compared_and_which_tracks_differ() {
        let compared = "Compared with MusicBrainz's editions";
        assert_eq!(likely_words("Down the Stairs", Some(2002), "CD", &[(18, 120, Some(638))], false), format!("{compared}: probably Down the Stairs (2002 CD); track 18's length differs (2:00 against 10:38)"));
        assert_eq!(likely_words("Ebb", None, "", &[(4, 61, None)], false), format!("{compared}: probably Ebb; track 4 has no length at MusicBrainz (1:01)"));
        assert_eq!(
            likely_words("Neap", Some(1998), "Digital Media", &[(2, 200, Some(205)), (7, 250, Some(241))], false),
            format!("{compared}: probably Neap (1998 Digital Media); lengths differ: track 2 (3:20 against 3:25) and track 7 (4:10 against 4:01)")
        );
        let five: Vec<(usize, i64, Option<i64>)> = (1..=5).map(|n| (n, 100, Some(110))).collect();
        assert_eq!(
            likely_words("Neap", Some(2002), "CD", &five, false),
            format!("{compared}: probably Neap (2002 CD); lengths differ: track 1 (1:40 against 1:50), track 2 (1:40 against 1:50), track 3 (1:40 against 1:50) and 2 more")
        );
        assert_eq!(likely_words("Neap", Some(2002), "CD", &[(2, 200, Some(204))], true), format!("{compared}: several fit within a few seconds; none exactly"));
        assert!(likely_words("Neap", Some(2002), "CD", &[(2, 120, Some(638))], true).contains("probably Neap"), "several, but one track far: which and why");
        assert_eq!(agrees_words("Down the Stairs", 18), "Matches a MusicBrainz edition: Down the Stairs, all 18 tracks");
        assert_eq!(differing([(1, 100, Some(102)), (2, 100, Some(103)), (3, 100, None)]), [(2, 100, Some(103)), (3, 100, None)], "within 2 s it matches");
        let files = vec![("01.flac".to_string(), "Angel".to_string(), 379), ("02.flac".into(), "Risingson".into(), 298)];
        let likely = score(&files, &Release { year: Some(1998), format: "CD".into(), ..release(&[("Angel", 384), ("Risingson", 298)]) }).unwrap();
        assert_eq!(likely.why, format!("{compared}: probably Mezzanine (1998 CD); track 1's length differs (6:19 against 6:24)"));
        assert_eq!(likely.fits, 1, "every track within a few seconds");
        for word in ["likely", "within 5", "ref"] {
            assert!(!likely.why.contains(word), "{}", likely.why);
        }
    }

    #[test]
    fn a_search_phrase_is_escaped() {
        assert_eq!(phrase(r#"He said "hi"\"#), r#""He said \"hi\"\\""#);
    }
}
