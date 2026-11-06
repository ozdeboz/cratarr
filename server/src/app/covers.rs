//! Change cover (0.9.5, the owner: not happy with an album's cover, look up others and choose one): an album's cover
//! candidates (its Discogs master's and some of its releases', the Cover Art Archive's), and the one chosen written as
//! a file beside the tracks and inside each (the owner's rule for covers), in every copy of the record (0101): the picture
//! fetched once, and each copy written and put back by itself, as copies can hold different pictures. Written ahead: before
//! anything changes, a copy's old cover file and old picture inside are kept in the data folder with what Undo needs, under
//! the undo token. Cratarr's own library only: the albums whose files it holds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use super::reach::Sent as _;

/// The most of an image's bytes taken: a FLAC's picture holds at most 16 MiB, and every track carries it.
const MOST_BYTES: usize = 12 * 1024 * 1024;
/// Discogs releases of the master looked at for their own images.
const RELEASES: usize = 6;
/// A look's candidates kept this long, so the page asking again right after gets them at once.
const LOOKED: Duration = Duration::from_secs(10 * 60);
/// The look's steps, as the page shows them ("Discogs · 3 of 7").
const MUSICBRAINZ: &str = "MusicBrainz";
const ARCHIVE: &str = "Cover Art Archive";
const DISCOGS: &str = "Discogs";

/// One cover to choose from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// `discogs` or `coverartarchive`.
    pub from: &'static str,
    /// What it is: "Master", "CD · Germany · 2012", "Front".
    pub what: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Current {
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Covers {
    pub current: Option<Current>,
    pub candidates: Vec<Candidate>,
}

/// A library album's place and names: its folder, its MusicBrainz release, artist and name.
struct Place {
    dir: PathBuf,
    release: Option<String>,
    artist: String,
    name: String,
}

fn place(album: &str) -> Result<Place> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let (release, artist, name) = served.library.album_names(album)?.context("No such album in Cratarr's own library")?;
    let copy = served.library.row_copy(album)?.context("No such album in Cratarr's own library")?;
    let root = served.library.roots().into_iter().find(|(f, _)| *f == copy.folder).map(|(_, r)| r).context("Its library is not there")?;
    Ok(Place { dir: Path::new(&root).join(copy.dir), release: release.filter(|r| super::discogs::is_mbid(r)), artist, name })
}

/// One copy of a record, as a cover goes to it: its folder, and its files.
pub(super) struct CopyAt {
    pub dir: PathBuf,
    tracks: Vec<PathBuf>,
}

/// What a cover goes to: every copy of a record, the copy that plays first, or the copy at (catalogue folder, folder in it)
/// alone. Its files are the library's, not what lies in its folder: `dir` is only what they share, and can be a parent of
/// other records.
pub(super) fn copies(served: &crate::serve::Serve, album: &str, at: Option<&(i64, String)>) -> Result<Vec<CopyAt>> {
    let roots = served.library.roots();
    let row = served.library.row_copy(album)?;
    let files = served.library.files_of_record(album)?;
    let mut all = served.library.copies_of(album)?;
    all.retain(|c| at.is_none_or(|(folder, dir)| c.folder == *folder && c.dir == *dir));
    all.sort_by_key(|c| row.as_ref() != Some(c));
    all.into_iter()
        .map(|c| {
            let root = Path::new(&roots.iter().find(|(f, _)| *f == c.folder).context("Its library is not there")?.1).to_path_buf();
            let prefix = format!("{}/", c.dir);
            let tracks = files.iter().filter(|f| f.1 == c.folder && (c.dir.is_empty() || f.2.starts_with(&prefix))).map(|f| root.join(&f.2)).collect();
            Ok(CopyAt { dir: root.join(&c.dir), tracks })
        })
        .collect()
}

