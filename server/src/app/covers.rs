//! Change cover (0.9.5, the owner: not happy with an album's cover, look up others and choose one): an album's cover
//! candidates (its Discogs master's and some of its releases', the Cover Art Archive's), and the one chosen written as
//! a file beside the tracks and inside each (the owner's rule for covers). Written ahead: before anything changes, the
//! old cover file and the old picture inside are kept in the data folder with what Undo needs, under the undo token.
//! Cratarr's own library only: the albums whose files it holds.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// What an undo token starts with.
pub const UNDO: char = 'v';
/// The most of an image's bytes read.
const MOST_BYTES: usize = 25 * 1024 * 1024;
/// Discogs releases of the master looked at for their own images.
const RELEASES: usize = 6;

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

/// An album's covers to choose from, and its own (Cratarr's own library: `cratarr` albums).
pub async fn candidates(album: &str) -> Result<Covers> {
    let a = album.to_string();
    let p = tokio::task::spawn_blocking(move || place(&a)).await??;
    let current = crate::serve::art::cover_file(&p.dir).map(|file| {
        let (width, height) = image::image_dimensions(&file).map_or((None, None), |(w, h)| (Some(w), Some(h)));
        Current { url: format!("/api/albums/{}/{album}/cover", crate::source::cratarr::KIND.id), width, height }
    });
    let http = super::discover::http()?;
    let rg = match &p.release {
        Some(r) => super::discogs::release_group_of(&http, r).await.unwrap_or_default(),
        None => None,
    };
    let mut out: Vec<Candidate> = Vec::new();
    // The Cover Art Archive's, the release's and its release group's.
    let archive = [p.release.as_ref().map(|r| format!("release/{r}")), rg.as_ref().map(|g| format!("release-group/{g}"))];
    for what in archive.into_iter().flatten() {
        if let Ok(r) = http.get(format!("https://coverartarchive.org/{what}")).send().await
            && r.status().is_success()
            && let Ok(v) = r.json::<serde_json::Value>().await
        {
            for i in v["images"].as_array().into_iter().flatten() {
                let Some(url) = i["image"].as_str().map(|u| u.replace("http://", "https://")) else { continue };
                let types: Vec<&str> = i["types"].as_array().into_iter().flatten().filter_map(|t| t.as_str()).collect();
                out.push(Candidate { url, width: None, height: None, from: "coverartarchive", what: if types.is_empty() { "Image".into() } else { types.join(" · ") } });
            }
        }
    }
    // Discogs': the master's, then some of its releases' own.
    if let Ok(Some(id)) = super::discogs::master_of(rg.as_deref(), &p.artist, &p.name).await {
        if let Ok(Some(m)) = super::discogs::master(id).await {
            out.extend(m.images.into_iter().map(|i| Candidate { url: i.url, width: i.width, height: i.height, from: "discogs", what: "Master".into() }));
        }
        for (release, what) in super::discogs::versions(id, RELEASES).await.unwrap_or_default() {
            for i in super::discogs::release_images(release).await.unwrap_or_default() {
                out.push(Candidate { url: i.url, width: i.width, height: i.height, from: "discogs", what: what.clone() });
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c.url.clone()));
    Ok(Covers { current, candidates: out })
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
    let r = super::discover::http()?.get(url).timeout(Duration::from_secs(60)).send().await?.error_for_status()?;
    if r.content_length().is_some_and(|n| n as usize > MOST_BYTES) {
        bail!("That image is too large");
    }
    let data = r.bytes().await?.to_vec();
    if data.is_empty() || data.len() > MOST_BYTES {
        bail!("That image could not be read");
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
        Ok(format!("{UNDO}{id}"))
    })
    .await??;
    if let Some(served) = crate::serve::instance() {
        served.catalogue().poke();
    }
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
        Ok(())
    })
    .await??;
    if let Some(served) = crate::serve::instance() {
        served.catalogue().poke();
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
}
