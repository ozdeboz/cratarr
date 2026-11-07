//! Small covers for every record (0101's point 16, D13): what a device shows for a record it has no files of, as one bundle, then
//! as they change. A cover is made once, into the data folder, as a decode takes tens of MB and a second or so on the NAS, and a
//! library has thousands: no request could wait for them, and nothing of them stays in memory. So:
//!
//! - **A maker works through what is missing** (`make`), a cover at a time, in the background, once a device has asked: a server
//!   with no device pays nothing. It waits for room as a decode does (`memwatch`), takes its turn beside `Serve::cover`'s, and
//!   logs each cover it makes.
//! - **The bundle is what is made so far.** It comes file by file from the disk, so a device never waits for the maker: what was not
//!   made yet comes by the feed, as a row of kind `cover`, as a cover that changed does.
//! - **A cover is no row of the index.** It moves the log's rev and not `/api/changes`' `changed` (`Catalogue::index_moved`): the
//!   first covers of a library take hours, and each would cost a whole build. This feed builds nothing, so a device asks it at
//!   every sync: that is also what sets the maker to a cover changed with no other word in the log.
//! - **The form is plain,** as a page and an app read it with no rules crate: `CRXV`, a version and the rev as fixed numbers, then
//!   for each record its id, its picture's length and the picture, all little-endian. Length 0: the record has no cover now.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Result, anyhow};
use axum::Json;
use axum::body::Body;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::ToggleError;
use crate::serve::{Serve, ids};

/// The longest side, as the shelf keeps its covers (the page's `COVER_PX`): a row's cover.
pub const SIDE: u32 = 160;
/// JPEG's quality: a cover this small shows no more above it, and Q4 of PREVIEW2 asks what the whole library weighs.
pub const QUALITY: u8 = 75;
/// The data folder's folder for them: `<record>-<cover stamp>.jpg`. Nothing else names them, so a stale one is a file to remove.
const FOLDER: &str = "device-covers";
/// What a bundle begins with, and its form's version: raised by anything a reader must know.
const MAGIC: &[u8; 4] = b"CRXV";
const VERSION: u32 = 1;
/// A breath between covers: it is the music that must not stutter on the NAS's few cores.
const BETWEEN: Duration = Duration::from_millis(50);
/// One maker at a time.
static MAKING: AtomicBool = AtomicBool::new(false);

fn folder_of(serve: &Serve) -> Option<PathBuf> {
    serve.data_dir.as_ref().map(|d| d.join(FOLDER))
}

/// Every record's cover stamp, by its number.
fn stamps_of(serve: &Serve) -> HashMap<i64, String> {
    serve.cover_stamps().iter().filter_map(|(album, stamp)| Some((album.strip_prefix(ids::ALBUM)?.parse().ok()?, stamp.clone()))).collect()
}

/// What the folder holds, by record: its stamp and its file. A record may hold a file with no picture in it: its cover could not be
/// read, which is not tried again until it changes.
fn held_in(dir: &Path) -> HashMap<i64, (String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return HashMap::new() };
    let parse = |p: PathBuf| {
        let name = p.file_stem()?.to_str()?;
        let (record, stamp) = name.split_once('-')?;
        Some((record.parse::<i64>().ok()?, (stamp.to_string(), p.clone())))
    };
    entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "jpg")).filter_map(|e| parse(e.path())).collect()
}

/// The sizes a list's cover is made at: none smaller than a row needs, none larger than a picture worth keeping small.
pub const SIZES: std::ops::RangeInclusive<u32> = 32..=2048;

/// A record's cover for a list or a grid (finding 86; the player and a panel keep the whole one): its small cover here where that
/// is as large as asked, made already, else made at that size once, through the disk cache (`Serve::sized_cover`). None where it
/// has no cover.
pub async fn for_list(serve: &Arc<Serve>, album: &str, size: u32) -> Option<(String, Vec<u8>)> {
    let size = size.clamp(*SIZES.start(), *SIZES.end());
    if size <= SIDE
        && let Some(small) = small_made(serve, album)
    {
        return Some(("image/jpeg".to_string(), small));
    }
    serve.sized_cover(album, size).await.unwrap_or_else(|e| {
        tracing::warn!(target: "library", album = %album, size, reason = %format!("{e:#}"), "no cover at that size");
        None
    })
}