/// What a look tells as it goes, and asks: where it is (its step's name, how many done, of how many where known), the
/// covers found so far (0.9.6, the owner: they show as they come), and whether to stop (a cover picked).
struct Watch<'a> {
    progress: &'a (dyn Fn(&str, u64, Option<u64>) + Send + Sync),
    found: &'a (dyn Fn(&Covers) + Send + Sync),
    stop: &'a (dyn Fn() -> bool + Send + Sync),
}

fn looked() -> &'static Mutex<HashMap<String, (Instant, Vec<Candidate>)>> {
    static LOOKED_LIST: OnceLock<Mutex<HashMap<String, (Instant, Vec<Candidate>)>>> = OnceLock::new();
    LOOKED_LIST.get_or_init(Default::default)
}

/// An album's covers to choose from, and its own (Cratarr's own library: `cratarr` albums): a look's just made, else
/// looked for now.
pub async fn candidates(album: &str) -> Result<Covers> {
    look(album, &Watch { progress: &|_, _, _| {}, found: &|_| {}, stop: &|| false }).await
}

/// Looks for an album's covers as a task (0.9.6, the owner: Change cover's wait as Discover's, with its progress);
/// the task's result is `Covers`, as `GET …/covers` gives them, and while it runs the covers found so far. Stopped
/// (`DELETE /api/tasks/{id}`), it ends before its next outside call with what it found. A look running for the album is
/// that one.
pub fn start(account: &str, album: &str) -> Result<super::tasks::Task, String> {
    let kind = format!("covers:{album}");
    if let Some(t) = super::tasks::tasks().of(account).into_iter().find(|t| t.kind == kind && t.finished.is_none()) {
        return Ok(t);
    }
    let a = album.to_string();
    super::tasks::spawn(&kind, account, false, move |h| async move {
        let (h1, h2, h3) = (h.clone(), h.clone(), h);
        let progress = move |name: &str, done: u64, total: Option<u64>| h1.step(name, done, total);
        let found = move |c: &Covers| h2.found(c);
        let stop = move || h3.stopped();
        look(&a, &Watch { progress: &progress, found: &found, stop: &stop }).await
    })
}

/// The look: the album's release group from MusicBrainz, then the Cover Art Archive and Discogs side by side.
async fn look(album: &str, w: &Watch<'_>) -> Result<Covers> {
    let progress = w.progress;
    let a = album.to_string();
    let p = tokio::task::spawn_blocking(move || place(&a)).await??;
    let current = crate::serve::art::cover_file(&p.dir).map(|file| {
        let (width, height) = image::image_dimensions(&file).map_or((None, None), |(w, h)| (Some(w), Some(h)));
        Current { url: format!("/api/albums/{}/{album}/cover", crate::source::cratarr::KIND.id), width, height }
    });
    let kept = looked().lock().unwrap_or_else(|e| e.into_inner()).get(album).filter(|(at, _)| at.elapsed() < LOOKED).map(|(_, c)| c.clone());
    if let Some(candidates) = kept {
        return Ok(Covers { current, candidates });
    }
    let http = super::discover::http()?;
    progress(MUSICBRAINZ, 0, Some(1));
    let rg = match &p.release {
        Some(r) => super::discogs::release_group_of(&http, r).await.unwrap_or_default(),
        None => None,
    };
    progress(MUSICBRAINZ, 1, Some(1));
    // What each source found so far, told as one list (the Archive's first, as in the end).
    let lists = Mutex::new([Vec::<Candidate>::new(), Vec::new()]);
    let got = |source: usize, more: Vec<Candidate>| {
        if more.is_empty() {
            return;
        }
        let mut l = lists.lock().unwrap_or_else(|e| e.into_inner());
        l[source].extend(more);
        (w.found)(&Covers { current: current.clone(), candidates: one_list(l[0].iter().chain(&l[1]).cloned()) });
    };
    // Each source's count; the step shown is Discogs' while it runs (the longer), else the Archive's.
    let counts = Mutex::new([(false, 0u64, None::<u64>); 2]);
    let tell = |source: usize, done: bool, n: u64, total: Option<u64>| {
        let mut c = counts.lock().unwrap_or_else(|e| e.into_inner());
        c[source] = (done, n, total);
        let (name, (_, n, total)) = if !c[1].0 { (DISCOGS, c[1]) } else { (ARCHIVE, c[0]) };
        progress(name, n, total);
    };
    let (archive_told, discogs_told) = (|n, t| tell(0, n == t, n, Some(t)), |n, t| tell(1, false, n, t));
    let (archive_got, discogs_got) = (|more| got(0, more), |more| got(1, more));
    tokio::join!(archive(&http, p.release.as_deref(), rg.as_deref(), &archive_told, &archive_got, w.stop), async {
        discogs(rg.as_deref(), &p.artist, &p.name, &discogs_told, &discogs_got, w.stop).await;
        tell(1, true, 0, None);
    });
    let out = {
        let l = lists.lock().unwrap_or_else(|e| e.into_inner());
        one_list(l[0].iter().chain(&l[1]).cloned())
    };
    // Stopped part-way: not kept as the album's whole list.
    if (w.stop)() {
        return Ok(Covers { current, candidates: out });
    }
    let mut kept = looked().lock().unwrap_or_else(|e| e.into_inner());
    kept.retain(|_, (at, _)| at.elapsed() < LOOKED);
    kept.insert(album.to_string(), (Instant::now(), out.clone()));
    Ok(Covers { current, candidates: out })
}

