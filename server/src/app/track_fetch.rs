//! Single-track replacement (0.9.8, the owner): a track an album lacks, or one to replace, fetched on its own when the
//! whole album cannot be had. One track at a time, never bulk (the March 2026 lesson):
//! 1. the album's release from MusicBrainz, each of its tracks with the file that holds it, or none (`release`);
//! 2. one slskd search for one of them, from MusicBrainz's artist, title and length (`search`); the admin picks a file;
//! 3. the file fetched into its own folder under slskd's, moved out to Cratarr's staging at once, and gated (`fetch`):
//!    decoded whole without a damaged frame, within [`crate::slskd::LENGTH_SLACK_MS`] of the recording, and named that
//!    recording by AcoustID at [`crate::acoustid::SURE`] or more;
//! 4. "Add to album" (`add`): tagged from MusicBrainz and the album's own tags, named as its neighbours are, the cover
//!    inside, into the album's folder; a track it replaces goes to the trash. Undo (`UNDO`) puts it all back.
//!
//! The searches and fetches are kept in memory (a restart forgets them; their staged files are tidied the next time).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::musicbrainz::Asked as _;
use crate::slskd::{Candidate, Slskd};

/// What an undo token starts with.
pub const UNDO: char = 'a';
/// Where the connection's choices are kept (user data's server choices, as Discogs' key).
pub const SLSKD_URL: &str = "slskd:url";
pub const SLSKD_KEY: &str = "slskd:key";
pub const SLSKD_FOLDER: &str = "slskd:folder";
pub const ACOUSTID_KEY: &str = "acoustid:key";
/// A file's length within this of the release's track places it there (s), as `incomplete` places them.
const PLACE_SECS: i64 = 5;
/// How long a search is waited for at most.
const SEARCH_FOR: Duration = Duration::from_secs(60);
/// How long a fetch may wait in an uploader's queue before it is given up.
const QUEUED_FOR: Duration = Duration::from_secs(30 * 60);
/// Staged files left from before (a restart, a fetch never added) go after this long.
const STAGED_FOR: Duration = Duration::from_secs(86_400);

// ---- Settings

/// slskd as set in Settings › Server: its address, key and completed-downloads folder (as Cratarr sees it).
#[derive(Clone)]
pub struct SlskdSet {
    pub url: String,
    pub key: String,
    pub folder: PathBuf,
}

fn choice(ud: &crate::userdata::UserData, k: &str) -> Result<Option<String>> {
    Ok(ud.import_choice(k)?.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
}

/// slskd's settings, where address, key and folder are all set.
pub fn slskd_set(ud: &crate::userdata::UserData) -> Result<Option<SlskdSet>> {
    Ok(match (choice(ud, SLSKD_URL)?, choice(ud, SLSKD_KEY)?, choice(ud, SLSKD_FOLDER)?) {
        (Some(url), Some(key), Some(folder)) => Some(SlskdSet { url, key, folder: PathBuf::from(folder) }),
        _ => None,
    })
}

pub fn acoustid_key(ud: &crate::userdata::UserData) -> Result<Option<String>> {
    choice(ud, ACOUSTID_KEY)
}

fn served() -> Result<Arc<crate::serve::Serve>> {
    crate::serve::instance().context("Cratarr's own library is not open")
}

async fn slskd() -> Result<(Slskd, PathBuf)> {
    let ud = served()?.userdata.clone();
    let set = tokio::task::spawn_blocking(move || slskd_set(&ud)).await??.context("slskd is not set up: Settings › Server")?;
    if !super::switches::is_on(super::switches::SLSKD) {
        bail!("slskd is switched off");
    }
    Ok((Slskd::new(&set.url, &set.key)?, set.folder))
}

// ---- 1. The release

/// The file that holds a release's track.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Have {
    pub source: String,
    pub id: String,
}

/// One of the release's tracks, as the page lists them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseTrack {
    pub disc: u32,
    pub position: u32,
    pub title: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length_ms: Option<i64>,
    pub recording: String,
    pub have: Option<Have>,
    #[serde(skip)]
    track_id: String,
    #[serde(skip)]
    artist_id: Option<String>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseView {
    pub release: String,
    pub title: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    pub tracks: Vec<ReleaseTrack>,
}

