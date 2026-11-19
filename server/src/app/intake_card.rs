//! The intake card (0.14, the owner, 2026-11-02): a record waiting in an Incoming library, checked before Keep. Its
//! header (title, artist, year, format, where it came from), its checks on top (each green, amber or red with one
//! sentence), its tracks under (length beside the reference's, a small waveform, where to hear its start, middle and
//! end), and what Cratarr suggests: Keep where every check is green and AcoustID agrees. Nothing is written or moved
//! before the yes (Keep, Replace or Delete, as Incoming has them; one journal Undo).
//!
//! What is slow to learn is heard once per record in the background and kept (`intake_heard`): each track decoded
//! whole (frames that would not decode, clipping, clicks, a lossy wall) and AcoustID's recordings for it. A card shows
//! those checks waiting until then.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The task that hears records (`app::tasks`).
pub const KIND: &str = "intake:hear";
/// How many of a waveform's bins the card carries.
const PEAKS: usize = 48;
/// The end's snippet starts this long before the end (ms).
const END_MS: i64 = 20_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Card {
    pub source: String,
    pub album: String,
    pub title: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    /// Its library (the Incoming it waits in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    pub from: From,
    /// "FLAC 24/96", "FLAC 16/44.1 · MP3 320".
    pub format: String,
    pub checks: Vec<Check>,
    pub tracks: Vec<CardTrack>,
    /// "keep" where every check is green and AcoustID agrees; else none, the checks saying why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested: Option<&'static str>,
}

/// Where a record came from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct From {
    /// soulseek, lidarr, bandcamp, folder.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    pub said: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Check {
    /// reference, acoustid, sound, one_record, folder_name, one_user, in_library.
    pub key: &'static str,
    /// green, amber, red; waiting while it is still being heard or looked up.
    pub state: &'static str,
    pub said: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CardTrack {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length_ms: Option<i64>,
    /// The reference's length for it (MusicBrainz's release, as matched).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_ms: Option<i64>,
    /// Its waveform, a few bins (0–255), where it was analysed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peaks: Option<Vec<u8>>,
    pub snippets: Snippets,
}

/// Where to hear a track's start (its sound's first, after silence), middle and end (ms).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Snippets {
    pub start: i64,
    pub middle: i64,
    pub end: i64,
}

/// What hearing a record's copy found, kept per copy (`intake_heard`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Heard {
    pub tracks: Vec<HeardTrack>,
    /// AcoustID was not asked (no key set).
    #[serde(default)]
    pub no_acoustid: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HeardTrack {
    /// Its file's name.
    pub file: String,
    pub damaged: u32,
    /// What its sound was heard to have wrong with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flaw: Option<String>,
    /// Where its first flaw is (ms), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flaw_at_ms: Option<i64>,
    /// The recordings AcoustID is sure of (its score at least `acoustid::SURE`).
    #[serde(default)]
    pub recordings: Vec<String>,
    /// It could not be read at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
}

/// One track as a card reads it.
struct Track {
    id: String,
    path: PathBuf,
    /// Its catalogue folder and its path in it.
    place: (i64, String),
    file: String,
    title: String,
    ms: i64,
    disc: Option<u32>,
    no: Option<u32>,
}

impl std::convert::From<super::renumber::Track> for Track {
    fn from(t: super::renumber::Track) -> Self {
        Track { id: t.id, file: t.file, title: t.title, ms: i64::from(t.secs) * 1000, disc: t.disc, no: t.track, path: t.path, place: t.place }
    }
}

/// The tracks a card is about: its copy's, at (catalogue folder, folder in it), whichever copy of the record plays.
fn tracks_of(album: &str, at: Option<&(i64, String)>) -> Result<(Vec<Track>, String, String)> {
    let (tracks, _, artist, title) = super::renumber::tracks_in(album, at)?;
    Ok((tracks.into_iter().map(Track::from).collect(), artist, title))
}

/// A record's format, as its files are: each kind once ("FLAC 24/96 · MP3 320").
fn format_of(files: &[crate::catalogue::tags::FileTags]) -> String {
    let said: BTreeSet<String> = files
        .iter()
        .map(|t| {
            let kind = t.format.clone().unwrap_or_else(|| "?".into());
            match (t.bit_depth, t.sample_rate_hz, t.bitrate_kbps) {
                (Some(b), Some(r), _) => format!("{kind} {b}/{}", khz(r)),
                (None, _, Some(k)) => format!("{kind} {k}"),
                _ => kind,
            }
        })
        .collect();
    said.into_iter().collect::<Vec<_>>().join(" · ")
}