/// Candidates once each, in their order.
fn one_list(all: impl Iterator<Item = Candidate>) -> Vec<Candidate> {
    let mut seen = std::collections::HashSet::new();
    all.filter(|c| seen.insert(c.url.clone())).collect()
}

/// The Cover Art Archive's: the release's and its release group's (`tell`: read, of how many; `got`: each one's, as
/// read; `stop`: asked before each).
async fn archive(
    http: &reqwest::Client,
    release: Option<&str>,
    rg: Option<&str>,
    tell: &(dyn Fn(u64, u64) + Sync),
    got: &(dyn Fn(Vec<Candidate>) + Sync),
    stop: &(dyn Fn() -> bool + Send + Sync),
) {
    let asked: Vec<String> = [release.map(|r| format!("release/{r}")), rg.map(|g| format!("release-group/{g}"))].into_iter().flatten().collect();
    let total = asked.len() as u64;
    tell(0, total);
    for (n, what) in asked.iter().enumerate() {
        if stop() {
            break;
        }
        let mut out = Vec::new();
        if let Ok(r) = http.get(format!("https://coverartarchive.org/{what}")).sent().await
            && r.status().is_success()
            && let Ok(v) = r.json::<serde_json::Value>().await
        {
            for i in v["images"].as_array().into_iter().flatten() {
                // Its 1200 px JPEG where the Archive has one: the original upload can be a PNG of many MB, and every
                // track carries the cover (seen on the sim, 2026-10-26: 8 MB in each track).
                let picked = i["thumbnails"]["1200"].as_str().or(i["thumbnails"]["large"].as_str()).or(i["image"].as_str());
                let Some(url) = picked.map(|u| u.replace("http://", "https://")) else { continue };
                let types: Vec<&str> = i["types"].as_array().into_iter().flatten().filter_map(|t| t.as_str()).collect();
                out.push(Candidate { url, width: None, height: None, from: "coverartarchive", what: if types.is_empty() { "Image".into() } else { types.join(" · ") } });
            }
        }
        got(out);
        tell(n as u64 + 1, total);
    }
}