/// A record's small cover as made already: none where it is not made yet, or its picture could not be read. Nothing here makes
/// one: what asks for a cover cannot wait for the maker.
fn small_made(serve: &Serve, album: &str) -> Option<Vec<u8>> {
    let stamp = serve.cover_stamps().get(album).cloned()?;
    let record = album.strip_prefix(ids::ALBUM)?;
    let file = folder_of(serve)?.join(format!("{record}-{stamp}.jpg"));
    std::fs::read(file).ok().filter(|picture| !picture.is_empty())
}

/// What goes out, read before a byte does: the rev a device goes on from, and each record's picture, none where it has none now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bundle {
    rev: i64,
    frames: Vec<(i64, Option<PathBuf>)>,
}

impl IntoResponse for Bundle {
    fn into_response(self) -> Response {
        let (tx, rx) = tokio::sync::mpsc::channel::<std::io::Result<Vec<u8>>>(8);
        tokio::spawn(async move {
            let mut head = MAGIC.to_vec();
            head.extend(VERSION.to_le_bytes());
            head.extend(self.rev.to_le_bytes());
            if tx.send(Ok(head)).await.is_err() {
                return;
            }
            for (record, file) in self.frames {
                let picture = match file {
                    // Made over since it was listed: the feed brings the new one.
                    Some(file) => match tokio::fs::read(&file).await {
                        Ok(picture) => picture,
                        Err(_) => continue,
                    },
                    None => Vec::new(),
                };
                let mut frame = Vec::with_capacity(picture.len() + 12);
                frame.extend(record.to_le_bytes());
                frame.extend((picture.len() as u32).to_le_bytes());
                frame.extend(picture);
                if tx.send(Ok(frame)).await.is_err() {
                    return;
                }
            }
        });
        ([(header::CONTENT_TYPE, "application/octet-stream"), (header::CACHE_CONTROL, "no-store")], Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).into_response()
    }
}

/// What a device at a rev is told of the covers, as the index's feed tells it of its rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feed {
    Bundle(Bundle),
    Same,
    Whole { rev: i64 },
}

impl IntoResponse for Feed {
    fn into_response(self) -> Response {
        match self {
            Feed::Bundle(b) => b.into_response(),
            Feed::Same => StatusCode::NO_CONTENT.into_response(),
            Feed::Whole { rev } => (StatusCode::GONE, Json(serde_json::json!({ "rev": rev, "reset": true }))).into_response(),
        }
    }
}

/// Every cover made so far. The log read first: what is made while the folder is read is logged past it, and comes again.
fn whole(serve: &Serve) -> Result<Bundle> {
    let rev = serve.catalogue().index_rev()?;
    let stamps = stamps_of(serve);
    let held = folder_of(serve).map(|d| held_in(&d)).unwrap_or_default();
    let mut frames: Vec<(i64, Option<PathBuf>)> = held
        .into_iter()
        .filter(|(record, (stamp, file))| stamps.get(record) == Some(stamp) && std::fs::metadata(file).is_ok_and(|m| m.len() > 0))
        .map(|(record, (_, file))| (record, Some(file)))
        .collect();
    frames.sort_unstable_by_key(|f| f.0);
    Ok(Bundle { rev, frames })
}

/// What changed of the covers after `since`.
fn feed(serve: &Serve, since: i64) -> Result<Feed> {
    let cat = serve.catalogue();
    let rev = cat.index_rev()?;
    if since < cat.index_floor()? || since > rev {
        return Ok(Feed::Whole { rev });
    }
    if since == rev {
        return Ok(Feed::Same);
    }
    let logged = cat.index_logged(since, rev)?;
    let stamps = stamps_of(serve);
    let held = folder_of(serve).map(|d| held_in(&d)).unwrap_or_default();
    let frames = logged.get("cover").map_or(&[][..], Vec::as_slice).iter().map(|record| {
        let file = held.get(record).filter(|(stamp, file)| stamps.get(record) == Some(stamp) && std::fs::metadata(file).is_ok_and(|m| m.len() > 0));
        (*record, file.map(|(_, file)| file.clone()))
    });
    Ok(Feed::Bundle(Bundle { rev, frames: frames.collect() }))
}

/// A file in place of what stood there for `record`, whole or not at all.
fn keep(dir: &Path, record: i64, stamp: &str, picture: &[u8], was: Option<&PathBuf>) -> Result<()> {
    let file = dir.join(format!("{record}-{stamp}.jpg"));
    let part = file.with_extension("part");
    std::fs::write(&part, picture)?;
    std::fs::rename(&part, &file)?;
    if let Some(was) = was.filter(|was| **was != file) {
        std::fs::remove_file(was).ok();
    }
    Ok(())
}