fn khz(hz: u32) -> String {
    let k = f64::from(hz) / 1000.0;
    if k.fract() == 0.0 { format!("{k:.0}") } else { format!("{k:.1}") }
}

/// Where to hear a track: its sound's start (the first bin above a tenth of its loudest), its middle, and a little
/// before its end.
pub fn snippets(waveform: &[u8], ms: i64) -> Snippets {
    let loudest = waveform.iter().copied().max().unwrap_or(0);
    let start = match waveform.iter().position(|b| loudest > 0 && *b >= loudest / 10) {
        Some(i) if !waveform.is_empty() => ms * i as i64 / waveform.len() as i64,
        _ => 0,
    };
    let middle = ms / 2;
    Snippets { start, middle, end: (ms - END_MS).max(middle) }
}

/// A waveform's few bins: each the loudest of its share.
fn peaks_of(waveform: &[u8]) -> Option<Vec<u8>> {
    if waveform.is_empty() {
        return None;
    }
    let per = waveform.len().div_ceil(PEAKS);
    Some(waveform.chunks(per).map(|c| c.iter().copied().max().unwrap_or(0)).collect())
}

/// One sentence of values that should be one ("Two album names: A, B").
fn one_of(what: &str, values: &BTreeSet<String>) -> Option<String> {
    (values.len() > 1).then(|| format!("{} {what}: {}", values.len(), values.iter().cloned().collect::<Vec<_>>().join(", ")))
}

