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
use serde::Serialize;

pub use super::intake_card_base::{Heard, HeardTrack};
use super::requests_host::RequestsHost;

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
    /// The reference's length for it: the version the "Which release" row names, its track this file pairs with (F53.4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_ms: Option<i64>,
    /// Who says that length, in the wording table's words ("MusicBrainz, Bandcamp say 10:38"): every source of the version named.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_by: Option<String>,
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

impl std::convert::From<super::renumber_base::Track> for Track {
    fn from(t: super::renumber_base::Track) -> Self {
        Track { id: t.id, file: t.file, title: t.title, ms: i64::from(t.secs) * 1000, disc: t.disc, no: t.track, path: t.path, place: t.place }
    }
}

/// The tracks a card is about, in the server `served`: its copy's, at (catalogue folder, folder in it), whichever copy of the
/// record plays.
fn tracks_of(served: &crate::serve::Serve, album: &str, at: Option<&(i64, String)>) -> Result<(Vec<Track>, String, String)> {
    let (tracks, artist, title) = match at {
        Some((folder, dir)) => {
            let (_, artist, title) = served.library.album_names(album)?.context("No such album in Cratarr's own library")?;
            (super::renumber_base::copy_tracks(&served.library, served.catalogue(), album, *folder, dir)?, artist, title)
        }
        None => {
            let (tracks, _, artist, title) = super::renumber_base::tracks_of_in(served, album)?;
            (tracks, artist, title)
        }
    };
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
    // The facts' cleaned values (C-WM-6, T-WM-2: the raw tags stay for the tag hash and the persistent ids alone).
    let field = |name: &str| -> BTreeSet<String> {
        tags.iter().filter_map(|t| t.items.iter().find(|(_, n, _)| *n == Some(name)).and_then(|(_, _, v)| crate::catalogue::fact_value(name, v))).filter(|v| !v.is_empty()).collect()
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

/// What hearing found, as the sound's and AcoustID's checks (waiting where not heard yet). `matched`: each file's recording on the
/// version the row names (file, recording), where it names one.
fn heard_checks(heard: Option<&Heard>, tracks: &[Track], matched: Option<&[(String, String)]>) -> (Check, Check) {
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
            .filter(|t| match matched.and_then(|mm| mm.iter().find(|(f, _)| *f == t.file)) {
                Some((_, rec)) => t.recordings.iter().any(|r| r.eq_ignore_ascii_case(rec)),
                None => !t.recordings.is_empty(),
            })
            .count();
        let state = if n == m { "green" } else if n * 2 >= m { "amber" } else { "red" };
        Check { key: "acoustid", state, said: format!("AcoustID knows {n} of {m} recordings{}", if matched.is_some() { " as the release has them" } else { "" }) }
    };
    (sound, acoustid)
}

/// The sources an arrival's job searches by its names (F53.7: MusicBrainz's matching and Discogs' search, never Bandcamp's).
pub const SEARCHED: [&str; 2] = ["musicbrainz", "discogs"];

/// The arrival's "Which release" row as read (F53): its check (key `reference`, the page's "Which release"), the verdict, the
/// versions it was worked out on, and the copy's files as the one pairing read them.
#[derive(Debug, Clone)]
pub struct Row {
    pub check: Check,
    pub release: cratarr_domain::which_release::Release,
    pub versions: Vec<cratarr_domain::facts::ListVersion>,
    pub copy: Vec<cratarr_domain::facts::Present>,
}

/// Whether a source's switch is on (a source with no switch of its own is).
fn switched_on(source: &str) -> bool {
    use super::switches::{BANDCAMP, DISCOGS, MUSICBRAINZ, is_on};
    match source {
        "musicbrainz" => is_on(MUSICBRAINZ),
        "discogs" => is_on(DISCOGS),
        "bandcamp" => is_on(BANDCAMP),
        _ => true,
    }
}

/// Whether a source asked lately didn't answer, and is held until later (its turn's pause: F13's "asked again later").
fn busy(source: &str) -> bool {
    match source {
        "musicbrainz" => super::musicbrainz::TURN.paused_until().is_some(),
        "discogs" => super::discogs::TURN.paused_until().is_some(),
        "bandcamp" => crate::bandcamp::TURN.paused_until().is_some(),
        _ => false,
    }
}

/// The "Which release" row (F53; T-F53-2, T-F53-3): which of the record's versions the arrival's copy at `at` (catalogue folder,
/// folder in it; none: its first Incoming copy, else its first) is, judged by every kept list equally (`which_release::row`), the
/// sources not read yet named beside it, the arrival's job's finds as guesses where no sure reference is kept. It reads only what is
/// kept: it asks no source, starts no task and stores nothing (H1, 23:55). Blocking.
pub fn which_release_row(served: &crate::serve::Serve, album: &str, at: Option<&(i64, String)>) -> Result<Row> {
    use cratarr_domain::facts::{ASKED, Reference, versions};
    use cratarr_domain::which_release::{Candidate, Kept, row, said};
    let cat = served.catalogue();
    let record = crate::serve::ids::record_number(album).context("No such album in Cratarr's own library")?;
    let kinds = served.library.kinds();
    let places = super::room::Places::of(&kinds, &cat.folders());
    let library = super::requests::library_of(&kinds, &places);
    let read = cat.record_read(record, &library)?.context("No such album in Cratarr's own library")?;
    let subject = crate::serve::ids::record(read.row.id);
    // The copy: the files the one pairing reads (the holding's reader, F53.10).
    let mut files = cat.copy_files(read.row.id)?;
    let incoming = |folder: &i64| crate::serve::library::kind_in(&kinds, *folder).0 == crate::settings::LibraryKind::Incoming;
    let copy = match at {
        Some((folder, dir)) => {
            let within = |key: &str| dir.is_empty() || key.starts_with(&format!("{}/", dir.trim_end_matches('/')));
            files.remove(folder).unwrap_or_default().into_iter().filter(|f| within(&f.key)).collect()
        }
        None => {
            let folder = files.keys().copied().find(incoming).or_else(|| files.keys().copied().next());
            folder.and_then(|f| files.remove(&f)).unwrap_or_default()
        }
    };
    // A find today's pairing confirms (a main library's or own music's copy, C15) is a reference: its list a version like any.
    let mut lists = read.lists.clone();
    for l in lists.iter_mut().filter(|l| read.copies.confirmed.contains(&l.reference)) {
        l.guessed = false;
    }
    let versions = versions(&lists);
    let references = cat.references_of(&subject)?;
    let sure: std::collections::BTreeSet<String> = references
        .iter()
        .filter(|(r, row)| row.how != "search" || row.confirmed || read.copies.confirmed.contains(&Reference::new(&r.source, &r.kind, &r.value)))
        .map(|(r, _)| r.source.clone())
        .collect();
    // Last.fm's kept words are its reference (OQ-HS01-5 (a)): a reference that gives no tracklist.
    let worded = !served.userdata.words_of(&subject, "lastfm")?.is_empty();
    let referenced = !sure.is_empty() || worded;
    let listed: std::collections::BTreeSet<&str> = lists.iter().filter(|l| !l.guessed && !l.tracks.is_empty()).map(|l| l.reference.source.as_str()).collect();
    let waiting: Vec<String> = ASKED.iter().filter(|s| sure.contains(**s) && !listed.contains(**s) && switched_on(s)).map(|s| s.to_string()).collect();
    let busy: Vec<String> = waiting.iter().filter(|s| busy(s)).cloned().collect();
    let title_of = |r: &Reference| read.said.iter().find(|s| s.fact == "title" && s.source == r.source && s.of == crate::catalogue::facts::of_ref(&r.kind, &r.value)).and_then(|s| s.value.clone());
    let guesses: Vec<Candidate> = if referenced { Vec::new() } else { read.guesses.iter().map(|r| Candidate { reference: r.clone(), title: title_of(r) }).collect() };
    let searching: Vec<String> = if referenced {
        Vec::new()
    } else {
        SEARCHED.iter().filter(|s| switched_on(s) && !super::arrival_job::searched(&served.userdata, &subject, s)).map(|s| s.to_string()).collect()
    };
    let release = row(&copy, &Kept { versions: &versions, pick: read.pick.as_ref(), guesses: &guesses, referenced, waiting: &waiting, busy: &busy, searching: &searching });
    let year = |r: &Reference| -> Option<u32> {
        read.said.iter().find(|s| s.fact == "year" && s.source == r.source && s.of == crate::catalogue::facts::of_ref(&r.kind, &r.value)).and_then(|s| s.value.as_deref()?.get(..4)?.parse().ok())
    };
    let check = Check { key: "reference", state: release.state.colour(), said: said(&release, &versions, &year) };
    Ok(Row { check, release, versions, copy })
}

/// Where the record came from: Bandcamp where its files say so (Bandcamp's own comment, "Visit https://….bandcamp.com"),
/// the Soulseek user its files were downloaded from (slskd's transfers), Lidarr where it imported its folder; else a
/// folder.
async fn from_of(app: &dyn RequestsHost, files: &[String], folder: &str, tags: &[crate::catalogue::tags::FileTags]) -> From {
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
    let set = crate::run::spawn_blocking(move || super::track_fetch::slskd_set(&ud)).await.ok()?.ok()??;
    let d = Arc::new(crate::slskd::Slskd::new(&set.url, &set.key).ok()?.downloads().await.ok()?);
    *KEPT.lock().unwrap_or_else(|e| e.into_inner()) = Some((std::time::Instant::now(), d.clone()));
    Some(d)
}

/// The folders Lidarr imported lately, asked at most once a minute.
async fn lidarr_folders(app: &dyn RequestsHost) -> Option<Arc<Vec<String>>> {
    static KEPT: std::sync::Mutex<Option<(std::time::Instant, Arc<Vec<String>>)>> = std::sync::Mutex::new(None);
    if let Some((at, f)) = KEPT.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && at.elapsed() < ASKED_FOR
    {
        return Some(f.clone());
    }
    let f = Arc::new(app.lidarr()?.imported_folders().await.ok()?);
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
/// the card reads, whichever copy plays; `library`: Incoming's for it. It reads only what is kept (LK.B1, T-F53-3): its matching is
/// the arrival's job's (`arrival_job`), its hearing an administrator's open of it (`hear_at_open`); until then its checks wait.
pub async fn card<H: RequestsHost>(app: &Arc<H>, album: &str, at: Option<(i64, String)>, library: Option<String>) -> Result<Card> {
    card_on(&**app, album, at, library).await
}

/// [`card`], on its host.
async fn card_on(app: &dyn RequestsHost, album: &str, at: Option<(i64, String)>, library: Option<String>) -> Result<Card> {
    let served = app.serve().clone().context("Cratarr's own library is not open")?;
    let a = album.to_string();
    let (s, a2, at2) = (served.clone(), a.clone(), at.clone());
    #[allow(clippy::type_complexity)]
    let (tracks, artist, title, tags, waves, heard, row): (Vec<Track>, String, String, Vec<crate::catalogue::tags::FileTags>, Vec<Vec<u8>>, Option<Heard>, Row) =
        crate::run::spawn_blocking(move || -> Result<_> {
            let (tracks, artist, title) = tracks_of(&s, &a2, at2.as_ref())?;
            let tags = tracks.iter().map(|t| crate::catalogue::tags::read(&t.path).map(|(f, _)| f).unwrap_or_default()).collect();
            let cat = s.catalogue();
            let waves = tracks.iter().map(|t| cat.analysis_of(t.place.0, &t.place.1).map(|an| an.waveform).unwrap_or_default()).collect();
            let heard = s.userdata.intake_heard(&a2, at2.as_ref().map(|p| p.0))?.and_then(|(_, j)| serde_json::from_str(&j).ok());
            let row = which_release_row(&s, &a2, at2.as_ref())?;
            Ok((tracks, artist, title, tags, waves, heard, row))
        })
        .await??;
    // Each card track's place on the row's copy, and so the track of the version named it pairs with.
    let paired = |t: &Track| row.copy.iter().position(|f| f.key == t.place.1);
    let named: Vec<(String, String)> =
        tracks.iter().filter_map(|t| Some((t.file.clone(), row.release.tracks.get(paired(t)?)?.as_ref()?.recording.clone()?))).collect();
    let files: Vec<String> = tracks.iter().map(|t| t.file.clone()).collect();
    let folder = tracks.first().and_then(|t| t.path.parent()).and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let from = from_of(app, &files, &folder, &tags).await;
    let mut checks = vec![row.check.clone()];
    let (sound, acoustid) = heard_checks(heard.as_ref(), &tracks, row.release.version.map(|_| named.as_slice()));
    checks.push(acoustid);
    checks.push(sound);
    checks.extend(file_checks(&tracks, &tags));
    if from.kind == "soulseek" {
        checks.push(Check { key: "one_user", state: "green", said: format!("From one Soulseek user's folder: {}", from.user.clone().unwrap_or_default()) });
    }
    let (kept, edition) = {
        let (s, a, at, mains) = (served.clone(), album.to_string(), at.clone(), app.main_folders());
        crate::run::spawn_blocking(move || stands_of(&s, &a, at.as_ref(), &mains)).await??
    };
    let name = |folder: i64| app.library_name(crate::source::cratarr::KIND.id, folder);
    checks.push(in_library(kept.as_ref().map(|(better, folder, why)| (*better, name(*folder), why.as_str())), edition.map(name)));
    let suggested = suggested(&checks);
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
            reference_ms: paired(t).and_then(|i| cratarr_domain::which_release::track_says(&row.release, i)).map(|(ms, _)| ms),
            reference_by: paired(t).and_then(|i| cratarr_domain::which_release::track_says(&row.release, i)).map(|(_, said)| said),
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
        suggested,
    })
}