/// The folder brought up to the library's covers, a cover at a time, each logged as it is made or goes. How many were made.
pub async fn make(serve: &Arc<Serve>) -> Result<usize> {
    let Some(dir) = folder_of(serve) else { return Ok(0) };
    let (me, at) = (serve.clone(), dir.clone());
    let (stamps, held) = tokio::task::spawn_blocking(move || -> Result<_> {
        std::fs::create_dir_all(&at)?;
        Ok((stamps_of(&me), held_in(&at)))
    })
    .await??;
    let cat = serve.catalogue().clone();
    let mut todo: Vec<(&i64, &String)> = stamps.iter().filter(|(record, stamp)| held.get(*record).is_none_or(|(kept, _)| kept != *stamp)).collect();
    todo.sort_unstable();
    let mut made = 0;
    for (record, stamp) in todo {
        crate::memwatch::wait_for_room().await;
        let album = ids::record(*record);
        let small = match serve.small_cover(&album, SIDE, QUALITY).await {
            Ok(small) => small,
            Err(e) => {
                tracing::warn!(target: "library", album = %album, reason = %format!("{e:#}"), "no small cover made");
                None
            }
        };
        keep(&dir, *record, stamp, small.as_deref().unwrap_or_default(), held.get(record).map(|(_, file)| file))?;
        cat.index_note("cover", *record)?;
        made += 1;
        tokio::time::sleep(BETWEEN).await;
    }
    for (record, (_, file)) in held.iter().filter(|(record, _)| !stamps.contains_key(*record)) {
        std::fs::remove_file(file).ok();
        cat.index_note("cover", *record)?;
    }
    Ok(made)
}

/// The maker started, where it is not at work: it finds nothing to do quickly, so every asking can do this.
fn warm(serve: &Arc<Serve>) {
    if serve.data_dir.is_none() || MAKING.swap(true, Ordering::SeqCst) {
        return;
    }
    let serve = serve.clone();
    tokio::spawn(async move {
        match make(&serve).await {
            Ok(0) => {}
            Ok(made) => tracing::info!(target: "library", made, "small covers made"),
            Err(e) => tracing::warn!(target: "library", reason = %format!("{e:#}"), "small covers not made"),
        }
        MAKING.store(false, Ordering::SeqCst);
    });
}

/// `GET /api/index/covers`: every small cover made so far, and the rev to go on from.
pub async fn read(serve: Arc<Serve>) -> Result<Bundle, ToggleError> {
    warm(&serve);
    Ok(tokio::task::spawn_blocking(move || whole(&serve)).await.map_err(|e| anyhow!("{e}"))??)
}