/// Text as compared: lower case, letters and digits only.
fn folded(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The checks that read only the files (one record; its folder's name as its tags say). Blocking.
fn file_checks(tracks: &[Track], tags: &[crate::catalogue::tags::FileTags]) -> Vec<Check> {
    let field = |name: &str| -> BTreeSet<String> {
        tags.iter().filter_map(|t| t.items.iter().find(|(_, n, _)| *n == Some(name)).map(|(_, _, v)| v.trim().to_string())).filter(|v| !v.is_empty()).collect()
    };
    let folders: BTreeSet<String> = tracks.iter().filter_map(|t| t.path.parent().map(|p| p.display().to_string())).collect();
    let years: BTreeSet<String> = field("date").into_iter().filter_map(|d| d.get(..4).map(str::to_string)).collect();
    let albums = field("album");
    let artists = field("albumartist");
    let off: Vec<String> = [one_of("folders", &folders), one_of("album names", &albums), one_of("album artists", &artists), one_of("years", &years)].into_iter().flatten().collect();
    let mut out = vec![match off.is_empty() {
        true => Check { key: "one_record", state: "green", said: "One folder, one album, one album artist, one year".into() },
        false => Check { key: "one_record", state: "amber", said: off.join("; ") },
    }];
    let folder = tracks.first().and_then(|t| t.path.parent()).and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some(album) = albums.iter().next() {
        out.push(match folded(&folder).contains(&folded(album)) {
            true => Check { key: "folder_name", state: "green", said: format!("The folder is named as the tags say: {folder}") },
            false => Check { key: "folder_name", state: "amber", said: format!("The folder is named {folder}; the tags say {album}") },
        });
    }
    out
}

/// What hearing found, as the sound's and AcoustID's checks (waiting where not heard yet).
fn heard_checks(heard: Option<&Heard>, tracks: &[Track], matched: Option<&super::matching::Match>) -> (Check, Check) {
    let Some(h) = heard else {
        let waiting = |key, said: &str| Check { key, state: "waiting", said: said.into() };
        return (waiting("sound", "Being heard"), waiting("acoustid", "Being heard"));
    };
    let place = |file: &str| tracks.iter().find(|t| t.file == file).and_then(|t| t.no).map_or_else(|| file.to_string(), |n| format!("Track {n}"));
    let mut red = Vec::new();
    let mut amber = Vec::new();
    for t in &h.tracks {
        if let Some(why) = &t.failed {
            red.push(format!("{}: {why}", place(&t.file)));
        } else if t.damaged > 0 {
            red.push(format!("{}: {} frames would not decode", place(&t.file), t.damaged));
        } else if let Some(f) = &t.flaw {
            amber.push(format!("{}: {f}", place(&t.file)));
        }
    }
    let sound = match (red.is_empty(), amber.is_empty()) {
        (true, true) => Check { key: "sound", state: "green", said: "Every track decodes whole, without clipping or clicks".into() },
        (false, _) => Check { key: "sound", state: "red", said: red.into_iter().chain(amber).collect::<Vec<_>>().join("; ") },
        (true, false) => Check { key: "sound", state: "amber", said: amber.join("; ") },
    };
    let acoustid = if h.no_acoustid {
        Check { key: "acoustid", state: "amber", said: "AcoustID is not set up: Settings › Server".into() }
    } else {
        // Each track known as its matched recording, else as any recording AcoustID is sure of.
        let m = h.tracks.len();
        let n = h
            .tracks
            .iter()
            .filter(|t| match matched.and_then(|mm| mm.tracks.iter().find(|(f, ..)| *f == t.file)) {
                Some((_, rec, _)) => t.recordings.iter().any(|r| r.eq_ignore_ascii_case(rec)),
                None => !t.recordings.is_empty(),
            })
            .count();
        let state = if n == m { "green" } else if n * 2 >= m { "amber" } else { "red" };
        Check { key: "acoustid", state, said: format!("AcoustID knows {n} of {m} recordings{}", if matched.is_some() { " as the release has them" } else { "" }) }
    };
    (sound, acoustid)
}

/// The reference's check: MusicBrainz's release as matched; else what Discogs knows; waiting while it is looked for.
/// The card's Reference line (the owner, 2026-11-19: "unreadable"; what was compared, so MusicBrainz never reads as the authority:
/// judging by every source equally comes with 0.18's matcher). A certain match: "Matches a MusicBrainz edition: <title>, all N
/// tracks". A likely one: "Compared with MusicBrainz's editions: probably <edition>; track 18's length differs (2:00 against
/// 10:38)", by the card's own tracks, so a match kept before reads so too; several editions fitting within a few seconds say so.
/// Only Discogs: "Only Discogs knows it (1999)". None: "No source knows it". Pure.
fn reference_check(matched: Option<&super::matching::Match>, looking: bool, discogs: Option<&super::discogs::AlbumFacts>, tracks: &[Track]) -> Check {
    let n = tracks.len();
    match (matched, discogs) {
        (Some(m), _) if m.certain => Check { key: "reference", state: "green", said: super::matching::agrees_words(&m.title, n) },
        (Some(m), _) => {
            let theirs = |t: &Track| m.tracks.iter().position(|(f, ..)| *f == t.file).and_then(|i| m.lengths.get(i).copied().flatten());
            let differ = super::matching::differing(tracks.iter().enumerate().map(|(i, t)| (t.no.map_or(i + 1, |no| no as usize), (t.ms + 500) / 1000, theirs(t))));
            Check { key: "reference", state: "amber", said: super::matching::likely_words(&m.title, m.year, &m.format, &differ, m.fits > 1) }
        }
        (None, _) if looking => Check { key: "reference", state: "waiting", said: "Being looked for on MusicBrainz".into() },
        (None, Some(d)) => Check { key: "reference", state: "amber", said: format!("Only Discogs knows it{}", d.year.map(|y| format!(" ({y})")).unwrap_or_default()) },
        (None, None) => Check { key: "reference", state: "red", said: "No source knows it".into() },
    }
}

/// Where the record came from: Bandcamp where its files say so (Bandcamp's own comment, "Visit https://….bandcamp.com"),
/// the Soulseek user its files were downloaded from (slskd's transfers), Lidarr where it imported its folder; else a
/// folder.
async fn from_of(app: &super::App, files: &[String], folder: &str, tags: &[crate::catalogue::tags::FileTags]) -> From {
    let bandcamp = tags.iter().any(|t| t.items.iter().any(|(k, _, v)| k.to_ascii_lowercase().contains("comm") && v.contains(".bandcamp.com")));
    if bandcamp {
        return From { kind: "bandcamp", user: None, said: "Bandcamp".into() };
    }
    if let Some(user) = soulseek_user(files).await {
        return From { kind: "soulseek", said: format!("Soulseek · {user}"), user: Some(user) };
    }
    if lidarr_folders(app).await.is_some_and(|f| f.iter().any(|x| x.eq_ignore_ascii_case(folder))) {
        return From { kind: "lidarr", user: None, said: "Lidarr".into() };
    }
    From { kind: "folder", user: None, said: "A folder".into() }
}

/// How long slskd's downloads and Lidarr's imports are kept for the cards before they are asked again.
const ASKED_FOR: std::time::Duration = std::time::Duration::from_secs(60);

/// slskd's downloads by user, asked at most once a minute (none where slskd is not set up or not reached).
async fn slskd_downloads() -> Option<Arc<Vec<(String, Vec<String>)>>> {
    static KEPT: std::sync::Mutex<Option<(std::time::Instant, Arc<Vec<(String, Vec<String>)>>)>> = std::sync::Mutex::new(None);
    if let Some((at, d)) = KEPT.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && at.elapsed() < ASKED_FOR
    {
        return Some(d.clone());
    }
    let served = crate::serve::instance()?;
    let ud = served.userdata.clone();
    let set = tokio::task::spawn_blocking(move || super::track_fetch::slskd_set(&ud)).await.ok()?.ok()??;
    let d = Arc::new(crate::slskd::Slskd::new(&set.url, &set.key).ok()?.downloads().await.ok()?);
    *KEPT.lock().unwrap_or_else(|e| e.into_inner()) = Some((std::time::Instant::now(), d.clone()));
    Some(d)
}

/// The folders Lidarr imported lately, asked at most once a minute.
async fn lidarr_folders(app: &super::App) -> Option<Arc<Vec<String>>> {
    static KEPT: std::sync::Mutex<Option<(std::time::Instant, Arc<Vec<String>>)>> = std::sync::Mutex::new(None);
    if let Some((at, f)) = KEPT.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && at.elapsed() < ASKED_FOR
    {
        return Some(f.clone());
    }
    let f = Arc::new(super::discover::lidarr(app)?.imported_folders().await.ok()?);
    *KEPT.lock().unwrap_or_else(|e| e.into_inner()) = Some((std::time::Instant::now(), f.clone()));
    Some(f)
}

/// The Soulseek user most of these files came from, by slskd's downloads (none where slskd is not set up or does not
/// remember them).
async fn soulseek_user(files: &[String]) -> Option<String> {
    let downloads = slskd_downloads().await?;
    let mut best: Option<(usize, String)> = None;
    for (user, names) in downloads.iter() {
        let n = files.iter().filter(|f| names.iter().any(|x| crate::slskd::base_name(x).eq_ignore_ascii_case(f))).count();
        if n * 2 > files.len() && best.as_ref().is_none_or(|(b, _)| n > *b) {
            best = Some((n, user.clone()));
        }
    }
    best.map(|(_, u)| u)
}

/// How a record's copy waiting stands in the library (C37a, OQ1; RF2): against the copy that plays the record for most, else another
/// record's copy in a main library its tracks pair with one for one (`Serve::stands_of`), whether it is better, that copy's catalogue
/// folder and why; else the folder of a main library holding another edition of it by its names. Blocking.
fn stands_of(served: &crate::serve::Serve, album: &str, at: Option<&(i64, String)>, mains: &[i64]) -> Result<(Option<(bool, i64, String)>, Option<i64>)> {
    use crate::serve::copies::why;
    let copies = served.library.copies_of(album)?;
    let kept = match at.and_then(|(f, d)| copies.iter().find(|c| c.folder == *f && &c.dir == d)) {
        Some(c) => served.stands_of(&[(album.to_string(), c.clone())], mains)?.pop().flatten().map(|(better, kept)| (better, kept.folder, why(c, &kept))),
        None => None,
    };
    if kept.is_some() {
        return Ok((kept, None));
    }
    let edition = match crate::serve::ids::record_number(album) {
        Some(record) => served.catalogue().other_edition_in(record, mains)?,
        None => None,
    };
    Ok((None, edition))
}

/// The card's line on the library (C37a; OQ1, the owner: two records, say so): kept elsewhere, better than the copy that plays it
/// for most or not, that copy named by its library; else another edition a main library has by name, with no action; else not in
/// the library yet. Pure.
fn in_library(kept: Option<(bool, String, &str)>, edition: Option<String>) -> Check {
    let (state, said) = match (kept, edition) {
        (Some((true, library, why)), _) => ("amber", format!("Better than the copy in {library}: {why}")),
        (Some((false, library, why)), _) => ("red", format!("Already in {library}: {why}")),
        (None, Some(library)) => ("amber", format!("{library} has another edition")),
        (None, None) => ("green", "Not in your library yet".to_string()),
    };
    Check { key: "in_library", state, said }
}

/// A record's card. `album`: Cratarr's own album id; `at`: its copy waiting, as (catalogue folder, folder in it), which is what
/// the card reads and hears, whichever copy plays; `library`: Incoming's for it. The hearing is started where it was never done.
pub async fn card(app: &Arc<super::App>, album: &str, at: Option<(i64, String)>, library: Option<String>) -> Result<Card> {
    let served = app.served.clone().context("Cratarr's own library is not open")?;
    let a = album.to_string();
    let (s, a2, at2) = (served.clone(), a.clone(), at.clone());
    #[allow(clippy::type_complexity)]
    let (tracks, artist, title, tags, waves, heard, discogs): (Vec<Track>, String, String, Vec<crate::catalogue::tags::FileTags>, Vec<Vec<u8>>, Option<Heard>, Option<super::discogs::AlbumFacts>) =
        tokio::task::spawn_blocking(move || -> Result<_> {
            let (tracks, artist, title) = tracks_of(&a2, at2.as_ref())?;
            let tags = tracks.iter().map(|t| crate::catalogue::tags::read(&t.path).map(|(f, _)| f).unwrap_or_default()).collect();
            let cat = s.catalogue();
            let waves = tracks.iter().map(|t| cat.analysis_of(t.place.0, &t.place.1).map(|an| an.waveform).unwrap_or_default()).collect();
            let heard = s.userdata.intake_heard(&a2, at2.as_ref().map(|p| p.0))?.and_then(|(_, j)| serde_json::from_str(&j).ok());
            let discogs = super::discogs_facts::album_kept(&s, &a2).ok().flatten();
            Ok((tracks, artist, title, tags, waves, heard, discogs))
        })
        .await??;
    let matched = super::matching::kept_or_find(&served, vec![a.clone()]).await?.into_iter().next().map(|(_, m)| m);
    let looking = matched.is_none() && super::tasks::tasks().of(&served.current_user().unwrap_or_default()).iter().any(|t| t.kind == super::matching::KIND && t.finished.is_none());
    if heard.is_none() {
        hear(app, &a, at.clone());
    }
    let files: Vec<String> = tracks.iter().map(|t| t.file.clone()).collect();
    let folder = tracks.first().and_then(|t| t.path.parent()).and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let from = from_of(app, &files, &folder, &tags).await;
    let mut checks = vec![reference_check(matched.as_ref(), looking, discogs.as_ref(), &tracks)];
    let (sound, acoustid) = heard_checks(heard.as_ref(), &tracks, matched.as_ref());
    checks.push(acoustid);
    checks.push(sound);
    checks.extend(file_checks(&tracks, &tags));
    if from.kind == "soulseek" {
        checks.push(Check { key: "one_user", state: "green", said: format!("From one Soulseek user's folder: {}", from.user.clone().unwrap_or_default()) });
    }
    let (kept, edition) = {
        let (s, a, at, mains) = (served.clone(), album.to_string(), at.clone(), app.library.main_folders());
        tokio::task::spawn_blocking(move || stands_of(&s, &a, at.as_ref(), &mains)).await??
    };
    let name = |folder: i64| app.incoming.library_name(crate::source::cratarr::KIND.id, folder);
    checks.push(in_library(kept.as_ref().map(|(better, folder, why)| (*better, name(*folder), why.as_str())), edition.map(name)));
    let all_green = checks.iter().all(|c| c.state == "green");
    let year = tags.iter().find_map(|t| t.items.iter().find(|(_, n, _)| *n == Some("date")).and_then(|(_, _, v)| v.get(..4)?.parse().ok()));
    let card_tracks = tracks
        .iter()
        .zip(&waves)
        .map(|(t, w)| CardTrack {
            id: t.id.clone(),
            disc: t.disc,
            position: t.no,
            title: t.title.clone(),
            length_ms: (t.ms > 0).then_some(t.ms),
            reference_ms: matched.as_ref().and_then(|m| m.tracks.iter().position(|(f, ..)| *f == t.file).and_then(|i| m.lengths.get(i).copied().flatten())).map(|s| s * 1000),
            peaks: peaks_of(w),
            snippets: snippets(w, t.ms),
        })
        .collect();
    Ok(Card {
        source: crate::source::cratarr::KIND.id.to_string(),
        album: a,
        title,
        artist,
        year,
        library,
        from,
        format: format_of(&tags),
        checks,
        tracks: card_tracks,
        suggested: all_green.then_some("keep"),
    })
}

/// A record waiting, as the list of them says it (its card opens on its own: `card`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Waiting {
    pub source: String,
    pub album: String,
    pub title: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    /// upgrade or duplicate, where it is in the library already.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<&'static str>,
}

/// Every record waiting in an Incoming library of Cratarr's own, as Incoming lists them: no card worked out (a card is
/// asked for one record; 0.14's preview.2: working out the cards of every Incoming library's records, 800 in one,
/// took minutes).
pub async fn waiting(app: &super::App) -> Vec<Waiting> {
    let view = app.incoming.list(None).await;
    let mut out = Vec::new();
    let mut places = Vec::new();
    for s in view.sources.iter().filter(|s| s.id == crate::source::cratarr::KIND.id) {
        if let Some(e) = &s.error {
            tracing::warn!(target: "library", reason = %e, "records waiting not listed");
        }
        for a in &s.albums {
            places.push((a.id.clone(), a.copies.first().map(|c| (c.folder, c.dir.clone()))));
            out.push(Waiting { source: s.id.clone(), album: a.id.clone(), title: a.title.clone(), artist: a.artist.clone().unwrap_or_default(), library: a.library.clone(), verdict: None });
        }
    }
    // Each one's stand against a copy kept elsewhere (C37a; RF2: another record's, paired track for track): an upgrade where Replace
    // would take it, else in two libraries.
    if let Some(served) = app.served.clone() {
        use crate::serve::copies::replaces;
        let mains = app.library.main_folders();
        let stands = tokio::task::spawn_blocking(move || {
            let found: Vec<Option<(String, crate::serve::copies::Copy)>> = places
                .iter()
                .map(|(id, at)| {
                    let c = served.library.copies_of(id).ok()?.into_iter().find(|c| at.as_ref().is_some_and(|(f, d)| c.folder == *f && &c.dir == d))?;
                    Some((id.clone(), c))
                })
                .collect();
            let asked: Vec<(String, crate::serve::copies::Copy)> = found.iter().flatten().cloned().collect();
            let mut stood = served.stands_of(&asked, &mains).unwrap_or_else(|e| {
                tracing::warn!(target: "library", reason = %format!("{e:#}"), "how records waiting stand not read");
                vec![None; asked.len()]
            });
            stood.reverse();
            found
                .iter()
                .map(|f| f.as_ref().and_then(|_| stood.pop().flatten()).map(|(better, kept)| if replaces(better, kept.kind) { "upgrade" } else { "duplicate" }))
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        for (w, v) in out.iter_mut().zip(stands) {
            w.verdict = v;
        }
    }
    out
}

/// Hears a record's copy in the background, once (one task at a time for the account; another asked meanwhile waits for its
/// next card): each track decoded whole and its recordings asked of AcoustID, kept under that copy.
pub fn hear(app: &Arc<super::App>, album: &str, at: Option<(i64, String)>) {
    let Some(served) = app.served.clone() else { return };
    let Some(account) = served.current_user() else { return };
    if super::tasks::tasks().of(&account).iter().any(|t| t.kind == KIND && t.finished.is_none()) {
        return;
    }
    let album = album.to_string();
    let _ = super::tasks::spawn(KIND, &account, false, move |h| async move {
        let (tracks, ..) = tokio::task::spawn_blocking({
            let (a, at) = (album.clone(), at.clone());
            move || tracks_of(&a, at.as_ref())
        })
        .await??;
        let ud = served.userdata.clone();
        let key = tokio::task::spawn_blocking(move || super::track_fetch::acoustid_key(&ud)).await??;
        let mut heard = Heard { no_acoustid: key.is_none(), ..Heard::default() };
        let total = tracks.len() as u64;
        for (i, t) in tracks.iter().enumerate() {
            h.step("Hearing the record's tracks", i as u64, Some(total));
            let path = t.path.clone();
            let mut one = HeardTrack { file: t.file.clone(), ..HeardTrack::default() };
            match tokio::task::spawn_blocking(move || crate::acoustid::print(&path)).await? {
                Ok(print) => {
                    one.damaged = print.damaged;
                    one.flaw = print.flaws.why();
                    one.flaw_at_ms = print.flaws.clicks.first().copied().or(print.flaws.first_clip).map(|s| (s * 1000.0) as i64);
                    if let Some(k) = &key {
                        match crate::acoustid::lookup(k, &print).await {
                            Ok(found) => one.recordings = found.iter().filter(|f| f.score >= crate::acoustid::SURE).flat_map(|f| f.recordings.iter().map(|r| r.id.clone())).collect(),
                            Err(e) => tracing::debug!("intake: AcoustID for {}: {e:#}", t.file),
                        }
                    }
                }
                Err(e) => one.failed = Some(super::plain(format!("{e:#}"))),
            }
            heard.tracks.push(one);
        }
        let (ud, json, a) = (served.userdata.clone(), serde_json::to_string(&heard)?, album.clone());
        let folder = at.map(|p| p.0);
        tokio::task::spawn_blocking(move || ud.set_intake_heard(&a, folder, super::moment::now_secs(), &json)).await??;
        anyhow::Ok(())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The owner (2026-11-19): the Reference line says what was compared, never MusicBrainz as the authority. A likely match names its
    /// edition and the tracks whose lengths differ, by the card's own track numbers, the file's length against the edition's; several
    /// editions fitting within a few seconds say so; a certain one says all its tracks match; only Discogs, and none, as said. No
    /// "likely", no "ref", no tolerance in seconds. A match kept before (no format, no count of editions) reads so too.
    #[test]
    fn the_reference_line_says_what_was_compared_and_which_tracks_differ() {
        let track = |no: u32, ms: i64| Track { id: format!("tr-{no}"), path: PathBuf::from(format!("{no:02}.flac")), place: (0, format!("A/{no:02}.flac")), file: format!("{no:02}.flac"), title: format!("Track {no}"), ms, disc: Some(1), no: Some(no) };
        let tracks: Vec<Track> = (1..=18).map(|no| track(no, if no == 18 { 120_000 } else { 200_000 })).collect();
        let m = super::super::matching::Match {
            release: "r1".into(),
            release_group: None,
            title: "Down the Stairs".into(),
            artist: "Kite Theory".into(),
            artist_id: None,
            year: Some(2002),
            certain: false,
            why: "Down the Stairs, 2002, CD · 17 of 18 tracks within 5 s".into(),
            tracks: tracks.iter().map(|t| (t.file.clone(), String::new(), None)).collect(),
            lengths: (1..=18).map(|no| Some(if no == 18 { 638 } else { 200 })).collect(),
            format: "CD".into(),
            fits: 0,
        };
        let line = |c: Check| (c.state, c.said);
        let probably = "Compared with MusicBrainz's editions: probably Down the Stairs (2002 CD); track 18's length differs (2:00 against 10:38)";
        assert_eq!(line(reference_check(Some(&m), false, None, &tracks)), ("amber", probably.to_string()));
        let kept_before = super::super::matching::Match { format: String::new(), ..m.clone() };
        assert_eq!(line(reference_check(Some(&kept_before), false, None, &tracks)).1, "Compared with MusicBrainz's editions: probably Down the Stairs (2002); track 18's length differs (2:00 against 10:38)");
        let near: Vec<Track> = (1..=18).map(|no| track(no, if no == 18 { 636_000 } else { 200_000 })).collect();
        let several = super::super::matching::Match { fits: 3, ..m.clone() };
        assert_eq!(line(reference_check(Some(&several), false, None, &near)).1, "Compared with MusicBrainz's editions: several fit within a few seconds; none exactly");
        let certain = super::super::matching::Match { certain: true, ..m.clone() };
        assert_eq!(line(reference_check(Some(&certain), false, None, &tracks)), ("green", "Matches a MusicBrainz edition: Down the Stairs, all 18 tracks".to_string()));
        let discogs = super::super::discogs::AlbumFacts { year: Some(1999), ..Default::default() };
        assert_eq!(line(reference_check(None, false, Some(&discogs), &tracks)), ("amber", "Only Discogs knows it (1999)".to_string()));
        assert_eq!(line(reference_check(None, false, None, &tracks)), ("red", "No source knows it".to_string()));
        for c in [reference_check(Some(&m), false, None, &tracks), reference_check(Some(&certain), false, None, &tracks)] {
            for word in ["likely", "within 5", "ref"] {
                assert!(!c.said.contains(word), "{}", c.said);
            }
        }
    }

    /// C37a, OQ1: the card's line on the library: a copy kept elsewhere it is better than (named by its library) or not; else another
    /// edition a main library has by name, no action; else nothing yet.
    #[test]
    fn the_card_says_how_the_record_stands_in_the_library() {
        let line = |c: Check| (c.state, c.said);
        let better = in_library(Some((true, "Music".into(), "lossless, yours is 320 kbps")), None);
        assert_eq!(line(better), ("amber", "Better than the copy in Music: lossless, yours is 320 kbps".to_string()));
        let twice = in_library(Some((false, "Music".into(), "320 kbps, same as yours")), Some("Music".into()));
        assert_eq!(line(twice), ("red", "Already in Music: 320 kbps, same as yours".to_string()));
        assert_eq!(line(in_library(None, Some("Music".into()))), ("amber", "Music has another edition".to_string()));
        assert_eq!(line(in_library(None, None)), ("green", "Not in your library yet".to_string()));
    }

    /// A copy that does not play has no rows in `songs`: all there is of it is its files, and a card about it must list those,
    /// in the order their own tags give.
    #[test]
    fn the_card_lists_its_copys_files() {
        let (root, cat, lib) = crate::serve::listener::tests::built("card", &[("Kept/1.flac", "Album", "One"), ("Kept/2.flac", "Album", "Two"), ("Other/1.flac", "Else", "Three")]);
        let album = lib.album_in(0, "Kept").unwrap().unwrap();
        cat.tag_for_tests(0, "Kept/1.flac", &[("album", "Album"), ("title", "One, by its tag"), ("tracknumber", "2/9"), ("discnumber", "1")]);
        cat.tag_for_tests(0, "Kept/2.flac", &[("album", "Album"), ("title", "Two, by its tag"), ("tracknumber", "1/9"), ("discnumber", "1")]);
        lib.db().execute("DELETE FROM songs WHERE rel LIKE 'Kept/%'", []).unwrap();
        let tracks: Vec<Track> = super::super::renumber::copy_tracks(&lib, &cat, &album, 0, "Kept").unwrap().into_iter().map(Track::from).collect();
        assert_eq!(tracks.iter().map(|t| (t.place.1.as_str(), t.title.as_str(), t.no)).collect::<Vec<_>>(), [("Kept/2.flac", "Two, by its tag", Some(1)), ("Kept/1.flac", "One, by its tag", Some(2))]);
        assert_eq!(tracks[0].path, root.join("Kept/2.flac"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn where_to_hear_a_track() {
        let mut w = vec![0u8; 100];
        for b in &mut w[10..] {
            *b = 200;
        }
        let s = snippets(&w, 100_000);
        assert_eq!((s.start, s.middle, s.end), (10_000, 50_000, 80_000));
        assert_eq!(snippets(&[], 30_000).start, 0, "no waveform: from its start");
        assert_eq!(snippets(&[], 30_000).end, 15_000, "a short one's end is its middle at least");
    }

    #[test]
    fn a_record_s_format_names_each_kind_once() {
        let f = |format: &str, bits: Option<u8>, rate: Option<u32>, kbps: Option<u32>| crate::catalogue::tags::FileTags {
            format: Some(format.into()),
            bit_depth: bits,
            sample_rate_hz: rate,
            bitrate_kbps: kbps,
            ..Default::default()
        };
        assert_eq!(format_of(&[f("FLAC", Some(24), Some(96_000), None), f("FLAC", Some(24), Some(96_000), None)]), "FLAC 24/96");
        assert_eq!(format_of(&[f("FLAC", Some(16), Some(44_100), None), f("MP3", None, None, Some(320))]), "FLAC 16/44.1 · MP3 320");
    }

    #[test]
    fn the_folder_and_the_tags_say_one_record() {
        let t = |path: &str| Track { id: "s".into(), path: path.into(), place: (0, path.into()), file: "f".into(), title: "t".into(), ms: 0, disc: None, no: None };
        let tags = |album: &str| crate::catalogue::tags::FileTags { items: vec![("ALBUM".into(), Some("album"), album.into())], ..Default::default() };
        let c = file_checks(&[t("/in/Air - Moon Safari (1998)/1.flac"), t("/in/Air - Moon Safari (1998)/2.flac")], &[tags("Moon Safari"), tags("Moon Safari")]);
        assert_eq!(c.iter().map(|c| (c.key, c.state)).collect::<Vec<_>>(), [("one_record", "green"), ("folder_name", "green")]);
        let c = file_checks(&[t("/in/x/1.flac"), t("/in/x/2.flac")], &[tags("Moon Safari"), tags("Premiers Symptômes")]);
        assert_eq!(c[0].state, "amber");
        assert!(c[0].said.contains("2 album names"));
        assert_eq!(c[1].state, "amber");
    }
}
