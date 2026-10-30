//! Covers of albums not in the library, kept by Cratarr (0.9.5, the owner: Discover took long to show its covers): a
//! release group's (the Cover Art Archive's, else its Discogs master's) and a Bandcamp album's, each fetched once, kept
//! as a file in the data folder for a month and served from there. The Cover Art Archive took about 3 s a cover (two
//! redirects to archive.org) every time; a Bandcamp cover took a read of its album page, one page a second. A look's
//! covers are fetched ahead after it, one at a time.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use super::reach::Sent as _;

/// The data folder's folder for them.
const FOLDER: &str = "cover-cache";
/// A kept cover fetched again after this long.
const KEEP: Duration = Duration::from_secs(30 * 86_400);
/// A kept cover not asked for in this long is taken away.
const FORGET: Duration = Duration::from_secs(60 * 86_400);
/// The most of a cover's bytes read.
const MOST_BYTES: usize = 5 * 1024 * 1024;
/// Covers fetched at once (each one's bytes held meanwhile).
static FETCHING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(3);
/// A look's covers fetched ahead by one task at a time.
static WARMING: AtomicBool = AtomicBool::new(false);

/// Whose cover.
#[derive(Debug, Clone, PartialEq)]
pub enum Of {
    /// A MusicBrainz release group.
    ReleaseGroup(String),
    /// A Bandcamp album, by its page.
    Bandcamp(String),
}

impl Of {
    /// The album a cover address of Cratarr's own names (`/api/covers/release-group/…`, `/api/cover/bandcamp/…`).
    pub fn of_address(cover: &str) -> Option<Of> {
        if let Some(rg) = cover.strip_prefix("/api/covers/release-group/") {
            return super::discogs::is_mbid(rg).then(|| Of::ReleaseGroup(rg.to_string()));
        }
        let id = cover.strip_prefix(&format!("/api/cover/{}/", crate::bandcamp::SOURCE))?;
        crate::bandcamp::read_id(id).map(|(page, _)| Of::Bandcamp(page))
    }

    fn key(&self) -> String {
        match self {
            Of::ReleaseGroup(rg) => format!("rg:{}", rg.to_ascii_lowercase()),
            Of::Bandcamp(page) => format!("bc:{page}"),
        }
    }
}

fn file_of(of: &Of) -> Option<PathBuf> {
    use sha2::Digest as _;
    let data = crate::serve::instance()?.data_dir.clone()?;
    let hash = sha2::Sha256::digest(of.key().as_bytes());
    let name: String = hash.iter().take(16).map(|b| format!("{b:02x}")).collect();
    Some(data.join(FOLDER).join(name))
}

/// A kept cover, while it is fresh.
fn kept(file: &std::path::Path) -> Option<Vec<u8>> {
    let age = std::fs::metadata(file).ok()?.modified().ok()?.elapsed().unwrap_or_default();
    (age < KEEP).then(|| std::fs::read(file).ok()).flatten().filter(|b| !b.is_empty())
}

/// An album's cover and its type: the kept one, else fetched and kept. `None`: it has none.
pub async fn cover(of: &Of) -> Result<Option<(Vec<u8>, &'static str)>> {
    let file = file_of(of).context("Cratarr's data folder is needed")?;
    let f = file.clone();
    if let Some(b) = tokio::task::spawn_blocking(move || kept(&f)).await? {
        return Ok(Some(sniffed(b)));
    }
    let _turn = FETCHING.acquire().await?;
    // Fetched by another meanwhile.
    let f = file.clone();
    if let Some(b) = tokio::task::spawn_blocking(move || kept(&f)).await? {
        return Ok(Some(sniffed(b)));
    }
    let bytes = match fetch(of).await {
        Ok(Some(b)) => b,
        Ok(None) => return Ok(None),
        // Not reachable now: the one kept before, however old.
        Err(e) => match tokio::fs::read(&file).await {
            Ok(b) if !b.is_empty() => return Ok(Some(sniffed(b))),
            _ => return Err(e),
        },
    };
    let b = bytes.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        std::fs::create_dir_all(file.parent().context("no folder")?)?;
        let part = file.with_extension("part");
        std::fs::write(&part, &b)?;
        std::fs::rename(&part, &file)?;
        Ok(())
    })
    .await??;
    Ok(Some(sniffed(bytes)))
}