/// A release's tracks as MusicBrainz gives them (with its recordings and artist credits).
fn release_of(v: &Value) -> ReleaseView {
    let credit = |c: &Value| -> (String, Option<String>) {
        let parts = c.as_array().cloned().unwrap_or_default();
        let name = parts.iter().map(|p| format!("{}{}", p["name"].as_str().unwrap_or_default(), p["joinphrase"].as_str().unwrap_or_default())).collect::<String>();
        (name.trim().to_string(), parts.first().and_then(|p| p["artist"]["id"].as_str()).map(str::to_string))
    };
    let mut tracks = Vec::new();
    for (d, m) in v["media"].as_array().into_iter().flatten().enumerate() {
        let disc = m["position"].as_u64().map_or(d as u32 + 1, |p| p as u32);
        for (i, t) in m["tracks"].as_array().into_iter().flatten().enumerate() {
            let (artist, artist_id) = credit(&t["artist-credit"]);
            tracks.push(ReleaseTrack {
                disc,
                position: t["position"].as_u64().map_or(i as u32 + 1, |p| p as u32),
                title: t["title"].as_str().unwrap_or_default().to_string(),
                artist,
                length_ms: t["length"].as_i64().or_else(|| t["recording"]["length"].as_i64()),
                recording: t["recording"]["id"].as_str().unwrap_or_default().to_string(),
                have: None,
                track_id: t["id"].as_str().unwrap_or_default().to_string(),
                artist_id,
                path: None,
            });
        }
    }
    ReleaseView {
        release: v["id"].as_str().unwrap_or_default().to_string(),
        title: v["title"].as_str().unwrap_or_default().to_string(),
        artist: credit(&v["artist-credit"]).0,
        year: v["date"].as_str().and_then(|d| d.get(..4)).and_then(|y| y.parse().ok()),
        tracks,
    }
}

/// Each file at the release's track of its title with the closest length (within `PLACE_SECS`), each track once.
fn place(view: &mut ReleaseView, files: &[super::renumber::Track]) {
    for f in files {
        let best = view
            .tracks
            .iter_mut()
            .filter(|t| t.have.is_none() && super::renumber::same_title(&f.title, &t.title))
            .filter(|t| t.length_ms.is_none_or(|l| (l / 1000 - i64::from(f.secs)).abs() <= PLACE_SECS))
            .min_by_key(|t| t.length_ms.map_or(i64::MAX, |l| (l / 1000 - i64::from(f.secs)).abs()));
        if let Some(t) = best {
            t.have = Some(Have { source: crate::source::cratarr::KIND.id.to_string(), id: f.id.clone() });
            t.path = Some(f.path.clone());
        }
    }
}

/// The release an album's missing tracks are on: the one `incomplete` found holding it, else its files' own, else its
/// match's.
fn release_id_of(served: &crate::serve::Serve, album: &str) -> Result<Option<String>> {
    let found = served.userdata.albums_incomplete()?.into_iter().find(|(a, _)| a == album).and_then(|(_, j)| serde_json::from_str::<super::incomplete::Incomplete>(&j).ok());
    if let Some(id) = found.and_then(|i| i.release_id) {
        return Ok(Some(id));
    }
    if let Some(own) = served.library.album_place(album)?.and_then(|p| p.2).filter(|m| super::discogs::is_mbid(m)) {
        return Ok(Some(own));
    }
    Ok(served
        .userdata
        .album_matches(&[album.to_string()])?
        .remove(album)
        .and_then(|(_, j)| j)
        .and_then(|j| serde_json::from_str::<super::matching::Match>(&j).ok())
        .map(|m| m.release))
}

