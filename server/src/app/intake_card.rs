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

/// What hearing a record found, kept per record (`intake_heard`).
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
    file: String,
    title: String,
    ms: i64,
    disc: Option<u32>,
    no: Option<u32>,
}

fn tracks_of(album: &str) -> Result<(Vec<Track>, String, String)> {
    let (tracks, _, artist, title) = super::renumber::tracks_of(album)?;
    Ok((
        tracks.into_iter().map(|t| Track { id: t.id, file: t.file, title: t.title, ms: i64::from(t.secs) * 1000, disc: t.disc, no: t.track, path: t.path }).collect(),
        artist,
        title,
    ))
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
fn reference_check(matched: Option<&super::matching::Match>, looking: bool, discogs: Option<&super::discogs::AlbumFacts>, n: usize) -> Check {
    match (matched, discogs) {
        (Some(m), _) if m.certain => Check { key: "reference", state: "green", said: format!("MusicBrainz agrees: {}, all {n} tracks", m.title) },
        (Some(m), _) => Check { key: "reference", state: "amber", said: format!("MusicBrainz likely: {}", m.why) },
        (None, _) if looking => Check { key: "reference", state: "waiting", said: "Being looked for on MusicBrainz".into() },
        (None, Some(d)) => Check { key: "reference", state: "amber", said: format!("Only Discogs knows it{}", d.year.map(|y| format!(" ({y})")).unwrap_or_default()) },
        (None, None) => Check { key: "reference", state: "red", said: "No reference knows it".into() },
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

/// A record's card. `album`: Cratarr's own album id; `library`, `verdict`: Incoming's for it. The hearing is started
/// where it was never done.
pub async fn card(app: &Arc<super::App>, album: &str, library: Option<String>, verdict: Option<&super::incoming::VerdictView>) -> Result<Card> {
    let served = app.served.clone().context("Cratarr's own library is not open")?;
    let a = album.to_string();
    let (s, a2) = (served.clone(), a.clone());
    #[allow(clippy::type_complexity)]
    let (tracks, artist, title, tags, waves, heard, discogs): (Vec<Track>, String, String, Vec<crate::catalogue::tags::FileTags>, Vec<Vec<u8>>, Option<Heard>, Option<super::discogs::AlbumFacts>) =
        tokio::task::spawn_blocking(move || -> Result<_> {
            let (tracks, artist, title) = tracks_of(&a2)?;
            let tags = tracks.iter().map(|t| crate::catalogue::tags::read(&t.path).map(|(f, _)| f).unwrap_or_default()).collect();
            let cat = s.catalogue();
            let waves = tracks
                .iter()
                .map(|t| {
                    s.library.song_file(&t.id).ok().flatten().and_then(|(folder, rel, ..)| cat.analysis_of(folder, &rel)).map(|an| an.waveform).unwrap_or_default()
                })
                .collect();
            let heard = s.userdata.intake_heard(&a2)?.and_then(|(_, j)| serde_json::from_str(&j).ok());
            let discogs = super::discogs_facts::album_kept(&s, &a2).ok().flatten();
            Ok((tracks, artist, title, tags, waves, heard, discogs))
        })
        .await??;
    let matched = super::matching::kept_or_find(&served, vec![a.clone()]).await?.into_iter().next().map(|(_, m)| m);
    let looking = matched.is_none() && super::tasks::tasks().of(&served.current_user().unwrap_or_default()).iter().any(|t| t.kind == super::matching::KIND && t.finished.is_none());
    if heard.is_none() {
        hear(app, &a);
    }
    let files: Vec<String> = tracks.iter().map(|t| t.file.clone()).collect();
    let folder = tracks.first().and_then(|t| t.path.parent()).and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let from = from_of(app, &files, &folder, &tags).await;
    let mut checks = vec![reference_check(matched.as_ref(), looking, discogs.as_ref(), tracks.len())];
    let (sound, acoustid) = heard_checks(heard.as_ref(), &tracks, matched.as_ref());
    checks.push(acoustid);
    checks.push(sound);
    checks.extend(file_checks(&tracks, &tags));
    if from.kind == "soulseek" {
        checks.push(Check { key: "one_user", state: "green", said: format!("From one Soulseek user's folder: {}", from.user.clone().unwrap_or_default()) });
    }
    checks.push(match verdict {
        None => Check { key: "in_library", state: "green", said: "Not in your library yet".into() },
        Some(v) if v.kind == "upgrade" => Check { key: "in_library", state: "amber", said: format!("Better than the copy in {}: {}", v.library, v.why) },
        Some(v) => Check { key: "in_library", state: "red", said: format!("Already in {}: {}", v.library, v.why) },
    });
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
pub async fn waiting(app: &Arc<super::App>) -> Vec<Waiting> {
    let view = app.incoming.list(None).await;
    let mut out = Vec::new();
    for s in view.sources.iter().filter(|s| s.id == crate::source::cratarr::KIND.id) {
        if let Some(e) = &s.error {
            tracing::warn!(target: "library", reason = %e, "records waiting not listed");
        }
        out.extend(s.albums.iter().map(|a| Waiting {
            source: s.id.clone(),
            album: a.id.clone(),
            title: a.title.clone(),
            artist: a.artist.clone().unwrap_or_default(),
            library: a.library.clone(),
            verdict: s.verdicts.get(&a.id).map(|v| v.kind),
        }));
    }
    out
}

/// Hears a record in the background, once (one task at a time for the account; another asked meanwhile waits for its
/// next card): each track decoded whole and its recordings asked of AcoustID, kept.
pub fn hear(app: &Arc<super::App>, album: &str) {
    let Some(served) = app.served.clone() else { return };
    let Some(account) = served.current_user() else { return };
    if super::tasks::tasks().of(&account).iter().any(|t| t.kind == KIND && t.finished.is_none()) {
        return;
    }
    let album = album.to_string();
    let _ = super::tasks::spawn(KIND, &account, false, move |h| async move {
        let (tracks, ..) = tokio::task::spawn_blocking({
            let a = album.clone();
            move || tracks_of(&a)
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
        tokio::task::spawn_blocking(move || ud.set_intake_heard(&a, super::moment::now_secs(), &json)).await??;
        anyhow::Ok(())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let t = |path: &str| Track { id: "s".into(), path: path.into(), file: "f".into(), title: "t".into(), ms: 0, disc: None, no: None };
        let tags = |album: &str| crate::catalogue::tags::FileTags { items: vec![("ALBUM".into(), Some("album"), album.into())], ..Default::default() };
        let c = file_checks(&[t("/in/Air - Moon Safari (1998)/1.flac"), t("/in/Air - Moon Safari (1998)/2.flac")], &[tags("Moon Safari"), tags("Moon Safari")]);
        assert_eq!(c.iter().map(|c| (c.key, c.state)).collect::<Vec<_>>(), [("one_record", "green"), ("folder_name", "green")]);
        let c = file_checks(&[t("/in/x/1.flac"), t("/in/x/2.flac")], &[tags("Moon Safari"), tags("Premiers Symptômes")]);
        assert_eq!(c[0].state, "amber");
        assert!(c[0].said.contains("2 album names"));
        assert_eq!(c[1].state, "amber");
    }
}