/// `GET /api/index/covers/changes?since=<rev>`: what changed of them, what was made since among it.
pub async fn changes(serve: Arc<Serve>, since: i64) -> Result<Feed, ToggleError> {
    warm(&serve);
    Ok(tokio::task::spawn_blocking(move || feed(&serve, since)).await.map_err(|e| anyhow!("{e}"))??)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use image::{Rgb, RgbImage};

    use super::*;
    use crate::app::device_index::tests::{Served, served_in};
    use crate::serve::art;
    use crate::serve::copies::Kind as CopyKind;

    /// A server with a data folder, an album with `picture` as its cover beside it.
    fn with_cover(name: &str, picture: &[u8]) -> (Served, PathBuf) {
        let data = std::env::temp_dir().join(format!("cratarr-device-covers-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&data).ok();
        std::fs::create_dir_all(&data).unwrap();
        let s = served_in(name, &[CopyKind::Main], Some(&data));
        std::fs::create_dir_all(s.roots[0].join("A")).unwrap();
        std::fs::write(s.roots[0].join("A/cover.jpg"), picture).unwrap();
        s.add(0, &[("A/01.flac", &[("album", "Record"), ("albumartist", "Someone"), ("title", "One")])]);
        s.built();
        (s, data)
    }

    fn picture(colour: [u8; 3], side: u32) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        RgbImage::from_pixel(side, side, Rgb(colour)).write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    }

    /// A bundle as it reaches a device: the rev it says, and each record's picture, none where it has none.
    async fn read_back(sent: Response) -> (i64, BTreeMap<i64, Vec<u8>>) {
        let bytes = axum::body::to_bytes(sent.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&bytes[..4], MAGIC);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), VERSION);
        let rev = i64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let (mut at, mut frames) = (16, BTreeMap::new());
        while at < bytes.len() {
            let record = i64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
            let length = u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
            frames.insert(record, bytes[at + 12..at + 12 + length].to_vec());
            at += 12 + length;
        }
        (rev, frames)
    }

    /// The bundle and the feed as `read` and `changes` answer them, the maker's turn left out: a test calls it.
    async fn whole_now(s: &Served) -> (i64, BTreeMap<i64, Vec<u8>>) {
        read_back(whole(&s.serve).unwrap().into_response()).await
    }

    #[tokio::test]
    async fn every_cover_comes_small_in_one_bundle_and_nothing_stands_in_for_what_is_not_made() {
        let (s, data) = with_cover("bundle", &picture([200, 30, 30], 1000));
        assert_eq!(whole_now(&s).await.1, BTreeMap::new(), "nothing made yet, nothing held up");
        assert_eq!(make(&s.serve).await.unwrap(), 1);
        let (rev, covers) = whole_now(&s).await;
        assert_eq!(rev, s.cat.index_rev().unwrap());
        let small = image::load_from_memory(covers.values().next().unwrap()).unwrap();
        assert_eq!((small.width(), small.height()), (SIDE, SIDE));
        assert_eq!(make(&s.serve).await.unwrap(), 0, "made once");
        std::fs::remove_dir_all(&data).ok();
    }

    /// The owner's D13: whole, then changes, comes to what a fresh bundle holds.
    #[tokio::test]
    async fn covers_changed_made_and_gone_come_by_the_feed_as_a_fresh_bundle_says_them() {
        let (s, data) = with_cover("feed", &picture([200, 30, 30], 400));
        // What was not made yet when the device fetched comes by the feed.
        let (mut rev, mut device) = whole_now(&s).await;
        assert!(device.is_empty());
        make(&s.serve).await.unwrap();
        let apply = |device: &mut BTreeMap<i64, Vec<u8>>, frames: BTreeMap<i64, Vec<u8>>| {
            for (record, picture) in frames {
                if picture.is_empty() {
                    device.remove(&record);
                } else {
                    device.insert(record, picture);
                }
            }
        };
        let Feed::Bundle(sent) = feed(&s.serve, rev).unwrap() else { panic!("the new cover") };
        let (to, frames) = read_back(sent.into_response()).await;
        apply(&mut device, frames);
        rev = to;
        assert_eq!(device, whole_now(&s).await.1);
        assert_eq!(device.len(), 1);
        assert_eq!(feed(&s.serve, rev).unwrap(), Feed::Same);

        // Changed: another picture under another stamp.
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(s.roots[0].join("A/cover.jpg"), picture([30, 200, 30], 400)).unwrap();
        s.cat.walk_all();
        s.built();
        s.serve.covers_read_again_for_tests();
        assert_eq!(whole_now(&s).await.1, BTreeMap::new(), "the old picture no longer passes for the record's");
        make(&s.serve).await.unwrap();
        let Feed::Bundle(sent) = feed(&s.serve, rev).unwrap() else { panic!("the changed cover") };
        let (to, frames) = read_back(sent.into_response()).await;
        apply(&mut device, frames);
        rev = to;
        assert_eq!(device, whole_now(&s).await.1);
        let green = image::load_from_memory(device.values().next().unwrap()).unwrap().to_rgb8();
        assert!(green.get_pixel(5, 5)[1] > 150, "{:?}", green.get_pixel(5, 5));
        assert_eq!(std::fs::read_dir(data.join(FOLDER)).unwrap().count(), 1, "the old one went");

        // Gone: a length of nothing says so.
        std::fs::remove_file(s.roots[0].join("A/cover.jpg")).unwrap();
        s.cat.walk_all();
        s.built();
        s.serve.covers_read_again_for_tests();
        make(&s.serve).await.unwrap();
        let Feed::Bundle(sent) = feed(&s.serve, rev).unwrap() else { panic!("the cover gone") };
        let (_, frames) = read_back(sent.into_response()).await;
        apply(&mut device, frames);
        assert_eq!(device, BTreeMap::new());
        std::fs::remove_dir_all(&data).ok();
    }

    #[tokio::test]
    async fn a_rev_not_kept_starts_again() {
        let (s, data) = with_cover("again", &picture([200, 30, 30], 200));
        let rev = s.cat.index_rev().unwrap();
        assert_eq!(feed(&s.serve, rev + 1).unwrap(), Feed::Whole { rev });
        assert_eq!(feed(&s.serve, rev).unwrap(), Feed::Same);
        std::fs::remove_dir_all(&data).ok();
    }

    /// A cover that cannot be read costs one decode, and no record's row says otherwise than that it has none.
    #[tokio::test]
    async fn a_cover_that_cannot_be_read_is_tried_once_and_sends_nothing() {
        let (s, data) = with_cover("broken", b"no picture at all");
        assert_eq!(make(&s.serve).await.unwrap(), 1);
        assert_eq!(whole_now(&s).await.1, BTreeMap::new());
        assert_eq!(make(&s.serve).await.unwrap(), 0, "not tried again until it changes");
        std::fs::remove_dir_all(&data).ok();
    }

    /// The record a test's library holds, by the cover it has.
    fn album_of(s: &Served) -> String {
        s.serve.cover_stamps().keys().next().cloned().unwrap()
    }

    /// Finding 86: a list's cover comes no larger than it asks, and the whole one stays whole for the player and a panel.
    #[tokio::test]
    async fn a_list_gets_its_cover_no_larger_than_it_asks_and_the_whole_one_stays_whole() {
        let (s, data) = with_cover("list", &picture([200, 30, 30], 1000));
        let album = album_of(&s);
        let (ct, small) = for_list(&s.serve, &album, 320).await.unwrap();
        assert_eq!(ct, "image/jpeg");
        let shown = image::load_from_memory(&small).unwrap();
        assert_eq!((shown.width(), shown.height()), (320, 320));
        let (_, whole) = s.serve.cover_bytes(&album).await.unwrap().unwrap();
        assert_eq!(image::load_from_memory(&whole).unwrap().width(), 1000);
        std::fs::remove_dir_all(&data).ok();
    }

    /// Made once, a size is the disk cache's: asked again it comes with nothing of the picture read.
    #[tokio::test]
    async fn a_size_made_once_comes_again_with_its_picture_gone() {
        let (s, data) = with_cover("cached", &picture([200, 30, 30], 1000));
        let album = album_of(&s);
        let first = for_list(&s.serve, &album, 320).await.unwrap().1;
        std::fs::remove_file(s.roots[0].join("A/cover.jpg")).unwrap();
        assert_eq!(for_list(&s.serve, &album, 320).await.unwrap().1, first);
        assert_eq!(for_list(&s.serve, &album, 640).await, None, "another size is another making, and there is nothing to make it of");
        std::fs::remove_dir_all(&data).ok();
    }

    /// A cover changed has another print, so a list never shows the one before.
    #[tokio::test]
    async fn a_cover_changed_is_made_again_at_its_size() {
        let (s, data) = with_cover("changed", &picture([200, 30, 30], 400));
        for_list(&s.serve, &album_of(&s), 200).await.unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(s.roots[0].join("A/cover.jpg"), picture([30, 200, 30], 400)).unwrap();
        s.cat.walk_all();
        s.built();
        s.serve.covers_read_again_for_tests();
        let green = image::load_from_memory(&for_list(&s.serve, &album_of(&s), 200).await.unwrap().1).unwrap().to_rgb8();
        assert!(green.get_pixel(5, 5)[1] > 150, "{:?}", green.get_pixel(5, 5));
        std::fs::remove_dir_all(&data).ok();
    }

    /// A size no larger than the small covers made here is one of them: no second maker, no decode.
    #[tokio::test]
    async fn a_small_size_is_the_small_cover_made_already() {
        let (s, data) = with_cover("small", &picture([200, 30, 30], 1000));
        let album = album_of(&s);
        make(&s.serve).await.unwrap();
        let made = std::fs::read_dir(data.join(FOLDER)).unwrap().next().unwrap().unwrap().path();
        std::fs::write(&made, b"made here").unwrap();
        assert_eq!(for_list(&s.serve, &album, 96).await.unwrap().1, b"made here");
        assert_eq!(for_list(&s.serve, &album, SIDE).await.unwrap().1, b"made here");
        assert_ne!(for_list(&s.serve, &album, SIDE + 1).await.unwrap().1, b"made here", "larger than they are, made at its size");
        std::fs::remove_dir_all(&data).ok();
    }

    /// A picture the maker could not read leaves an empty file: that is no cover for a list, which gets the picture as it is.
    #[tokio::test]
    async fn a_small_cover_that_could_not_be_made_is_no_answer() {
        let (s, data) = with_cover("unmade", b"no picture at all");
        make(&s.serve).await.unwrap();
        assert_eq!(for_list(&s.serve, &album_of(&s), 96).await.unwrap().1, b"no picture at all");
        std::fs::remove_dir_all(&data).ok();
    }

    /// What asks for a cover cannot wait for the maker: a small size not made yet is made at once.
    #[tokio::test]
    async fn a_small_size_not_made_yet_is_made_at_once() {
        let (s, data) = with_cover("not-yet", &picture([200, 30, 30], 1000));
        assert_eq!(image::load_from_memory(&for_list(&s.serve, &album_of(&s), 96).await.unwrap().1).unwrap().width(), 96);
        std::fs::remove_dir_all(&data).ok();
    }

    #[tokio::test]
    async fn a_cover_smaller_than_asked_comes_as_it_is() {
        let original = picture([200, 30, 30], 300);
        let (s, data) = with_cover("as-is", &original);
        assert_eq!(for_list(&s.serve, &album_of(&s), 640).await.unwrap().1, original);
        std::fs::remove_dir_all(&data).ok();
    }

    #[tokio::test]
    async fn a_size_out_of_reason_is_held_to_the_sizes_made() {
        let (s, data) = with_cover("bounds", &picture([200, 30, 30], 2100));
        let album = album_of(&s);
        assert_eq!(image::load_from_memory(&for_list(&s.serve, &album, 1).await.unwrap().1).unwrap().width(), *SIZES.start());
        assert_eq!(image::load_from_memory(&for_list(&s.serve, &album, 100_000).await.unwrap().1).unwrap().width(), *SIZES.end());
        std::fs::remove_dir_all(&data).ok();
    }

    #[tokio::test]
    async fn a_record_with_no_cover_has_none_at_any_size() {
        let data = std::env::temp_dir().join(format!("cratarr-device-covers-none-{}", std::process::id()));
        std::fs::create_dir_all(&data).unwrap();
        let s = served_in("none", &[CopyKind::Main], Some(&data));
        s.add(0, &[("A/01.flac", &[("album", "Record"), ("albumartist", "Someone"), ("title", "One")])]);
        s.built();
        let album = s.serve.library.album_ids().unwrap().into_iter().next().unwrap();
        assert_eq!(for_list(&s.serve, &album, 320).await, None);
        std::fs::remove_dir_all(&data).ok();
    }

    /// A track's row asks by its song: its record's cover.
    #[tokio::test]
    async fn a_song_is_known_by_its_record() {
        let (s, data) = with_cover("song", &picture([200, 30, 30], 200));
        let song = ids::song(s.cat.track_of(0, "A/01.flac").unwrap());
        assert_eq!(s.serve.album_of_song(&song), Some(album_of(&s)));
        assert_eq!(s.serve.album_of_song("tr-999999"), None);
        std::fs::remove_dir_all(&data).ok();
    }

    /// A Subsonic client's 304 is answered by the library's print: nothing of the picture is read (finding 87).
    #[tokio::test]
    async fn a_304_reads_nothing_of_the_picture() {
        use axum::http::{HeaderMap, HeaderValue};
        let (s, data) = with_cover("unchanged", &picture([200, 30, 30], 400));
        let album = album_of(&s);
        let print = s.serve.cover_stamps().get(&album).cloned().unwrap();
        std::fs::remove_file(s.roots[0].join("A/cover.jpg")).unwrap();
        let mut asked = HeaderMap::new();
        asked.insert(header::IF_NONE_MATCH, HeaderValue::from_str(&format!("\"{print}-300\"")).unwrap());
        assert_eq!(s.serve.cover(&album, Some(300), &asked).await.unwrap().status(), StatusCode::NOT_MODIFIED);
        assert_eq!(s.serve.cover(&album, Some(300), &HeaderMap::new()).await.unwrap().status(), StatusCode::NOT_FOUND, "read, it is gone");
        std::fs::remove_dir_all(&data).ok();
    }

    /// Subsonic's sizes come the same way: one making, kept.
    #[tokio::test]
    async fn a_subsonic_size_is_made_and_kept_as_a_lists() {
        use axum::http::HeaderMap;
        let (s, data) = with_cover("subsonic", &picture([200, 30, 30], 1000));
        let album = album_of(&s);
        let sent = s.serve.cover(&album, Some(300), &HeaderMap::new()).await.unwrap();
        let body = axum::body::to_bytes(sent.into_body(), usize::MAX).await.unwrap();
        assert_eq!(image::load_from_memory(&body).unwrap().width(), 300);
        std::fs::remove_file(s.roots[0].join("A/cover.jpg")).unwrap();
        assert_eq!(for_list(&s.serve, &album, 300).await.unwrap().1, body.to_vec(), "one cache for both");
        std::fs::remove_dir_all(&data).ok();
    }

    /// Finding 86's number: what the server holds for a grid of 100 records' covers, whole as lists asked them before and sized as
    /// now, cold and warm. Its peak, from /proc after resetting it, so it reads a scenario's own; a picture as a real cover is, which
    /// JPEG keeps large.
    #[test]
    #[ignore = "a measure: --ignored measured_grid_of_100 --nocapture --test-threads=1, with dependencies optimised"]
    fn measured_grid_of_100_records() {
        const RECORDS: usize = 100;
        let status = |field: &str| -> u64 {
            let text = std::fs::read_to_string("/proc/self/status").unwrap();
            let line = text.lines().find(|l| l.starts_with(field)).unwrap();
            line.split_whitespace().nth(1).unwrap().parse::<u64>().unwrap() / 1024
        };
        let reset = || std::fs::write("/proc/self/clear_refs", "5").unwrap();
        let data = std::env::temp_dir().join(format!("cratarr-device-covers-grid-{}", std::process::id()));
        std::fs::remove_dir_all(&data).ok();
        std::fs::create_dir_all(&data).unwrap();
        let s = served_in("grid", &[CopyKind::Main], Some(&data));
        let mut r = cratarr_rules::Rng::seeded(86);
        let picture = RgbImage::from_fn(3000, 3000, |x, y| {
            let grain = (r.unit() * 48.0) as u32;
            Rgb([((x / 12 + grain) % 256) as u8, ((y / 12 + grain) % 256) as u8, (((x + y) / 24 + grain) % 256) as u8])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92).encode_image(&picture).unwrap();
        let cover = out.into_inner();
        drop(picture);
        let files: Vec<(String, Vec<(String, String)>)> = (0..RECORDS)
            .map(|n| {
                std::fs::create_dir_all(s.roots[0].join(format!("R{n}"))).unwrap();
                std::fs::write(s.roots[0].join(format!("R{n}/cover.jpg")), &cover).unwrap();
                (format!("R{n}/01.flac"), vec![("album".to_string(), format!("Record {n}")), ("albumartist".to_string(), "Someone".to_string()), ("title".to_string(), "One".to_string())])
            })
            .collect();
        let tags: Vec<Vec<(&str, &str)>> = files.iter().map(|(_, t)| t.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()).collect();
        let listed: Vec<(&str, &[(&str, &str)])> = files.iter().zip(&tags).map(|((rel, _), t)| (rel.as_str(), t.as_slice())).collect();
        s.add(0, &listed);
        s.built();
        let albums: Vec<String> = s.serve.cover_stamps().keys().cloned().collect();
        assert_eq!(albums.len(), RECORDS);
        println!("a cover: 3000 by 3000, {:.1} MB; {RECORDS} records", cover.len() as f64 / 1_048_576.0);
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap();
        let grid = |what: &str, at_once: usize, sized: bool| {
            let before = status("VmRSS:");
            reset();
            let began = std::time::Instant::now();
            // Each ask as a request is: in flight with its bytes until it is sent, `at_once` of them at a time.
            let sent: usize = runtime.block_on(async {
                let gate = Arc::new(tokio::sync::Semaphore::new(at_once));
                let mut asks = tokio::task::JoinSet::new();
                for album in albums.iter().cloned() {
                    let (serve, gate) = (s.serve.clone(), gate.clone());
                    asks.spawn(async move {
                        let _turn = gate.acquire_owned().await.unwrap();
                        let got = if sized { for_list(&serve, &album, 320).await } else { serve.cover_bytes(&album).await.unwrap() };
                        got.map_or(0, |(_, bytes)| bytes.len())
                    });
                }
                let mut all = 0;
                while let Some(one) = asks.join_next().await {
                    all += one.unwrap();
                }
                all
            });
            println!(
                "{what:<34} {at_once:>3} at once: peak {:>4} MB over the {before} MB before, {:>5.1} s, {:.1} MB sent",
                status("VmHWM:").saturating_sub(before),
                began.elapsed().as_secs_f64(),
                sent as f64 / 1_048_576.0
            );
        };
        grid("whole, as lists asked before", RECORDS, false);
        grid("whole, as lists asked before", 6, false);
        grid("sized 320, cold", RECORDS, true);
        grid("sized 320, warm", RECORDS, true);
        grid("sized 320, warm", 6, true);
        std::fs::remove_dir_all(&data).ok();
    }

    /// A picture that looks like a real cover to JPEG: colour fields of every size, smooth, as 1/f noise is. `fine`: how much grain
    /// over it, 0 for a clean print, as a photograph has some.
    fn cover_like(side: u32, seed: u64, fine: f32) -> RgbImage {
        let mut r = cratarr_rules::Rng::seeded(seed);
        let mut plane = vec![[0f32; 3]; (side * side) as usize];
        let mut weight = 90.0f32;
        let mut cells = 2u32;
        while cells < side {
            let grid: Vec<[f32; 3]> = (0..(cells + 1) * (cells + 1)).map(|_| [r.unit() as f32 - 0.5, r.unit() as f32 - 0.5, r.unit() as f32 - 0.5]).collect();
            let step = side as f32 / cells as f32;
            for y in 0..side {
                for x in 0..side {
                    let (gx, gy) = (x as f32 / step, y as f32 / step);
                    let (ix, iy) = (gx as u32, gy as u32);
                    let (fx, fy) = (gx - ix as f32, gy - iy as f32);
                    let at = |a: u32, b: u32| grid[(b * (cells + 1) + a) as usize];
                    for c in 0..3 {
                        let top = at(ix, iy)[c] * (1.0 - fx) + at(ix + 1, iy)[c] * fx;
                        let low = at(ix, iy + 1)[c] * (1.0 - fx) + at(ix + 1, iy + 1)[c] * fx;
                        plane[(y * side + x) as usize][c] += weight * (top * (1.0 - fy) + low * fy);
                    }
                }
            }
            weight *= 0.72;
            cells *= 2;
        }
        RgbImage::from_fn(side, side, |x, y| {
            let p = plane[(y * side + x) as usize];
            let grain = fine * (r.unit() as f32 - 0.5);
            Rgb(p.map(|v| (128.0 + v + grain).clamp(0.0, 255.0) as u8))
        })
    }

    /// Noise in blocks as large as a small cover's pixels, so that making it small leaves it noise: what JPEG can do least with.
    fn speckled(side: u32, seed: u64) -> RgbImage {
        let mut r = cratarr_rules::Rng::seeded(seed);
        let cells = side.div_ceil(4);
        let grid: Vec<[u8; 3]> = (0..cells * cells).map(|_| [0, 0, 0].map(|_: u8| (r.unit() * 255.0) as u8)).collect();
        RgbImage::from_fn(side, side, |x, y| Rgb(grid[((y / 4) * cells + x / 4) as usize]))
    }

    /// PREVIEW2's Q4 waits on this: what small covers weigh over a library of 3,146 records. No real cover can be reached from here,
    /// and staging's are off limits, so this is an estimate between two ends that hold: the fixture's own, one colour each, the
    /// floor; noise a small cover keeps, more than any cover has, the ceiling. Pictures made to look like covers to JPEG lie between,
    /// and they are smooth where a real cover has type and hard edges: their low side. Only a run over real covers (D22) settles it.
    #[test]
    #[ignore = "a measure: rt-side.sh test measured_small_covers --ignored --nocapture --test-threads=1"]
    fn measured_small_covers_for_3146_records() {
        const RECORDS: f64 = 3_146.0;
        let source = |picture: RgbImage| {
            let mut out = std::io::Cursor::new(Vec::new());
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92).encode_image(&picture).unwrap();
            out.into_inner()
        };
        let solid = |n: u32| RgbImage::from_pixel(600, 600, Rgb([(n * 53 % 200 + 40) as u8, (n * 97 % 200 + 40) as u8, (n * 31 % 200 + 40) as u8]));
        let kinds: Vec<(&str, Vec<Vec<u8>>)> = vec![
            ("the fixture's, one colour", (0..8).map(|n| source(solid(n))).collect()),
            ("like a clean print", (0..8).map(|n| source(cover_like(600, n, 0.0))).collect()),
            ("like a photograph", (0..8).map(|n| source(cover_like(600, 100 + n, 24.0))).collect()),
            ("heavy grain", (0..8).map(|n| source(cover_like(600, 200 + n, 90.0))).collect()),
            ("noise a small cover keeps", (0..8).map(|n| source(speckled(600, 300 + n))).collect()),
        ];
        println!("a record's small cover, and {RECORDS} of them, in MB");
        for (side, quality) in [(96, 60), (96, 75), (128, 60), (128, 75), (160, 60), (160, 75), (160, 85), (200, 75), (240, 75), (300, 75)] {
            for (what, pictures) in &kinds {
                let sizes: Vec<usize> = pictures.iter().map(|p| art::jpeg_at(p, side, quality).unwrap().len()).collect();
                let mean = sizes.iter().sum::<usize>() as f64 / sizes.len() as f64;
                println!("{side:>3} px q{quality}: {:>7.0} bytes, {:>6.1} MB  {what}", mean, mean * RECORDS / 1_048_576.0);
            }
        }
    }
}