/// Discogs': the master's, then some of its releases' own (`tell`: read, of how many once known: the master and its
/// releases; `got`: each one's, as read; `stop`: asked before each call, so the calls not made leave Discogs' pace to
/// the next look).
async fn discogs(
    rg: Option<&str>,
    artist: &str,
    name: &str,
    tell: &(dyn Fn(u64, Option<u64>) + Sync),
    got: &(dyn Fn(Vec<Candidate>) + Sync),
    stop: &(dyn Fn() -> bool + Send + Sync),
) {
    tell(0, None);
    if stop() {
        return;
    }
    let Ok(Some(id)) = super::discogs::master_of(rg, artist, name).await else { return };
    if stop() {
        return;
    }
    let mut main = None;
    if let Ok(Some(m)) = super::discogs::master(id).await {
        main = m.main_release;
        got(m.images.into_iter().map(|i| Candidate { url: i.url, width: i.width, height: i.height, from: "discogs", what: "Master".into() }).collect());
    }
    tell(1, None);
    if stop() {
        return;
    }
    let releases = super::discogs::versions(id, main, RELEASES).await.unwrap_or_default();
    let total = Some(1 + releases.len() as u64);
    tell(1, total);
    for (n, (release, what)) in releases.into_iter().enumerate() {
        if stop() {
            return;
        }
        let images = super::discogs::release_images(release).await.unwrap_or_default();
        got(images.into_iter().map(|i| Candidate { url: i.url, width: i.width, height: i.height, from: "discogs", what: what.clone() }).collect());
        tell(2 + n as u64, total);
    }
}

/// Only images from these hosts are fetched: the candidates' own.
fn allowed(url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url) else { return false };
    let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
    u.scheme() == "https" && ["coverartarchive.org", "archive.org", "i.discogs.com", "img.discogs.com"].iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

/// What Undo needs of one copy, kept before anything changes (written ahead).
#[derive(Debug, Serialize, Deserialize)]
struct Kept {
    dir: PathBuf,
    /// The copy's tracks: what the picture goes inside.
    tracks: Vec<PathBuf>,
    /// The old cover file's name (its bytes beside, as `old-file`), where there was one.
    old_file: Option<String>,
    /// The old picture inside the first track (its bytes beside, as `old-inside`), where there was one.
    old_inside: Option<String>,
    /// The new cover file's name.
    new_file: String,
    /// The new picture's type (its bytes beside, as `new`).
    #[serde(default)]
    new_mime: Option<String>,
}

fn kept_dir(served: &crate::serve::Serve, id: &str) -> Result<PathBuf> {
    let data = served.data_dir.clone().context("Cratarr's data folder is needed")?;
    Ok(Path::new(&data).join("cover-undo").join(id))
}

/// The image at `url` (one of its candidates' hosts), fetched.
async fn fetch(url: &str) -> Result<Vec<u8>> {
    if !allowed(url) {
        bail!("That image is not one of the album's covers to choose from");
    }
    let r = super::discover::http()?.get(url).timeout(Duration::from_secs(60)).sent().await?.error_for_status()?;
    if r.content_length().is_some_and(|n| n as usize > MOST_BYTES) {
        bail!("That image is too large");
    }
    let data = r.bytes().await?.to_vec();
    if data.is_empty() || data.len() > MOST_BYTES {
        bail!("That image could not be read");
    }
    Ok(data)
}

/// An administrator's own picture (0.9.6, the owner: Upload cover, for everyone), kept as it was sent (JPEG, PNG or
/// WebP).
fn uploaded(data: &[u8]) -> Result<()> {
    if data.len() > MOST_BYTES {
        bail!("That picture is too large (at most {} MB)", MOST_BYTES / (1024 * 1024));
    }
    if !matches!(crate::serve::art::sniff(data), "image/png" | "image/webp") && !data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        bail!("Upload a JPEG, PNG or WebP picture");
    }
    Ok(())
}

/// The cover the administrator chose (`url`) or uploaded (`data`) as a run on the one path (decision 0092's writers'
/// map, 0.14: the choice is the yes; Undo is the run's).
pub async fn run(app: &std::sync::Arc<super::App>, album: &str, url: Option<&str>, data: Option<Vec<u8>>) -> Result<super::file_runs::Status> {
    let (data, from) = match (url, data) {
        (Some(u), _) => (fetch(u).await?, "chosen"),
        (None, Some(d)) => {
            uploaded(&d)?;
            (d, "uploaded")
        }
        (None, None) => bail!("Which picture?"),
    };
    let a = album.to_string();
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let p = tokio::task::spawn_blocking(move || proposal(&served, &a, &data, from)).await??;
    Ok(super::file_runs::start(app, p))
}