/// The album's release, each track with the file that holds it.
pub async fn release(album: &str) -> Result<ReleaseView> {
    let s = served()?;
    let a = album.to_string();
    let (id, files) = tokio::task::spawn_blocking(move || -> Result<_> { Ok((release_id_of(&s, &a)?, super::renumber::tracks_of(&a)?.0)) }).await??;
    let id = id.context("This album has no MusicBrainz release yet")?;
    let http = super::discover::http()?;
    let v: Value = http
        .get(format!("https://musicbrainz.org/ws/2/release/{id}?inc=recordings+artist-credits&fmt=json"))
        .asked()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mut view = release_of(&v);
    place(&mut view, &files);
    Ok(view)
}

// ---- 2. Searching

/// What a search is for.
#[derive(Debug, Clone)]
struct Wanted {
    album: String,
    track: ReleaseTrack,
    /// The track it replaces.
    replace: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Searching {
    Searching,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchView {
    pub state: Searching,
    pub candidates: Vec<Candidate>,
    pub bandcamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

struct SearchJob {
    wanted: Wanted,
    view: SearchView,
}

// ---- 3. Fetching

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Fetching {
    Queued,
    Downloading,
    Checking,
    Passed,
    Refused,
    Added,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchView {
    pub state: Fetching,
    pub percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
}

struct FetchJob {
    wanted: Wanted,
    view: FetchView,
    staged: Option<PathBuf>,
}

#[derive(Default)]
struct Jobs {
    searches: HashMap<String, SearchJob>,
    fetches: HashMap<String, FetchJob>,
}

static JOBS: Mutex<Option<Jobs>> = Mutex::new(None);

fn jobs<T>(f: impl FnOnce(&mut Jobs) -> T) -> T {
    f(JOBS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Jobs::default))
}

fn new_id() -> String {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).expect("the operating system's random source");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// What Soulseek is asked: the artist and title as words (its search matches words; punctuation only hinders).
fn query(artist: &str, title: &str) -> String {
    let words = |s: &str| s.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect::<String>();
    format!("{} {}", words(artist), words(title)).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Starts a search for the album's track of `recording`; its id.
pub async fn search(album: &str, recording: &str, replace: Option<String>) -> Result<String> {
    let view = release(album).await?;
    let track = view.tracks.into_iter().find(|t| t.recording == recording).context("That recording is not on the album's release")?;
    if replace.is_none() && track.have.is_some() {
        bail!("The album has that track already");
    }
    let (slskd, _) = slskd().await?;
    let text = query(&track.artist, &track.title);
    let bandcamp = format!("https://bandcamp.com/search?q={}&item_type=t", text.replace(' ', "+"));
    let id = new_id();
    let wanted = Wanted { album: album.to_string(), track, replace };
    let length_ms = wanted.track.length_ms;
    jobs(|j| {
        j.searches.insert(id.clone(), SearchJob { wanted, view: SearchView { state: Searching::Searching, candidates: Vec::new(), bandcamp, why: None } });
    });
    let job = id.clone();
    tokio::spawn(async move {
        let found = async {
            let sid = slskd.search(&text).await?;
            let started = std::time::Instant::now();
            while !slskd.searched(&sid).await? && started.elapsed() < SEARCH_FOR {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            let answers = slskd.answers(&sid).await;
            slskd.forget(&sid).await;
            // FLAC and MP3 only: the library's formats, which Cratarr writes tags into.
            anyhow::Ok(crate::slskd::candidates(answers?, length_ms).into_iter().filter(|c| matches!(c.format.as_str(), "flac" | "mp3")).enumerate().map(|(i, c)| Candidate { id: i, ..c }).collect::<Vec<_>>())
        }
        .await;
        jobs(|j| {
            if let Some(s) = j.searches.get_mut(&job) {
                match found {
                    Ok(c) => (s.view.state, s.view.candidates) = (Searching::Done, c),
                    Err(e) => (s.view.state, s.view.why) = (Searching::Failed, Some(format!("{e:#}"))),
                }
            }
        });
    });
    Ok(id)
}

pub fn search_view(id: &str) -> Option<SearchView> {
    jobs(|j| j.searches.get(id).map(|s| s.view.clone()))
}

/// Fetches one of a search's candidates; the fetch's id.
pub async fn fetch(search: &str, candidate: usize) -> Result<String> {
    let (wanted, c) = jobs(|j| {
        let s = j.searches.get(search)?;
        Some((s.wanted.clone(), s.view.candidates.iter().find(|c| c.id == candidate)?.clone()))
    })
    .context("That search is gone; search again")?;
    let (slskd, folder) = slskd().await?;
    let s = served()?;
    let staging = Path::new(s.data_dir.as_deref().context("Cratarr's data folder is needed")?).join("track-fetch");
    tidy(&staging);
    let batch = slskd.fetch(&c.user, &c.offered).await?;
    let id = new_id();
    jobs(|j| {
        j.fetches.insert(id.clone(), FetchJob { wanted: wanted.clone(), view: FetchView { state: Fetching::Queued, percent: 0.0, why: None, score: None }, staged: None });
    });
    let job = id.clone();
    tokio::spawn(async move {
        let set = |f: &dyn Fn(&mut FetchJob)| {
            let changed = jobs(|j| j.fetches.get_mut(&job).map(|x| {
                let before = (x.view.state.clone(), x.view.percent.round());
                f(x);
                before != (x.view.state.clone(), x.view.percent.round())
            }));
            if changed == Some(true) {
                tell(&job);
            }
        };
        let got = follow(&slskd, &c, &batch, &folder, &staging.join(&job), &|p| {
            set(&|f| (f.view.state, f.view.percent) = (if p > 0.0 { Fetching::Downloading } else { Fetching::Queued }, p));
        })
        .await;
        let staged = match got {
            Ok(p) => p,
            Err(e) => {
                set(&|f| (f.view.state, f.view.why) = (Fetching::Refused, Some(format!("{e:#}"))));
                return;
            }
        };
        set(&|f| (f.view.state, f.view.percent, f.staged) = (Fetching::Checking, 100.0, Some(staged.clone())));
        let verdict = gate(&staged, &wanted.track).await;
        set(&|f| match &verdict {
            Ok(score) => (f.view.state, f.view.score) = (Fetching::Passed, Some(*score)),
            Err(e) => (f.view.state, f.view.why) = (Fetching::Refused, Some(format!("{e:#}"))),
        });
    });
    Ok(id)
}

/// A fetch's state as it changes, for the live channel (`fetch`: its id with its state).
#[derive(Debug, Clone, Serialize)]
pub struct Told {
    pub id: String,
    #[serde(flatten)]
    pub view: FetchView,
}

fn channel() -> &'static tokio::sync::broadcast::Sender<Told> {
    static C: std::sync::OnceLock<tokio::sync::broadcast::Sender<Told>> = std::sync::OnceLock::new();
    C.get_or_init(|| tokio::sync::broadcast::channel(64).0)
}

pub fn subscribe() -> tokio::sync::broadcast::Receiver<Told> {
    channel().subscribe()
}

/// The fetch changed: told to whoever listens.
fn tell(id: &str) {
    if let Some(view) = fetch_view(id) {
        let _ = channel().send(Told { id: id.to_string(), view });
    }
}

pub fn fetch_view(id: &str) -> Option<FetchView> {
    jobs(|j| j.fetches.get(id).map(|f| f.view.clone()))
}

/// The file staged and the fetch forgotten (a refused one dropped, or one the admin does not want).
pub fn drop_fetch(id: &str) {
    if let Some(f) = jobs(|j| j.fetches.remove(id))
        && let Some(dir) = f.staged.as_deref().and_then(Path::parent)
    {
        std::fs::remove_dir_all(dir).ok();
    }
}

/// Follows the transfer until it is done; the file moved to `stage`, and slskd's record of it taken away.
async fn follow(slskd: &Slskd, c: &Candidate, batch: &str, folder: &Path, stage: &Path, told: &(dyn Fn(f64) + Sync)) -> Result<PathBuf> {
    let started = std::time::Instant::now();
    let t = loop {
        let t = slskd.transfer(batch).await?;
        if t.done() {
            break t;
        }
        if t.percent_complete <= 0.0 && started.elapsed() > QUEUED_FOR {
            slskd.drop_transfer(&c.user, &t.id).await;
            bail!("{} did not send it", c.user);
        }
        told(t.percent_complete);
        tokio::time::sleep(Duration::from_secs(2)).await;
    };
    if !t.succeeded() {
        slskd.drop_transfer(&c.user, &t.id).await;
        bail!("The download failed{}", t.exception.map(|e| format!(": {e}")).unwrap_or_default());
    }
    let dir = folder.join(crate::slskd::FOLDER).join(batch);
    let name = crate::slskd::base_name(&c.file).to_string();
    let (dir2, stage2) = (dir.clone(), stage.to_path_buf());
    let staged = tokio::task::spawn_blocking(move || -> Result<PathBuf> {
        let from = std::fs::read_dir(&dir2)
            .with_context(|| format!("slskd's folder {} cannot be read", dir2.display()))?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.is_file())
            .with_context(|| format!("{name} is not in slskd's folder"))?;
        std::fs::create_dir_all(&stage2)?;
        let to = stage2.join(from.file_name().context("no file name")?);
        if std::fs::rename(&from, &to).is_err() {
            std::fs::copy(&from, &to).context("moving it out of slskd's folder")?;
            std::fs::remove_file(&from).ok();
        }
        std::fs::remove_dir(&dir2).ok();
        Ok(to)
    })
    .await??;
    slskd.drop_transfer(&c.user, &t.id).await;
    Ok(staged)
}

/// Staged files of fetches gone (a restart) or never added, taken away after a day.
fn tidy(staging: &Path) {
    let Ok(entries) = std::fs::read_dir(staging) else { return };
    for e in entries.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > STAGED_FOR);
        let known = jobs(|j| j.fetches.contains_key(&e.file_name().to_string_lossy().to_string()));
        if old && !known {
            std::fs::remove_dir_all(e.path()).ok();
        }
    }
}

/// The gate: decoded whole, of the recording's length, and the recording by AcoustID; its score.
async fn gate(file: &Path, track: &ReleaseTrack) -> Result<f32> {
    let f = file.to_path_buf();
    let print = tokio::task::spawn_blocking(move || crate::acoustid::print(&f)).await??;
    refused_for(&print, track.length_ms)?;
    let ud = served()?.userdata.clone();
    let key = tokio::task::spawn_blocking(move || acoustid_key(&ud)).await??.context("AcoustID is not set up: Settings › Server")?;
    let found = crate::acoustid::lookup(&key, &print).await?;
    crate::acoustid::names(&found, &track.recording).context("AcoustID does not know this file as that recording")
}

/// Why a file is refused before AcoustID is asked: a damaged frame, or a length not the recording's.
fn refused_for(print: &crate::acoustid::Print, length_ms: Option<i64>) -> Result<()> {
    if print.damaged > 0 {
        bail!("{} frames would not decode", print.damaged);
    }
    if let Some(want) = length_ms {
        let off = (print.duration_secs * 1000.0).round() as i64 - want;
        if off.abs() > crate::slskd::LENGTH_SLACK_MS {
            bail!("It is {:.0} s {} than the recording", off.abs() as f64 / 1000.0, if off > 0 { "longer" } else { "shorter" });
        }
    }
    Ok(())
}

// ---- 4. Add to album

/// The album-wide tags a new track takes from its neighbour.
const ALBUM_KEYS: [lofty::tag::ItemKey; 15] = {
    use lofty::tag::ItemKey::*;
    [
        AlbumTitle,
        AlbumArtist,
        RecordingDate,
        Year,
        ReleaseDate,
        OriginalReleaseDate,
        Genre,
        Label,
        CatalogNumber,
        Barcode,
        TrackTotal,
        DiscTotal,
        MusicBrainzReleaseId,
        MusicBrainzReleaseGroupId,
        MusicBrainzReleaseArtistId,
    ]
};

/// A new track's file name, as its neighbour's is made: the neighbour's with its number and title swapped for the new
/// one's (its number's width kept); "NN Title" where the neighbour's name has no title in it.
fn name_like(neighbour: &str, number: u32, title: &str, ext: &str) -> String {
    let safe: String = title.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c }).collect();
    let stem = neighbour.rsplit_once('.').map_or(neighbour, |(s, _)| s);
    let digits = stem.chars().take_while(char::is_ascii_digit).count();
    let width = if digits == 0 { 2 } else { digits };
    let num = format!("{number:0width$}");
    // The neighbour's own title is what follows its number and separator, whatever the separator.
    let rest = &stem[digits..];
    let lead: String = rest.chars().take_while(|c| !c.is_alphanumeric()).collect();
    let made = if digits > 0 && !rest.is_empty() {
        let rest_title = &rest[lead.len()..];
        // "Artist - Title": the part after the last " - " is the title.
        match rest_title.rsplit_once(" - ") {
            Some((before, _)) => format!("{num}{lead}{before} - {safe}"),
            None => format!("{num}{lead}{safe}"),
        }
    } else {
        format!("{num} {safe}")
    };
    format!("{made}.{ext}")
}