/// What a Confirm on the row did (F53.7 (b), (d)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmed {
    /// The guess is the record's reference now, by hand.
    Kept,
    /// The record holds a sure reference already (another's Confirm came first): nothing kept.
    ChangedSince,
    /// No such guess of the record.
    NoSuchGuess,
}

/// Confirm (F53.7 (b), (d); OQ-F53-1 (a)): an administrator's yes to one of the row's guesses, made the record's reference by hand
/// (`how` "hand": sure, OQ-VR-4 (a)), so its kept list judges from now on. Refused where the record holds a sure reference already:
/// a Confirm sent from a section read before another's, so two candidates are never confirmed by a race, and nothing is kept. The
/// record owed to the change log, so an open section reads it again (LV1). Blocking.
pub fn confirm(served: &crate::serve::Serve, album: &str, r: &cratarr_domain::facts::Reference) -> Result<Confirmed> {
    // One Confirm at a time: its look and its write one step.
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let cat = served.catalogue();
    let record = crate::serve::ids::record_number(album).context("No such album in Cratarr's own library")?;
    let subject = crate::serve::ids::record(record);
    let references = cat.references_of(&subject)?;
    let guess = references.iter().any(|(x, row)| x.source == r.source && x.kind == r.kind && x.value == r.value && row.how == "search" && !row.confirmed);
    if !guess {
        return Ok(Confirmed::NoSuchGuess);
    }
    if references.iter().any(|(_, row)| row.how != "search" || row.confirmed) {
        return Ok(Confirmed::ChangedSince);
    }
    cat.set_reference(&subject, &crate::catalogue::identity::Ref::new(&r.source, &r.kind, &r.value), "hand")?;
    cat.owe_records(&[record])?;
    Ok(Confirmed::Kept)
}

