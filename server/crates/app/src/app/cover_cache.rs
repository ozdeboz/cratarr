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
/// How long a record none of whose pictures gave one is not asked again, while its pictures are the same (M5).
const NONE_KEEP: Duration = Duration::from_secs(86_400);
/// The most of a cover's bytes read (the Spotify picture relay's too, Y11).
pub const MOST_BYTES: usize = 5 * 1024 * 1024;
/// Covers fetched at once (each one's bytes held meanwhile).
static FETCHING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(3);
/// A look's covers fetched ahead by one task at a time.
static WARMING: AtomicBool = AtomicBool::new(false);

/// Whose cover: in `catalogue::records`, beside the requests' cover check that reads it (SPLIT-PLAN P7).
pub use crate::catalogue::records::Of;

/// One of a record's pictures (F16.2), as its cover tries them in turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picture {
    /// A picture of its own on the covers' hosts, as its row keeps it.
    Outside(String),
    /// A release group's: the Cover Art Archive's, else its Discogs master's (`discogs::release_group_cover`).
    Group(String),
    /// A Discogs master's or release's primary image (`master`, `release`).
    Discogs(&'static str, u64),
    /// A Bandcamp album's picture, by its page.
    Bandcamp(String),
}

/// A record's cover address (F16: every list and page names a record's cover so): `/api/albums/cratarr/al-<n>/cover`.
pub fn record_address(record: i64) -> String {
    format!("/api/albums/{}/{}/cover", crate::source::cratarr::KIND.id, crate::serve::ids::record(record))
}

/// A record not had's pictures in the order its cover tries them (F16.2), each once: its own (Cratarr's own address as its group's
/// or its Bandcamp page's, or an outside picture; a Discogs thumb gives way first to the primary image of the Discogs release, else
/// master, the record holds: one Discogs question), its release group's, its own Discogs master's or release's, its Bandcamp
/// page's. `refs`: (source, kind, value). Pure.
pub fn pictures_of(own: Option<&str>, refs: &[(String, String, String)]) -> Vec<Picture> {
    let of = |source: &str, kind: &str| refs.iter().find(|(s, k, _)| s == source && k == kind).map(|(.., v)| v.clone());
    let discogs = |kind: &'static str| of("discogs", kind).and_then(|v| v.parse::<u64>().ok()).map(|id| Picture::Discogs(kind, id));
    let mut out: Vec<Picture> = Vec::new();
    match own.map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) if c.starts_with('/') => match Of::of_address(c) {
            Some(Of::ReleaseGroup(rg)) => out.push(Picture::Group(rg)),
            Some(Of::Bandcamp(page)) => out.push(Picture::Bandcamp(page)),
            None => {}
        },
        Some(c) => {
            let thumb = reqwest::Url::parse(c).ok().and_then(|u| u.host_str().map(|h| h == "discogs.com" || h.ends_with(".discogs.com"))).unwrap_or(false);
            if thumb && let Some(p) = discogs("release").or_else(|| discogs("master")) {
                out.push(p);
            }
            out.push(Picture::Outside(c.to_string()));
        }
        None => {}
    }
    out.extend(of("musicbrainz", "release-group").map(Picture::Group));
    out.extend(discogs("master"));
    out.extend(discogs("release"));
    out.extend(of("bandcamp", "album").map(Picture::Bandcamp));
    let mut seen = Vec::new();
    out.retain(|p| {
        let new = !seen.contains(p);
        seen.push(p.clone());
        new
    });
    out
}

/// The cover of Cratarr's record `album` where it is not had (C54, F16: `/api/albums/cratarr/al-<n>/cover`), merges followed: kept,
/// else each of its pictures tried in turn (`pictures_of`) until one gives one, kept a month by the record. None: no such record, one
/// had (its files hold its cover: the album route), or none of its pictures gives one; an error where the record could not be read
/// now (M4: the route asks again, never "no cover").
pub async fn of_record(cat: std::sync::Arc<crate::catalogue::Catalogue>, album: &str) -> Result<Option<(Vec<u8>, &'static str)>> {
    let keep = crate::serve::instance().and_then(|s| s.data_dir.clone()).map(|d| d.join(FOLDER));
    of_record_tried(cat, album, keep, fetch_picture).await
}