/// A picture as a run: one file of the run a copy, each said as it stands now. Blocking.
pub(super) fn proposal(served: &crate::serve::Serve, album: &str, data: &[u8], from: &str) -> Result<super::file_runs::Proposal> {
    let (_, artist, title) = served.library.album_names(album)?.context("No such album in Cratarr's own library")?;
    let (kept, said) = prepare(served, &copies(served, album, None)?, data)?;
    let files = kept
        .into_iter()
        .map(|(dir, keep)| crate::journal::FileWrite {
            album: album.to_string(),
            before: vec![("cover".into(), vec![crate::serve::art::cover_file(&dir).map_or("none".to_string(), |_| "its cover".to_string())])],
            after: vec![("cover".into(), vec![said.clone()])],
            from: vec![("cover".into(), from.into())],
            path: dir,
            track: None,
            undone: false,
            whole: Some(crate::journal::Whole { op: "cover".into(), ms: None, kept: Some(keep), planned: String::new(), wrote: None }),
        })
        .collect();
    Ok(super::file_runs::Proposal { kind: "cover".into(), title: format!("Cover · {title}"), records: vec![crate::journal::RunRecord { album: album.to_string(), title, artist }], files, failed: Vec::new() })
}

/// A candidate's picture fetched once and kept for each copy, ready for a run to write (a cover fix on Health's sheet or
/// before Keep): each copy's folder with where it is kept, and the picture as the sheet says it.
pub async fn prepared(served: &std::sync::Arc<crate::serve::Serve>, album: &str, url: &str, at: Option<&(i64, String)>) -> Result<(Vec<(PathBuf, PathBuf)>, String)> {
    let data = fetch(url).await?;
    let (served, a, at) = (served.clone(), album.to_string(), at.cloned());
    tokio::task::spawn_blocking(move || prepare(&served, &copies(&served, &a, at.as_ref())?, &data)).await?
}

/// A picture's type and its cover file's extension; refused where it is no image Cratarr can use (its header read
/// only: nothing is decoded).
fn kind_of(data: &[u8]) -> Result<(&'static str, &'static str)> {
    if image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format()?.into_dimensions().is_err() {
        bail!("That is not an image Cratarr can use");
    }
    let mime = crate::serve::art::sniff(data);
    let ext = match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => bail!("That is not an image Cratarr can use"),
    };
    Ok((mime, ext))
}

/// The new picture kept for each copy, ready to be written (nothing changed yet): each one's folder with where it is kept,
/// and the picture as the sheet says it ("1200×1200 JPEG"). Each copy its own: a `Kept` holds that copy's old pictures, and
/// goes when its Undo is done. Blocking.
fn prepare(served: &crate::serve::Serve, copies: &[CopyAt], data: &[u8]) -> Result<(Vec<(PathBuf, PathBuf)>, String)> {
    let (mime, ext) = kind_of(data)?;
    if copies.is_empty() {
        bail!("No such album in Cratarr's own library");
    }
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis();
    let mut out = Vec::new();
    for (n, copy) in copies.iter().enumerate() {
        let keep = kept_dir(served, &format!("{at}-{n}"))?;
        std::fs::create_dir_all(&keep)?;
        std::fs::write(keep.join("new"), data)?;
        let kept = Kept { dir: copy.dir.clone(), tracks: copy.tracks.clone(), old_file: None, old_inside: None, new_file: format!("cover.{ext}"), new_mime: Some(mime.to_string()) };
        std::fs::write(keep.join("kept.json"), serde_json::to_vec(&kept)?)?;
        out.push((copy.dir.clone(), keep));
    }
    let size = image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format().ok().and_then(|r| r.into_dimensions().ok());
    let kind = ext.to_uppercase().replace("JPG", "JPEG");
    Ok((out, size.map_or(kind.clone(), |(w, h)| format!("{w}×{h} {kind}"))))
}

