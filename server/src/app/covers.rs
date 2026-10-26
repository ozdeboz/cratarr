//! Change cover (0.9.5, the owner: not happy with an album's cover, look up others and choose one): an album's cover
//! candidates (its Discogs master's and some of its releases', the Cover Art Archive's), and the one chosen written as
//! a file beside the tracks and inside each (the owner's rule for covers). Written ahead: before anything changes, the
//! old cover file and the old picture inside are kept in the data folder with what Undo needs, under the undo token.
//! Cratarr's own library only: the albums whose files it holds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use super::reach::Sent as _;

/// What an undo token starts with.
pub const UNDO: char = 'v';
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
    let (folder, dir, release, artist, name) = served.library.album_place(album)?.context("No such album in Cratarr's own library")?;
    let root = served.library.roots().into_iter().find(|(f, _)| *f == folder).map(|(_, r)| r).context("Its library is not there")?;
    Ok(Place { dir: Path::new(&root).join(dir), release: release.filter(|r| super::discogs::is_mbid(r)), artist, name })
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

/// What Undo needs, kept before anything changes (written ahead).
#[derive(Debug, Serialize, Deserialize)]
struct Kept {
    dir: PathBuf,
    /// The old cover file's name (its bytes beside, as `old-file`), where there was one.
    old_file: Option<String>,
    /// The old picture inside the first track (its bytes beside, as `old-inside`), where there was one.
    old_inside: Option<String>,
    /// The new cover file's name.
    new_file: String,
}

fn kept_dir(id: &str) -> Result<PathBuf> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let data = served.data_dir.clone().context("Cratarr's data folder is needed")?;
    Ok(Path::new(&data).join("cover-undo").join(id))
}

/// Sets an album's cover to the image at `url` (one of its candidates' hosts): written as `cover.<ext>` beside its
/// tracks (the old cover file taken away) and inside each FLAC and MP3 track. Returns the undo token.
pub async fn set(album: &str, url: &str) -> Result<String> {
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
    write(album, data).await
}

/// Sets an album's cover to a picture of the administrator's own (0.9.6, the owner: Upload cover, for everyone), as a
/// chosen one is set; kept as it was sent (JPEG, PNG or WebP). Returns the undo token.
pub async fn upload(album: &str, data: Vec<u8>) -> Result<String> {
    if data.len() > MOST_BYTES {
        bail!("That picture is too large (at most {} MB)", MOST_BYTES / (1024 * 1024));
    }
    if !matches!(crate::serve::art::sniff(&data), "image/png" | "image/webp") && !data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        bail!("Upload a JPEG, PNG or WebP picture");
    }
    write(album, data).await
}

/// The picture written as the album's cover, beside its tracks and inside each, the old one kept for Undo first.
async fn write(album: &str, data: Vec<u8>) -> Result<String> {
    // A picture whose size can be read (its header only: nothing is decoded).
    if image::ImageReader::new(std::io::Cursor::new(&data)).with_guessed_format()?.into_dimensions().is_err() {
        bail!("That is not an image Cratarr can use");
    }
    let mime = crate::serve::art::sniff(&data);
    let ext = match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => bail!("That is not an image Cratarr can use"),
    };
    let a = album.to_string();
    let token = tokio::task::spawn_blocking(move || -> Result<String> {
        let p = place(&a)?;
        let id = format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis());
        let keep = kept_dir(&id)?;
        std::fs::create_dir_all(&keep)?;
        // Written ahead: the old pictures and what Undo needs, before anything changes.
        let old_file = crate::serve::art::cover_file(&p.dir);
        if let Some(f) = &old_file {
            std::fs::copy(f, keep.join("old-file"))?;
        }
        let tracks = crate::covers_inside::tracks_of(&p.dir);
        let old_inside = tracks.iter().find_map(|t| crate::serve::art::embedded(t));
        if let Some((_, bytes)) = &old_inside {
            std::fs::write(keep.join("old-inside"), bytes)?;
        }
        let new_file = format!("cover.{ext}");
        let kept = Kept {
            dir: p.dir.clone(),
            old_file: old_file.as_ref().and_then(|f| f.file_name()).map(|n| n.to_string_lossy().into_owned()),
            old_inside: old_inside.map(|(m, _)| m),
            new_file: new_file.clone(),
        };
        std::fs::write(keep.join("kept.json"), serde_json::to_vec(&kept)?)?;
        // Then the change: the old file out, the new one beside the tracks and inside each.
        if let Some(f) = &old_file {
            std::fs::remove_file(f)?;
        }
        crate::covers_inside::write_file(&p.dir.join(&new_file), &data)?;
        let done = crate::covers_inside::replace(&tracks, Some((&data, mime)));
        for (t, why) in &done {
            tracing::warn!("covers: {} kept its old picture: {why}", t.display());
        }
        if let Some(served) = crate::serve::instance() {
            served.reread(&tracks);
        }
        Ok(format!("{UNDO}{id}"))
    })
    .await??;
    Ok(token)
}

/// Undoes a cover change (`v…`): the new cover file taken away, the old one back, the old picture inside the tracks
/// again (none where there was none). Safe to repeat.
pub async fn undo(token: &str) -> Result<()> {
    let id = token.strip_prefix(UNDO).filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit())).context("Not a cover's undo")?.to_string();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let keep = kept_dir(&id)?;
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
        let tracks = crate::covers_inside::tracks_of(&kept.dir);
        let old = match &kept.old_inside {
            Some(mime) => Some((std::fs::read(keep.join("old-inside"))?, mime.clone())),
            None => None,
        };
        for (t, why) in crate::covers_inside::replace(&tracks, old.as_ref().map(|(b, m)| (b.as_slice(), m.as_str()))) {
            tracing::warn!("covers: {} not given its old picture back: {why}", t.display());
        }
        std::fs::remove_dir_all(&keep).ok();
        if let Some(served) = crate::serve::instance() {
            served.reread(&tracks);
        }
        Ok(())
    })
    .await??;
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

    #[tokio::test]
    async fn an_upload_is_a_picture_of_a_size_a_flac_can_carry() {
        let refused = |r: Result<String>| r.unwrap_err().to_string();
        assert!(refused(upload("al-x", b"not a picture".to_vec()).await).contains("JPEG, PNG or WebP"));
        assert!(refused(upload("al-x", vec![0xFF; MOST_BYTES + 1]).await).contains("too large"));
        assert!(refused(upload("al-x", vec![0xFF, 0xD8, 0xFF, 0x00, 0x01]).await).contains("not an image"), "a JPEG's mark, no picture");
    }
}