/// What Undo needs.
#[derive(Debug, Serialize, Deserialize)]
struct Added {
    root: PathBuf,
    /// The new track, relative to the root.
    added: String,
    /// The track it replaced, in the trash.
    trashed: Option<String>,
}

fn undo_file(id: &str) -> Result<PathBuf> {
    let data = served()?.data_dir.clone().context("Cratarr's data folder is needed")?;
    Ok(Path::new(&data).join("track-fetch-undo").join(format!("{id}.json")))
}

/// Adds a passed fetch to its album; the undo token.
pub async fn add(fetch: &str) -> Result<String> {
    let (wanted, staged) = jobs(|j| {
        let f = j.fetches.get(fetch)?;
        (f.view.state == Fetching::Passed).then(|| (f.wanted.clone(), f.staged.clone()))
    })
    .context("That track has not passed the check")?;
    let staged = staged.context("That track is not staged")?;
    let s = served()?;
    let token = tokio::task::spawn_blocking(move || place_track(&s, &wanted, &staged)).await??;
    jobs(|j| {
        if let Some(f) = j.fetches.get_mut(fetch) {
            f.view.state = Fetching::Added;
        }
    });
    tell(fetch);
    drop_fetch(fetch);
    Ok(token)
}

fn place_track(s: &crate::serve::Serve, wanted: &Wanted, staged: &Path) -> Result<String> {
    let (folder, dir, ..) = s.library.album_place(&wanted.album)?.context("No such album in Cratarr's own library")?;
    let root = PathBuf::from(s.library.roots().into_iter().find(|(f, _)| *f == folder).map(|(_, r)| r).context("Its library is not there")?);
    let (tracks, ..) = super::renumber::tracks_of(&wanted.album)?;
    let t = &wanted.track;
    let replaced = wanted.replace.as_ref().map(|id| tracks.iter().find(|x| &x.id == id).context("The track to replace is gone")).transpose()?;
    // The neighbour: the closest track on its disc, else any other.
    let neighbour = tracks
        .iter()
        .filter(|x| replaced.is_none_or(|r| r.id != x.id))
        .min_by_key(|x| (x.disc.unwrap_or(1) != t.disc, x.track.map_or(u32::MAX, |n| n.abs_diff(t.position))))
        .context("The album has no other track to take its tags from")?;
    let album_dir = root.join(&dir);
    // Hi-res is made CD's 16 bit / 44.1 kHz as Incoming's are (decision 0013).
    let source = if crate::convert::needs_conversion(staged)?.is_some() {
        let cd = staged.with_extension("cd.flac");
        crate::convert::to_cd_flac(staged, &cd)?;
        cd
    } else {
        staged.to_path_buf()
    };
    let ext = source.extension().and_then(|x| x.to_str()).unwrap_or("flac").to_ascii_lowercase();
    let name = name_like(&neighbour.file, t.position, &t.title, &ext);
    let dest = album_dir.join(&name);
    // The replaced track to the trash first: the new one may take its name.
    let now = super::moment::now_secs();
    let trashed = match replaced {
        Some(r) => Some(crate::trash::trash_file(&root, &rel_of(&root, &r.path)?, now)?),
        None => None,
    };
    if dest.exists() {
        if let Some(tok) = &trashed {
            crate::trash::restore_file(&root, tok).ok();
        }
        bail!("{name} is there already");
    }
    let placed = (|| -> Result<()> {
        // Copied in under a hidden name, then renamed: no walk sees a half-written track.
        let part = album_dir.join(format!(".{name}.cratarr-part"));
        std::fs::copy(&source, &part).with_context(|| format!("copying into {}", album_dir.display()))?;
        if let Ok(m) = std::fs::metadata(&neighbour.path) {
            std::fs::set_permissions(&part, m.permissions()).ok();
        }
        std::fs::rename(&part, &dest)?;
        tag(&dest, &neighbour.path, t)
    })();
    if let Err(e) = placed {
        std::fs::remove_file(&dest).ok();
        if let Some(tok) = &trashed {
            crate::trash::restore_file(&root, tok).ok();
        }
        return Err(e);
    }
    // The album's cover inside it too (the owner's rule: inside every file and next to them).
    if let Err(e) = crate::covers_inside::complete(&album_dir) {
        tracing::warn!("track fetch: the cover not put inside {}: {e:#}", dest.display());
    }
    let id = now.to_string() + &new_id()[..4];
    let undo = Added { root: root.clone(), added: rel_of(&root, &dest)?, trashed };
    let file = undo_file(&id)?;
    std::fs::create_dir_all(file.parent().context("no folder")?)?;
    std::fs::write(&file, serde_json::to_vec(&undo)?)?;
    let mut changed = vec![dest];
    changed.extend(replaced.map(|r| r.path.clone()));
    s.reread(&changed);
    s.catalogue().poke();
    Ok(format!("{UNDO}{id}"))
}