/// What Cratarr suggests for a record by its checks: Keep where every one is green; else nothing, the checks saying why.
pub(crate) fn suggested(checks: &[Check]) -> Option<&'static str> {
    checks.iter().all(|c| c.state == "green").then_some("keep")
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
/// took minutes). Read from Incoming's list, by the app (`app_requests`).
pub async fn waiting<H: RequestsHost>(app: &Arc<H>) -> Vec<Waiting> {
    app.intake_waiting().await
}

/// The copy an arrival's hearing hears: the record's copy in an Incoming library (its catalogue folder and its folder there), where
/// it has one. Blocking.
pub fn arrival_copy(served: &crate::serve::Serve, album: &str) -> Result<Option<(i64, String)>> {
    let kinds = served.library.kinds();
    Ok(served
        .library
        .copies_of(album)?
        .into_iter()
        .find(|c| crate::serve::library::kind_in(&kinds, c.folder).0 == crate::settings::LibraryKind::Incoming)
        .map(|c| (c.folder, c.dir)))
}

/// An arrival heard, begun by an administrator's open of it (LK.B1, OQ-LK-3 (a): the record's open from the Incoming place): a
/// Cratarr job (no account's) keyed by the record, one running followed, never a second; none where its copy is heard already. Each
/// track decoded whole (kind `intake`, its memory declared) and its recordings asked of AcoustID through the door, kept under that
/// copy. Its task, where one runs or ran within its hour.
pub async fn hear_at_open(served: Arc<crate::serve::Serve>, album: &str, at: Option<(i64, String)>) -> Option<crate::runner::Task> {
    let (ud, a, folder) = (served.userdata.clone(), album.to_string(), at.as_ref().map(|p| p.0));
    let heard = crate::run::spawn_blocking(move || ud.intake_heard(&a, folder)).await.ok()?.ok()?.is_some();
    if heard {
        return None;
    }
    let handle = match crate::runner::runner().begin_keyed(KIND, album, None) {
        Ok(h) => h,
        Err(Ok(running)) => return Some(running),
        Err(Err(_)) => return None,
    };
    let album = album.to_string();
    // The server's AcoustID key, and the door as installed here asking with it (CU15): AcoustID is asked through it.
    let ud = served.userdata.clone();
    let key = crate::run::spawn_blocking(move || super::track_fetch::acoustid_key(&ud)).await.ok().and_then(Result::ok).flatten();
    let door = super::references_door::door_with_acoustid(key.clone());
    crate::runner::jobs::run(Arc::new(handle), move |h| async move {
        let (tracks, ..) = crate::run::spawn_blocking({
            let (s, a, at) = (served.clone(), album.clone(), at.clone());
            move || tracks_of(&s, &a, at.as_ref())
        })
        .await??;
        let heard = hearing(door.as_ref(), &tracks, key.is_some(), |i, total| h.step("Hearing the record's tracks", i, Some(total))).await?;
        let (ud, json, a) = (served.userdata.clone(), serde_json::to_string(&heard)?, album.clone());
        let folder = at.map(|p| p.0);
        crate::run::spawn_blocking(move || ud.set_intake_heard(&a, folder, super::moment::now_secs(), &json)).await??;
        // The open section reads it again by the change log (LV1).
        if let Some(record) = crate::serve::ids::record_number(&album) {
            let cat = served.catalogue().clone();
            let _ = crate::run::spawn_blocking(move || cat.owe_records(&[record])).await;
        }
        anyhow::Ok(())
    })
    .ok()
}