/// The picture kept in `keep` (`prepare`) written into its copy: the old pictures kept first (written ahead), then the old cover file
/// out, the new one beside the tracks and inside each; the tracks read again. `ready` comes between the two: from there on
/// `undo_kept` puts it all back, however far this gets, and before there it would take the tracks' pictures out. Blocking.
pub fn write_kept(keep: &Path, ready: impl FnOnce()) -> Result<()> {
    let mut kept: Kept = serde_json::from_slice(&std::fs::read(keep.join("kept.json")).context("The picture is no longer kept")?)?;
    let data = std::fs::read(keep.join("new"))?;
    let mime = kept.new_mime.clone().unwrap_or_else(|| crate::serve::art::sniff(&data).to_string());
    let old_file = crate::serve::art::cover_file(&kept.dir);
    if let Some(f) = &old_file {
        std::fs::copy(f, keep.join("old-file"))?;
    }
    let tracks = kept.tracks.clone();
    let old_inside = tracks.iter().find_map(|t| crate::serve::art::embedded(t));
    if let Some((_, bytes)) = &old_inside {
        std::fs::write(keep.join("old-inside"), bytes)?;
    }
    kept.old_file = old_file.as_ref().and_then(|f| f.file_name()).map(|n| n.to_string_lossy().into_owned());
    kept.old_inside = old_inside.map(|(m, _)| m);
    std::fs::write(keep.join("kept.json"), serde_json::to_vec(&kept)?)?;
    ready();
    if let Some(f) = &old_file {
        std::fs::remove_file(f)?;
    }
    crate::covers_inside::write_file(&kept.dir.join(&kept.new_file), &data)?;
    for (t, why) in &crate::covers_inside::replace(&tracks, Some((&data, mime.as_str()))) {
        tracing::warn!("covers: {} kept its old picture: {why}", t.display());
    }
    if let Some(served) = crate::serve::instance() {
        served.reread(&tracks);
    }
    Ok(())
}