fn rel_of(root: &Path, p: &Path) -> Result<String> {
    Ok(p.strip_prefix(root).context("not inside its library")?.to_string_lossy().replace('\\', "/"))
}

/// The new track's tags: the album's from its neighbour, its own from MusicBrainz; whatever the uploader's file had
/// taken away first (its pictures too: the album's cover goes in after).
fn tag(track: &Path, neighbour: &Path, t: &ReleaseTrack) -> Result<()> {
    use lofty::prelude::*;
    use lofty::tag::{ItemKey, ItemValue, TagItem};
    let theirs: Vec<TagItem> = lofty::read_from_path(neighbour)
        .ok()
        .and_then(|f| f.primary_tag().cloned())
        .map(|tag| tag.items().filter(|i| ALBUM_KEYS.contains(&i.key())).cloned().collect())
        .unwrap_or_default();
    let width = super::fixes::values(neighbour, &["tracknumber"])?.into_iter().next().flatten().filter(|n| n.starts_with('0')).map_or(1, |n| n.len());
    let own: Vec<(ItemKey, String)> = [
        (ItemKey::TrackTitle, Some(t.title.clone())),
        (ItemKey::TrackArtist, Some(t.artist.clone())),
        (ItemKey::TrackNumber, Some(format!("{:0width$}", t.position))),
        (ItemKey::DiscNumber, Some(t.disc.to_string())),
        (ItemKey::MusicBrainzRecordingId, Some(t.recording.clone()).filter(|r| !r.is_empty())),
        (ItemKey::MusicBrainzTrackId, Some(t.track_id.clone()).filter(|r| !r.is_empty())),
        (ItemKey::MusicBrainzArtistId, t.artist_id.clone()),
    ]
    .into_iter()
    .filter_map(|(k, v)| Some((k, v?)))
    .collect();
    let (title, recording) = (t.title.clone(), t.recording.clone());
    crate::retag::rewrite(
        track,
        |tag| {
            tag.clear();
            for i in theirs {
                tag.insert(i);
            }
            for (k, v) in own {
                tag.insert(TagItem::new(k, ItemValue::Text(v)));
            }
        },
        |copy| {
            let got = super::fixes::values(copy, &["title", "musicbrainz_trackid"])?;
            if got[0].as_deref() != Some(title.as_str()) || (!recording.is_empty() && got[1].as_deref() != Some(recording.as_str())) {
                bail!("its tags did not read back as written");
            }
            Ok(())
        },
    )
}