/// Tests: `of_record`, its pictures kept in `keep` (none: not kept) and fetched through `fetch`, a failure to read the record as none.
#[cfg(test)]
pub async fn of_record_with<F, Fut>(cat: std::sync::Arc<crate::catalogue::Catalogue>, album: &str, keep: Option<PathBuf>, fetch: F) -> Option<(Vec<u8>, &'static str)>
where
    F: Fn(Picture) -> Fut,
    Fut: std::future::Future<Output = Result<Option<Vec<u8>>>>,
{
    of_record_tried(cat, album, keep, fetch).await.unwrap_or_else(|e| {
        tracing::debug!("covers: {album} not read: {e:#}");
        None
    })
}

/// `of_record` through `fetch`. One record's fetch at a time (F16.5): a second ask waits for the first and serves what it kept. A
/// picture that answers nothing, or fails, gives way to the next (F16.4); where none gives one now, the one kept before, however old.
/// Where each answered none (no failure), that is kept a day as the record's "no picture" (M5), by its pictures: none is asked
/// again meanwhile, and a change of its pictures (its references, its own) lets go of it.
async fn of_record_tried<F, Fut>(cat: std::sync::Arc<crate::catalogue::Catalogue>, album: &str, keep: Option<PathBuf>, fetch: F) -> Result<Option<(Vec<u8>, &'static str)>>
where
    F: Fn(Picture) -> Fut,
    Fut: std::future::Future<Output = Result<Option<Vec<u8>>>>,
{
    let Some(record) = crate::serve::ids::record_number(album) else { return Ok(None) };
    let read = crate::run::spawn_blocking(move || -> Result<Option<(i64, Vec<Picture>)>> {
        let Some(row) = cat.record(record)?.filter(|r| !r.had) else { return Ok(None) };
        let refs: Vec<(String, String, String)> = cat.references_of(&crate::serve::ids::record(row.id))?.into_iter().map(|(r, _)| (r.source, r.kind, r.value)).collect();
        Ok(Some((row.id, pictures_of(row.cover.as_deref(), &refs))))
    });
    let Some((id, pictures)) = read.await?? else { return Ok(None) };
    let file = keep.as_ref().map(|k| k.join(named(&format!("rec:{id}"))));
    let none = keep.map(|k| k.join(named(&format!("rec:{id}.none"))));
    let print = format!("{pictures:?}");
    let fresh = |f: Option<PathBuf>| async move {
        let f = f?;
        crate::run::spawn_blocking(move || kept(&f)).await.ok().flatten()
    };
    if let Some(b) = fresh(file.clone()).await {
        return Ok(Some(sniffed(b)));
    }
    let one = record_turn(id);
    let _one = one.lock().await;
    // Fetched by another meanwhile.
    if let Some(b) = fresh(file.clone()).await {
        return Ok(Some(sniffed(b)));
    }
    let old = |f: Option<PathBuf>| async move { tokio::fs::read(f?).await.ok().filter(|b| !b.is_empty()).map(sniffed) };
    // M5: none of these pictures gave one within the day: none asked again.
    let (n, p) = (none.clone(), print.clone());
    if crate::run::spawn_blocking(move || n.is_some_and(|n| none_kept(&n, &p))).await? {
        return Ok(old(file).await);
    }
    let mut failed = false;
    for p in pictures {
        let _turn = FETCHING.acquire().await?;
        match fetch(p.clone()).await {
            Ok(Some(b)) if !b.is_empty() => {
                if let Some(f) = file.clone() {
                    let kept = b.clone();
                    let wrote = crate::run::spawn_blocking(move || -> Result<()> {
                        std::fs::create_dir_all(f.parent().context("no folder")?)?;
                        let part = f.with_extension("part");
                        std::fs::write(&part, &kept)?;
                        std::fs::rename(&part, &f)?;
                        Ok(())
                    })
                    .await;
                    if !matches!(wrote, Ok(Ok(()))) {
                        tracing::debug!("covers: {album}'s picture not kept");
                    }
                }
                return Ok(Some(sniffed(b)));
            }
            Ok(_) => {}
            Err(e) => {
                failed = true;
                tracing::debug!("covers: {p:?} of {album}: {e:#}");
            }
        }
    }
    // Each answered none: kept so a day (M5). One failed: nothing kept, asked again next time (F16.4).
    if !failed && let Some(n) = none {
        let wrote = crate::run::spawn_blocking(move || -> Result<()> {
            std::fs::create_dir_all(n.parent().context("no folder")?)?;
            Ok(std::fs::write(&n, print)?)
        })
        .await;
        if !matches!(wrote, Ok(Ok(()))) {
            tracing::debug!("covers: {album}'s \"no picture\" not kept");
        }
    }
    // None gives one now: the one kept before, however old.
    Ok(old(file).await)
}

/// Whether a record's "no picture" (M5) is kept for these pictures (`print`), younger than a day.
fn none_kept(file: &std::path::Path, print: &str) -> bool {
    let young = std::fs::metadata(file).ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).is_some_and(|age| age < NONE_KEEP);
    young && std::fs::read_to_string(file).is_ok_and(|kept| kept == print)
}

/// One record's cover fetched at a time (F16.5): its lock, the records not asked for let go of.
fn record_turn(record: i64) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    type Turns = std::sync::Mutex<std::collections::HashMap<i64, std::sync::Arc<tokio::sync::Mutex<()>>>>;
    static TURNS: std::sync::OnceLock<Turns> = std::sync::OnceLock::new();
    let mut turns = TURNS.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner());
    turns.retain(|_, t| std::sync::Arc::strong_count(t) > 1);
    turns.entry(record).or_default().clone()
}