/// A copy's tracks heard (`hear`'s work): each decoded whole and, where AcoustID is set (`asks`), its recordings asked through the
/// door (CU15.2), those it is sure of (`acoustid::SURE`) kept. Each step told to `step` (done, of).
async fn hearing(door: &dyn super::references_door::References, tracks: &[Track], asks: bool, step: impl Fn(u64, u64)) -> Result<Heard> {
    let mut heard = Heard { no_acoustid: !asks, ..Heard::default() };
    let total = tracks.len() as u64;
    for (i, t) in tracks.iter().enumerate() {
        step(i as u64, total);
        let path = t.path.clone();
        let mut one = HeardTrack { file: t.file.clone(), ..HeardTrack::default() };
        match crate::run::spawn_blocking(move || crate::acoustid::print(&path)).await? {
            Ok(print) => {
                one.damaged = print.damaged;
                one.flaw = print.flaws.why();
                one.flaw_at_ms = print.flaws.clicks.first().copied().or(print.flaws.first_clip).map(|s| (s * 1000.0) as i64);
                if asks {
                    match door.recordings(&print).await {
                        Ok(found) => one.recordings = found.iter().filter(|f| f.score >= crate::acoustid::SURE).flat_map(|f| f.recordings.iter().map(|r| r.id.clone())).collect(),
                        Err(e) => tracing::debug!("intake: AcoustID for {}: {e:#}", t.file),
                    }
                }
            }
            Err(e) => one.failed = Some(super::plain(format!("{e:#}"))),
        }
        heard.tracks.push(one);
    }
    Ok(heard)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CU15.2: a card hears AcoustID through the door. A card's copy of 3 tracks; a stand-in door expecting their recordings. The
    /// hearing kept names the door's recordings at 0.9 and over, per track. Never a question the stand-in did not expect, nor a
    /// lookup outside the door.
    #[tokio::test]
    async fn a_card_hears_acoustid_through_the_door() {
        use crate::acoustid::{Found, Recording};
        use crate::app::references_door::{Asked, StandIn};
        let tide = crate::fixtures::gate::TIDE_TABLES;
        let folder = crate::fixtures::gate::gate_cases_once().join(crate::fixtures::gate::CLEAN).join(tide.folder());
        let tracks: Vec<Track> = (1..=3)
            .map(|n| {
                let file = tide.file(n);
                Track { id: format!("tr-{n}"), path: folder.join(&file), place: (1, file.clone()), file, title: tide.tracks[n - 1].0.into(), ms: i64::from(tide.tracks[n - 1].1) * 1000, disc: Some(1), no: Some(n as u32) }
            })
            .collect();
        let found = |score: f32, id: &str| Found { score, recordings: vec![Recording { id: id.into() }] };
        let door = StandIn::new().recordings(vec![found(0.95, "rec-sure"), found(0.6, "rec-unsure")]).install();
        let heard = hearing(&*crate::app::references_door::door(), &tracks, true, |_, _| {}).await.unwrap();
        let kept: Heard = serde_json::from_str(&serde_json::to_string(&heard).unwrap()).unwrap();
        assert_eq!(kept.tracks.len(), 3);
        assert!(kept.tracks.iter().all(|t| t.recordings == ["rec-sure"]), "the door's recordings at 0.9 and over, per track: {:?}", kept.tracks);
        assert!(!kept.no_acoustid);
        let asked = door.asked();
        assert_eq!(asked.len(), 3, "one question a track: {asked:?}");
        assert!(asked.iter().all(|a| matches!(a, Asked::Recordings { .. })));
        assert!(door.violations().is_empty(), "{:?}", door.violations());
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
        let tracks: Vec<Track> = super::super::renumber_base::copy_tracks(&lib, &cat, &album, 0, "Kept").unwrap().into_iter().map(Track::from).collect();
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

    /// C-WM-6 (T-WM-2): the arrival's file checks read the facts' cleaned values (`hygiene`), never the raw tags: an album name
    /// with a site's stamp beside it is the same album as without, and a stamp alone is no album name. Red first: the raw values,
    /// two album names.
    #[test]
    fn the_file_checks_read_the_cleaned_values() {
        let t = |path: &str| Track { id: "s".into(), path: path.into(), place: (0, path.into()), file: "f".into(), title: "t".into(), ms: 0, disc: None, no: None };
        let tags = |album: &str, artist: &str| crate::catalogue::tags::FileTags {
            items: vec![("ALBUM".into(), Some("album"), album.into()), ("ALBUMARTIST".into(), Some("albumartist"), artist.into())],
            ..Default::default()
        };
        let files = [t("/in/Mumia - Answered/1.mp3"), t("/in/Mumia - Answered/2.mp3"), t("/in/Mumia - Answered/3.mp3")];
        let c = file_checks(&files, &[tags("Answered", "Mumia"), tags("Answered [www.electronicfresh.com]", "Mumia"), tags("Answered", "www.electronicfresh.com")]);
        assert_eq!(c.iter().map(|c| (c.key, c.state)).collect::<Vec<_>>(), [("one_record", "green"), ("folder_name", "green")], "{c:?}");
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

    /// SK5.3 (the owner, 2026-11-19): what Soulseek's check let through as notes shows on its arrival's card, amber, in the gate's
    /// own words, as its hearing keeps them (`hear`: `Findings::why`): no red for notes alone, never green, and no Keep suggested
    /// while a check is not green. Keep, Replace and Delete stay in the album's menu, as for any arrival.
    #[test]
    fn the_notes_show_amber_on_the_arrivals_card() {
        use crate::analysis::flaws::Findings;
        let track = |no: u32, title: &str| {
            let file = format!("{no:02} {title}.flac");
            Track { id: format!("tr-{no}"), path: PathBuf::from(&file), place: (0, format!("Tide Tables/{file}")), file, title: title.into(), ms: 240_000, disc: Some(1), no: Some(no) }
        };
        let tracks = [track(1, "Slack Water"), track(2, "Neap"), track(3, "Spring Tide")];
        let heard = |no: u32, flaws: Findings| {
            let t = &tracks[no as usize - 1];
            HeardTrack { file: t.file.clone(), flaw: flaws.why(), recordings: vec![format!("rec-{no}")], ..HeardTrack::default() }
        };
        let h = Heard {
            tracks: vec![
                heard(1, Findings::default()),
                heard(2, Findings { clips: 5, first_clip: Some(106.0), ..Findings::default() }),
                heard(3, Findings { clicks: vec![130.0], pop_at_end: true, ..Findings::default() }),
            ],
            no_acoustid: false,
        };
        let (sound, acoustid) = heard_checks(Some(&h), &tracks, None);
        assert_eq!(sound, Check { key: "sound", state: "amber", said: "Track 2: Clipped at 1:46; Track 3: A click at 2:10; a pop at the very end".into() });
        assert_eq!(acoustid.state, "green", "AcoustID knows each");
        assert_eq!(suggested(&[acoustid.clone(), sound]), None, "not every check green: Keep not suggested");
        assert_eq!(suggested(&[acoustid]), Some("keep"));
    }
}