/// A cover change put back from what `write_kept` kept. Blocking.
pub fn undo_kept(keep: &Path) -> Result<()> {
    let kept: Kept = serde_json::from_slice(&std::fs::read(keep.join("kept.json")).context("That cover change can no longer be undone")?)?;
    let new = kept.dir.join(&kept.new_file);
    if new.exists() {
        std::fs::remove_file(&new)?;
    }
    if let Some(name) = &kept.old_file {
        let back = kept.dir.join(name);
        if !back.exists() {
            std::fs::copy(keep.join("old-file"), &back)?;
        }
    }
    let tracks = kept.tracks.clone();
    let old = match &kept.old_inside {
        Some(mime) => Some((std::fs::read(keep.join("old-inside"))?, mime.clone())),
        None => None,
    };
    for (t, why) in crate::covers_inside::replace(&tracks, old.as_ref().map(|(b, m)| (b.as_slice(), m.as_str()))) {
        tracing::warn!("covers: {} not given its old picture back: {why}", t.display());
    }
    std::fs::remove_dir_all(keep).ok();
    if let Some(served) = crate::serve::instance() {
        served.reread(&tracks);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_candidates_hosts_are_fetched() {
        assert!(allowed("https://i.discogs.com/abc/rs:fit/x.jpg"));
        assert!(allowed("https://coverartarchive.org/release/1/front"));
        assert!(allowed("https://ia800.us.archive.org/x.jpg"));
        assert!(!allowed("http://i.discogs.com/x.jpg"), "https only");
        assert!(!allowed("https://evil.example/x.jpg"));
        assert!(!allowed("https://i.discogs.com.evil.example/x.jpg"));
        assert!(!allowed("https://localhost/x.jpg"));
    }

    #[test]
    fn an_upload_is_a_picture_of_a_size_a_flac_can_carry() {
        assert!(uploaded(b"not a picture").unwrap_err().to_string().contains("JPEG, PNG or WebP"));
        assert!(uploaded(&vec![0xFF; MOST_BYTES + 1]).unwrap_err().to_string().contains("too large"));
        assert!(kind_of(&[0xFF, 0xD8, 0xFF, 0x00, 0x01]).unwrap_err().to_string().contains("not an image"), "a JPEG's mark, no picture");
    }

    /// A folder with its cover file and two tracks with the old picture inside, and what `prepare` keeps of a new one.
    fn album_with_a_new_cover_kept(d: &Path) -> (PathBuf, Vec<PathBuf>) {
        std::fs::write(d.join("cover.jpg"), b"old file").unwrap();
        let tracks = vec![crate::retag::fixtures::flac(d, "one"), crate::retag::fixtures::flac(d, "two")];
        assert!(crate::covers_inside::replace(&tracks, Some((b"old inside", "image/jpeg"))).is_empty());
        let keep = d.join("keep");
        std::fs::create_dir_all(&keep).unwrap();
        std::fs::write(keep.join("new"), b"new picture").unwrap();
        let kept = Kept { dir: d.to_path_buf(), tracks: tracks.clone(), old_file: None, old_inside: None, new_file: "cover.jpg".into(), new_mime: Some("image/jpeg".into()) };
        std::fs::write(keep.join("kept.json"), serde_json::to_vec(&kept).unwrap()).unwrap();
        (keep, tracks)
    }

    fn inside(track: &Path) -> Vec<u8> {
        crate::serve::art::embedded(track).unwrap().1
    }

    #[test]
    fn a_cover_is_ready_for_its_undo_once_its_old_pictures_are_kept_and_before_anything_changes() {
        let d = crate::retag::fixtures::dir("cover-ready");
        let (keep, tracks) = album_with_a_new_cover_kept(&d);
        let mut heard = false;
        write_kept(&keep, || {
            heard = true;
            let kept: Kept = serde_json::from_slice(&std::fs::read(keep.join("kept.json")).unwrap()).unwrap();
            assert_eq!((kept.old_file.as_deref(), kept.old_inside.as_deref()), (Some("cover.jpg"), Some("image/jpeg")));
            assert_eq!(std::fs::read(keep.join("old-inside")).unwrap(), b"old inside");
            assert_eq!(std::fs::read(d.join("cover.jpg")).unwrap(), b"old file", "nothing changed yet");
            assert_eq!(inside(&tracks[0]), b"old inside");
        })
        .unwrap();
        assert!(heard);
        assert_eq!(std::fs::read(d.join("cover.jpg")).unwrap(), b"new picture");
        std::fs::remove_dir_all(&d).ok();
    }

    /// What a stop leaves part way through the tracks: the new cover file, the first track done, the second not.
    #[test]
    fn a_cover_stopped_part_way_through_its_tracks_is_put_back_whole_by_its_undo() {
        let d = crate::retag::fixtures::dir("cover-cut");
        let (keep, tracks) = album_with_a_new_cover_kept(&d);
        write_kept(&keep, || ()).unwrap();
        assert!(crate::covers_inside::replace(&tracks[1..], Some((b"old inside", "image/jpeg"))).is_empty());
        assert_eq!((inside(&tracks[0]), inside(&tracks[1])), (b"new picture".to_vec(), b"old inside".to_vec()));

        undo_kept(&keep).unwrap();
        assert_eq!(std::fs::read(d.join("cover.jpg")).unwrap(), b"old file");
        assert_eq!((inside(&tracks[0]), inside(&tracks[1])), (b"old inside".to_vec(), b"old inside".to_vec()));
        std::fs::remove_dir_all(&d).ok();
    }
}