/// A picture fetched for a record's cover (`of_record`): through the covers' hosts only (O1), Bandcamp's from its image servers.
async fn fetch_picture(p: Picture) -> Result<Option<Vec<u8>>> {
    let http = super::reach::client(super::reach::Purpose::Covers).timeout(Duration::from_secs(30)).build()?;
    match p {
        Picture::Outside(url) => image(&http, &url, MOST_BYTES).await,
        Picture::Group(rg) => fetch(&Of::ReleaseGroup(rg)).await,
        Picture::Bandcamp(page) => fetch(&Of::Bandcamp(page)).await,
        Picture::Discogs(kind, id) => {
            let images = if kind == "master" { super::discogs::master(id).await?.map(|m| m.images).unwrap_or_default() } else { super::discogs::release_images(id).await? };
            match images.into_iter().next() {
                Some(i) => image(&http, &i.url, MOST_BYTES).await,
                None => Ok(None),
            }
        }
    }
}

/// A kept cover's file name in the cover cache, by its key.
fn named(key: &str) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(key.as_bytes()).iter().take(16).map(|b| format!("{b:02x}")).collect()
}

fn file_of(of: &Of) -> Option<PathBuf> {
    let data = crate::serve::instance()?.data_dir.clone()?;
    Some(data.join(FOLDER).join(named(&of.key())))
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
    if let Some(b) = crate::run::spawn_blocking(move || kept(&f)).await? {
        return Ok(Some(sniffed(b)));
    }
    let _turn = FETCHING.acquire().await?;
    // Fetched by another meanwhile.
    let f = file.clone();
    if let Some(b) = crate::run::spawn_blocking(move || kept(&f)).await? {
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
    crate::run::spawn_blocking(move || -> Result<()> {
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
    // The covers' hosts only, a redirect followed while it stays on them (O1).
    let http = super::reach::client(super::reach::Purpose::Covers).timeout(Duration::from_secs(30)).build()?;
    match of {
        Of::ReleaseGroup(rg) => {
            // Kept as having none anywhere (O6): the Cover Art Archive is not asked again on every view.
            if super::discogs::kept_without_cover(rg).await {
                return Ok(None);
            }
            let caa = format!("https://coverartarchive.org/release-group/{rg}/front-250");
            if let Some(b) = image(&http, &caa, MOST_BYTES).await? {
                return Ok(Some(b));
            }
            match super::discogs::release_group_cover(rg).await? {
                Some(url) if url != caa => image(&http, &url, MOST_BYTES).await,
                _ => Ok(None),
            }
        }
        Of::Bandcamp(page) => match crate::bandcamp::Bandcamp::get().cover(page).await? {
            // Bandcamp's own pictures only (its image servers).
            Some(url) if reqwest::Url::parse(&url).ok().and_then(|u| u.host_str().map(|h| h.ends_with(".bcbits.com"))).unwrap_or(false) => {
                image(&http, &url, MOST_BYTES).await
            }
            _ => Ok(None),
        },
    }
}

/// An image's bytes, `most` of them at most (`MOST_BYTES` for the covers kept; Soulseek's placed cover reads larger ones, Y3);
/// `None` where there is none (404), or where the address is none of the covers' hosts (O1: Discogs names it).
pub(super) async fn image(http: &reqwest::Client, url: &str, most: usize) -> Result<Option<Vec<u8>>> {
    if !reqwest::Url::parse(url).is_ok_and(|u| super::reach::Purpose::Covers.allows(&u)) {
        return Ok(None);
    }
    let r = http.get(url).timeout(Duration::from_secs(30)).sent().await?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let r = r.error_for_status()?;
    if !r.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|t| t.starts_with("image/")) {
        bail!("{url} is not an image");
    }
    // Read in pieces, cut at its cap (O1).
    let b = super::reach::read_capped(r, most).await?;
    if b.is_empty() {
        bail!("{url} could not be read");
    }
    Ok(Some(b))
}

/// A look's covers fetched ahead, one at a time, in the background (one such task at a time; the next look's does the
/// rest); the covers not asked for in a long while taken away first.
pub fn warm(covers: Vec<String>) {
    let wanted: Vec<Of> = covers.iter().filter_map(|c| Of::of_address(c)).collect();
    if wanted.is_empty() || WARMING.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::run::spawn(async move {
        if let Some(dir) = crate::serve::instance().and_then(|s| s.data_dir.clone()).map(|d| d.join(FOLDER)) {
            let _ = crate::run::spawn_blocking(move || forget_old(&dir)).await;
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

    use crate::catalogue::Catalogue;
    use crate::catalogue::records::Ref;
    use crate::serve::ids;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nnight walks";
    const GROUP: &str = "00000000-0000-4000-8000-000000000f16";

    /// A record not had: `cover` its row's, holding `refs`.
    fn record_with(cat: &Catalogue, cover: Option<&str>, refs: &[(&str, &str, &str)]) -> i64 {
        let record = cat.record_for_tests("Night Walks", 0);
        if let Some(c) = cover {
            cat.db_for_tests().execute("UPDATE records SET cover = ?2 WHERE id = ?1", rusqlite::params![record, c]).unwrap();
        }
        for (s, k, v) in refs {
            cat.set_reference(&ids::record(record), &Ref::new(s, k, v), "link").unwrap();
        }
        record
    }

    type Asks = Arc<std::sync::Mutex<Vec<Picture>>>;

    /// A fetch answering each picture as `answer` says, its asks kept in order.
    fn answering(asked: &Asks, answer: fn(&Picture) -> Result<Option<Vec<u8>>>) -> impl Fn(Picture) -> std::future::Ready<Result<Option<Vec<u8>>>> {
        let asked = asked.clone();
        move |p| {
            asked.lock().unwrap().push(p.clone());
            std::future::ready(answer(&p))
        }
    }

    fn keep_folder(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-covers-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    /// F16.2: a record's cover tries each of its pictures in turn until one gives one: "Break Synonyms", its group answering none,
    /// gives its Bandcamp page's picture, asked after the group and its Discogs master; "Wabi Sabi", an i.discogs.com thumb and a
    /// Discogs release, gives the release's primary image, one Discogs question, before the thumb. The order in full: its own, its
    /// group's, its Discogs master's and release's, its Bandcamp page's, each once. Never one picture tried alone; never none where a
    /// later picture gives one.
    #[tokio::test]
    async fn a_records_cover_tries_each_picture_in_turn() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let page = "https://kitetheory.bandcamp.com/album/break-synonyms";
        let breaks = record_with(&cat, None, &[("musicbrainz", "release-group", GROUP), ("discogs", "master", "5000030"), ("bandcamp", "album", page)]);
        let asked: Asks = Arc::default();
        let got = of_record_with(cat.clone(), &ids::record(breaks), None, answering(&asked, |p| Ok(matches!(p, Picture::Bandcamp(_)).then(|| PNG.to_vec())))).await;
        assert_eq!(got.map(|(b, _)| b), Some(PNG.to_vec()), "Bandcamp's picture");
        assert_eq!(*asked.lock().unwrap(), [Picture::Group(GROUP.into()), Picture::Discogs("master", 5000030), Picture::Bandcamp(page.into())]);
        let thumb = "https://i.discogs.com/abc/wabi-sabi-150.jpg";
        let wabi = record_with(&cat, Some(thumb), &[("discogs", "release", "3600050")]);
        let asked: Asks = Arc::default();
        let got = of_record_with(cat.clone(), &ids::record(wabi), None, answering(&asked, |p| Ok(matches!(p, Picture::Discogs("release", 3600050)).then(|| PNG.to_vec())))).await;
        assert!(got.is_some(), "the release's primary image");
        assert_eq!(*asked.lock().unwrap(), [Picture::Discogs("release", 3600050)], "one Discogs question, before the thumb");
        let refs = |r: &[(&str, &str, &str)]| r.iter().map(|(s, k, v)| (s.to_string(), k.to_string(), v.to_string())).collect::<Vec<_>>();
        let all = refs(&[("bandcamp", "album", page), ("discogs", "release", "3600051"), ("discogs", "master", "5000031"), ("musicbrainz", "release-group", GROUP)]);
        let own = format!("/api/covers/release-group/{GROUP}");
        assert_eq!(pictures_of(Some(&own), &all), [Picture::Group(GROUP.into()), Picture::Discogs("master", 5000031), Picture::Discogs("release", 3600051), Picture::Bandcamp(page.into())], "each once");
        assert_eq!(
            pictures_of(Some(thumb), &all),
            [Picture::Discogs("release", 3600051), Picture::Outside(thumb.into()), Picture::Group(GROUP.into()), Picture::Discogs("master", 5000031), Picture::Bandcamp(page.into())]
        );
        let outside = "https://coverartarchive.org/release/x/front-250";
        assert_eq!(pictures_of(Some(outside), &refs(&[("bandcamp", "album", page)])), [Picture::Outside(outside.into()), Picture::Bandcamp(page.into())]);
    }

    /// F16.4: a picture answering none, or failing (out of reach), gives way to the next; none kept as "no cover" after a failure (the
    /// next ask fetches again); a kept picture older than a month fetched again, the old one served while that fails; a record merged
    /// since gives the record it is now's pictures; a record had since gives none here (its files' cover is the album route's).
    #[tokio::test]
    async fn a_cover_that_fails_gives_way_to_the_next() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let keep = keep_folder("fails");
        let page = "https://kitetheory.bandcamp.com/album/answered";
        let r = record_with(&cat, None, &[("musicbrainz", "release-group", GROUP), ("bandcamp", "album", page)]);
        let asked: Asks = Arc::default();
        let failing = |p: &Picture| if matches!(p, Picture::Group(_)) { Err(anyhow::anyhow!("out of reach")) } else { Ok(Some(PNG.to_vec())) };
        assert!(of_record_with(cat.clone(), &ids::record(r), Some(keep.clone()), answering(&asked, failing)).await.is_some());
        assert_eq!(*asked.lock().unwrap(), [Picture::Group(GROUP.into()), Picture::Bandcamp(page.into())], "the next tried");
        // Nothing gives one, a failure among them: nothing kept, and the next ask asks again.
        let other = record_with(&cat, None, &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000f17")]);
        let asked: Asks = Arc::default();
        assert!(of_record_with(cat.clone(), &ids::record(other), Some(keep.clone()), answering(&asked, |_| Err(anyhow::anyhow!("timed out")))).await.is_none());
        assert!(!keep.join(named(&format!("rec:{other}"))).exists(), "no \"no cover\" kept");
        assert!(of_record_with(cat.clone(), &ids::record(other), Some(keep.clone()), answering(&asked, |_| Ok(Some(PNG.to_vec())))).await.is_some(), "asked again");
        assert_eq!(asked.lock().unwrap().len(), 2);
        // Kept over a month: fetched again; failing, the old one served.
        let file = keep.join(named(&format!("rec:{r}")));
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(std::time::SystemTime::now() - Duration::from_secs(31 * 86_400)).unwrap();
        let asked: Asks = Arc::default();
        let old = of_record_with(cat.clone(), &ids::record(r), Some(keep.clone()), answering(&asked, |_| Err(anyhow::anyhow!("away")))).await;
        assert_eq!((old.map(|(b, _)| b), asked.lock().unwrap().len()), (Some(PNG.to_vec()), 2), "asked again, the old one served");
        // Merged since: the record it is now.
        let gone = record_with(&cat, None, &[("bandcamp", "album", "https://kitetheory.bandcamp.com/album/merged")]);
        let now = record_with(&cat, None, &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000f18")]);
        cat.merge_records(gone, now).unwrap();
        let asked: Asks = Arc::default();
        of_record_with(cat.clone(), &ids::record(gone), None, answering(&asked, |_| Ok(None))).await;
        assert!(asked.lock().unwrap().contains(&Picture::Group("00000000-0000-4000-8000-000000000f18".into())), "the record it is now: {:?}", asked.lock().unwrap());
        // Had since: none here, nothing asked.
        let had = cat.record_for_tests("Had", 2);
        cat.set_reference(&ids::record(had), &Ref::new("musicbrainz", "release-group", GROUP), "link").unwrap();
        let asked: Asks = Arc::default();
        assert!(of_record_with(cat.clone(), &ids::record(had), None, answering(&asked, |_| Ok(Some(PNG.to_vec())))).await.is_none());
        assert!(asked.lock().unwrap().is_empty());
        std::fs::remove_dir_all(&keep).ok();
    }

    /// M5 (the review of 1.2): a record not had none of whose pictures gives one (each answers none) is not asked again within the
    /// day: no Discogs question on each view. A day on, asked again; its pictures changed (a reference added), asked again at once. One
    /// picture failing keeps nothing (F16.4's test).
    #[tokio::test]
    async fn a_record_with_no_picture_is_not_asked_again_for_a_day() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let keep = keep_folder("none");
        let r = record_with(&cat, None, &[("discogs", "master", "5000040"), ("discogs", "release", "3600040")]);
        let asked: Asks = Arc::default();
        let album = ids::record(r);
        assert!(of_record_with(cat.clone(), &album, Some(keep.clone()), answering(&asked, |_| Ok(None))).await.is_none());
        assert_eq!(asked.lock().unwrap().len(), 2, "each picture asked");
        assert!(of_record_with(cat.clone(), &album, Some(keep.clone()), answering(&asked, |_| Ok(None))).await.is_none());
        assert_eq!(asked.lock().unwrap().len(), 2, "none asked again within the day");
        let mark = keep.join(named(&format!("rec:{r}.none")));
        std::fs::File::options().write(true).open(&mark).unwrap().set_modified(std::time::SystemTime::now() - Duration::from_secs(86_400 + 60)).unwrap();
        assert!(of_record_with(cat.clone(), &album, Some(keep.clone()), answering(&asked, |_| Ok(None))).await.is_none());
        assert_eq!(asked.lock().unwrap().len(), 4, "a day on, asked again");
        cat.set_reference(&ids::record(r), &Ref::new("bandcamp", "album", "https://kitetheory.bandcamp.com/album/none-yet"), "link").unwrap();
        assert!(of_record_with(cat.clone(), &album, Some(keep.clone()), answering(&asked, |_| Ok(None))).await.is_none());
        assert_eq!(asked.lock().unwrap().len(), 7, "its pictures changed: asked again at once");
        std::fs::remove_dir_all(&keep).ok();
    }

    /// F16.5: two lists asking one record's cover at once: one fetch, the second served what the first kept. The guards (pins): a
    /// picture off the covers' hosts, over http or at a private address is never asked; Bandcamp's pictures come from its image
    /// servers alone; a row carries Cratarr's own address alone.
    #[tokio::test]
    async fn one_cover_fetched_once_through_the_covers_hosts() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let keep = keep_folder("once");
        let r = record_with(&cat, None, &[("musicbrainz", "release-group", GROUP)]);
        let count = Arc::new(AtomicUsize::new(0));
        let slow = {
            let count = count.clone();
            move |_: Picture| {
                let count = count.clone();
                async move {
                    count.fetch_add(1, SeqCst);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Ok::<_, anyhow::Error>(Some(PNG.to_vec()))
                }
            }
        };
        let album = ids::record(r);
        let (a, b) = tokio::join!(of_record_with(cat.clone(), &album, Some(keep.clone()), slow.clone()), of_record_with(cat.clone(), &album, Some(keep.clone()), slow.clone()));
        assert!(a.is_some() && b.is_some());
        assert_eq!(count.load(SeqCst), 1, "one fetch");
        let http = reqwest::Client::new();
        for left in ["https://example.org/a.jpg", "http://coverartarchive.org/release-group/x/front-250", "https://192.168.1.10/a.jpg"] {
            assert_eq!(image(&http, left, MOST_BYTES).await.unwrap(), None, "{left}");
        }
        assert!(include_str!("cover_cache.rs").contains("h.ends_with(\".bcbits.com\")"), "Bandcamp's image servers alone");
        assert_eq!(record_address(r), format!("/api/albums/cratarr/{album}/cover"));
        std::fs::remove_dir_all(&keep).ok();
    }
}