/// Undoes an Add to album: the new track to the trash, the one it replaced back.
pub async fn undo(token: &str) -> Result<()> {
    let id = token.strip_prefix(UNDO).filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())).context("Not an added track's undo")?.to_string();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let file = undo_file(&id)?;
        let a: Added = serde_json::from_slice(&std::fs::read(&file).context("That can no longer be undone")?)?;
        let now = super::moment::now_secs();
        crate::trash::trash_file(&a.root, &a.added, now)?;
        let mut changed = vec![a.root.join(&a.added)];
        if let Some(tok) = &a.trashed {
            let back = crate::trash::restore_file(&a.root, tok)?;
            changed.push(a.root.join(back));
        }
        std::fs::remove_file(&file).ok();
        if let Some(s) = crate::serve::instance() {
            s.reread(&changed);
            s.catalogue().poke();
        }
        Ok(())
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_track_is_named_as_its_neighbour_is() {
        assert_eq!(name_like("05 Euphoria.flac", 6, "Bloom", "flac"), "06 Bloom.flac");
        assert_eq!(name_like("05 - Euphoria.flac", 6, "Bloom", "mp3"), "06 - Bloom.mp3");
        assert_eq!(name_like("05 Arushi Jain - Euphoria.flac", 6, "Bloom: Part 1", "flac"), "06 Arushi Jain - Bloom_ Part 1.flac");
        assert_eq!(name_like("Euphoria.flac", 6, "Bloom", "flac"), "06 Bloom.flac");
        assert_eq!(name_like("005 Euphoria.flac", 12, "Bloom", "flac"), "012 Bloom.flac");
    }

    #[test]
    fn soulseek_is_asked_for_words() {
        assert_eq!(query("Arushi Jain", "Delight (Reprise) — Part 2"), "Arushi Jain Delight Reprise Part 2");
    }

    #[test]
    fn a_file_off_the_recordings_length_or_damaged_is_refused_before_acoustid() {
        let p = |secs: f64, damaged: u32| crate::acoustid::Print { fingerprint: String::new(), duration_secs: secs, damaged };
        assert!(refused_for(&p(240.0, 0), Some(241_500)).is_ok());
        assert_eq!(refused_for(&p(236.0, 0), Some(240_000)).unwrap_err().to_string(), "It is 4 s shorter than the recording");
        assert_eq!(refused_for(&p(240.0, 3), Some(240_000)).unwrap_err().to_string(), "3 frames would not decode");
        assert!(refused_for(&p(240.0, 0), None).is_ok());
    }

    #[test]
    fn the_release_lists_every_track_with_the_file_that_holds_it() {
        let v = serde_json::json!({
            "id": "rel", "title": "Delight", "date": "2023-03-10",
            "artist-credit": [{ "name": "Arushi Jain", "joinphrase": "", "artist": { "id": "art" } }],
            "media": [{ "position": 1, "tracks": [
                { "id": "t1", "position": 1, "title": "Euphoria", "length": 300_000, "recording": { "id": "r1" }, "artist-credit": [{ "name": "Arushi Jain", "artist": { "id": "art" } }] },
                { "id": "t2", "position": 2, "title": "Bloom", "length": 250_000, "recording": { "id": "r2" }, "artist-credit": [{ "name": "Arushi Jain", "artist": { "id": "art" } }] }
            ]}]
        });
        let mut view = release_of(&v);
        assert_eq!((view.year, view.artist.as_str(), view.tracks.len()), (Some(2023), "Arushi Jain", 2));
        let files = vec![super::super::renumber::Track { id: "f1".into(), path: PathBuf::from("/m/a/01 Euphoria.flac"), file: "01 Euphoria.flac".into(), title: "Euphoria".into(), secs: 301, disc: Some(1), track: Some(1) }];
        place(&mut view, &files);
        assert_eq!(view.tracks[0].have.as_ref().map(|h| h.id.as_str()), Some("f1"));
        assert!(view.tracks[1].have.is_none(), "Bloom is missing");
        assert_eq!((view.tracks[1].recording.as_str(), view.tracks[1].track_id.as_str()), ("r2", "t2"));
    }
}