fn sniffed(b: Vec<u8>) -> (Vec<u8>, &'static str) {
    let mime = crate::serve::art::sniff(&b);
    (b, mime)
}

/// The cover's bytes from where it is: a release group's from the Cover Art Archive (else as
/// `discogs::release_group_cover` finds it, which remembers none for a while), a Bandcamp album's from its page's.
async fn fetch(of: &Of) -> Result<Option<Vec<u8>>> {
    let http = super::discover::http()?;
    match of {
        Of::ReleaseGroup(rg) => {
            let caa = format!("https://coverartarchive.org/release-group/{rg}/front-250");
            if let Some(b) = image(&http, &caa).await? {
                return Ok(Some(b));
            }
            match super::discogs::release_group_cover(rg).await? {
                Some(url) if url != caa => image(&http, &url).await,
                _ => Ok(None),
            }
        }
        Of::Bandcamp(page) => match crate::bandcamp::Bandcamp::get().cover(page).await? {
            // Bandcamp's own pictures only (its image servers).
            Some(url) if reqwest::Url::parse(&url).ok().and_then(|u| u.host_str().map(|h| h.ends_with(".bcbits.com"))).unwrap_or(false) => {
                image(&http, &url).await
            }
            _ => Ok(None),
        },
    }
}

/// An image's bytes; `None` where there is none (404).
async fn image(http: &reqwest::Client, url: &str) -> Result<Option<Vec<u8>>> {
    let r = http.get(url).timeout(Duration::from_secs(30)).sent().await?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let r = r.error_for_status()?;
    if !r.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|t| t.starts_with("image/")) {
        bail!("{url} is not an image");
    }
    if r.content_length().is_some_and(|n| n as usize > MOST_BYTES) {
        bail!("{url} is too large");
    }
    let b = r.bytes().await?;
    if b.is_empty() || b.len() > MOST_BYTES {
        bail!("{url} could not be read");
    }
    Ok(Some(b.to_vec()))
}

/// A look's covers fetched ahead, one at a time, in the background (one such task at a time; the next look's does the
/// rest); the covers not asked for in a long while taken away first.
pub fn warm(covers: Vec<String>) {
    let wanted: Vec<Of> = covers.iter().filter_map(|c| Of::of_address(c)).collect();
    if wanted.is_empty() || WARMING.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        if let Some(dir) = crate::serve::instance().and_then(|s| s.data_dir.clone()).map(|d| d.join(FOLDER)) {
            let _ = tokio::task::spawn_blocking(move || forget_old(&dir)).await;
        }
        let mut fetched = 0;
        for of in &wanted {
            match cover(of).await {
                Ok(Some(_)) => fetched += 1,
                Ok(None) => {}
                Err(e) => tracing::debug!("covers: {of:?} not kept: {e:#}"),
            }
        }
        tracing::info!("covers: {fetched} of a look's {} kept", wanted.len());
        WARMING.store(false, Ordering::SeqCst);
    });
}

/// The kept covers not fetched again in FORGET taken away.
fn forget_old(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).is_some_and(|age| age > FORGET) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cover_address_names_its_album() {
        let rg = "b1392450-e666-3926-a536-22c65f834433";
        assert_eq!(Of::of_address(&format!("/api/covers/release-group/{rg}")), Some(Of::ReleaseGroup(rg.into())));
        let page = "https://artist.bandcamp.com/album/one";
        let id = crate::bandcamp::track_id(page, 0);
        assert_eq!(Of::of_address(&format!("/api/cover/bandcamp/{id}")), Some(Of::Bandcamp(page.into())));
        assert_eq!(Of::of_address("/api/covers/release-group/not-an-id"), None);
        assert_eq!(Of::of_address("https://coverartarchive.org/release/x/front-250"), None);
        assert_eq!(Of::of_address("/api/albums/cratarr/1/cover"), None);
    }
}
