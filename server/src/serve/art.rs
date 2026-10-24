//! Album art for the OpenSubsonic API (decision 0030): the folder's cover image, else the art embedded in the album's
//! first track; resized to the size a client asks for, and kept resized in a disk cache with a size limit.
//!
//! Patterns from Navidrome (https://github.com/navidrome/navidrome, GPL-3.0-or-later, Copyright the Navidrome authors),
//! translated: a cover's id carries a fingerprint of the picture, so a client may keep it for good and asks again only
//! when it changes (`server/imghttp/headers.go`); sources in the order folder file, then embedded
//! (`conf/configuration.go`); resized copies in an on-disk cache of limited size, oldest out first
//! (`core/artwork/image_cache.go`). The resizing follows nghe's (https://github.com/vnghia/nghe, AGPL-3.0, Copyright
//! vnghia): the `image` crate's Triangle filter. Changed: JPEG out at quality 85; one resize at a time (a client asks for
//! hundreds of covers at once, and decoding a large picture takes tens of megabytes).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use md5::{Digest, Md5};

/// The largest picture read (as Cratarr's folder covers, `covers::MAX_BYTES`).
pub const MAX_BYTES: u64 = crate::covers::MAX_BYTES;

/// The cache's size; the oldest copies go when it is passed (Navidrome's default image cache is 100 MB).
pub const CACHE_BYTES: u64 = 100 * 1024 * 1024;

/// The best cover file directly in an album's folder, by Cratarr's candidate names (`covers::CANDIDATES`).
pub fn cover_file(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(usize, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()) {
        let name = e.file_name();
        let Some(rank) = name.to_str().and_then(crate::covers::candidate_rank) else { continue };
        // An empty file is no cover.
        if e.file_type().is_ok_and(|t| t.is_file()) && e.metadata().is_ok_and(|m| m.len() > 0) && best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, e.path()));
        }
    }
    best.map(|(_, p)| p)
}

/// A short fingerprint of a file: it changes when the file does (its name, size and time).
pub fn print_of(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let at = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    let h = Md5::digest(format!("{}\n{}\n{at}", path.display(), meta.len()).as_bytes());
    Some(h.iter().take(5).map(|b| format!("{b:02x}")).collect())
}

/// The fingerprint of an album's art: its folder file's, else its first track's (its embedded picture changes with it);
/// none when it has neither (the owner, 2026-10-24: a first track without a picture had counted as a cover, and Home
/// showed such albums blank). The first track is opened only for an album without a cover file, which the library's
/// rule makes rare (covers inside the files and next to them, `covers_inside`).
pub fn album_print(dir: &Path, first_track: Option<&Path>) -> Option<String> {
    match cover_file(dir) {
        Some(f) => print_of(&f),
        None => first_track.filter(|t| embedded(t).is_some()).and_then(print_of),
    }
}

/// The picture embedded in a track, the front cover first: (content type, bytes).
pub fn embedded(track: &Path) -> Option<(String, Vec<u8>)> {
    use lofty::config::ParseOptions;
    use lofty::picture::PictureType;
    use lofty::prelude::*;
    let file = lofty::probe::Probe::open(track).ok()?.options(ParseOptions::new().read_cover_art(true)).read().ok()?;
    let pictures: Vec<&lofty::picture::Picture> = file.tags().iter().flat_map(|t| t.pictures()).collect();
    let pick = pictures.iter().find(|p| p.pic_type() == PictureType::CoverFront).or_else(|| pictures.first())?;
    let data = pick.data();
    if data.is_empty() || data.len() as u64 > MAX_BYTES {
        return None;
    }
    let ct = pick.mime_type().map(|m| m.as_str().to_string()).filter(|m| m.starts_with("image/")).unwrap_or_else(|| sniff(data).to_string());
    Some((ct, data.to_vec()))
}

/// A picture's type from its first bytes.
pub fn sniff(data: &[u8]) -> &'static str {
    match data {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'G', b'I', b'F', ..] => "image/gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        _ => "image/jpeg",
    }
}

/// A picture no larger than `size` on its longer side, as JPEG; `None` when it is that small already.
pub fn resize(data: &[u8], size: u32) -> Result<Option<Vec<u8>>> {
    let img = image::load_from_memory(data).context("not a picture Cratarr can read")?;
    if img.width().max(img.height()) <= size {
        return Ok(None);
    }
    let small = img.resize(size, size, image::imageops::FilterType::Triangle).to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85).encode_image(&small)?;
    Ok(Some(out.into_inner()))
}

/// A picture as JPEG, no larger than `size` on its longer side (a crate's own picture): resized when larger, else only
/// written again as JPEG.
pub fn jpeg_within(data: &[u8], size: u32) -> Result<Vec<u8>> {
    if let Some(small) = resize(data, size)? {
        return Ok(small);
    }
    let img = image::load_from_memory(data).context("not a picture Cratarr can read")?.to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85).encode_image(&img)?;
    Ok(out.into_inner())
}

/// Resized covers on disk, at most `limit` bytes; the oldest go first.
pub struct Cache {
    dir: Option<PathBuf>,
    limit: u64,
}

impl Cache {
    pub fn new(dir: Option<PathBuf>, limit: u64) -> Self {
        Self { dir, limit }
    }

    fn file(&self, key: &str) -> Option<PathBuf> {
        let name: String = Md5::digest(key.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        self.dir.as_ref().map(|d| d.join(format!("{name}.jpg")))
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        std::fs::read(self.file(key)?).ok()
    }

    pub fn put(&self, key: &str, data: &[u8]) {
        let (Some(dir), Some(file)) = (self.dir.as_ref(), self.file(key)) else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let tmp = file.with_extension("tmp");
        if std::fs::write(&tmp, data).is_ok() {
            let _ = std::fs::rename(&tmp, &file);
        }
        self.trim();
    }

    /// Takes the oldest copies out until the cache fits its limit.
    fn trim(&self) {
        let Some(dir) = &self.dir else { return };
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                m.is_file().then(|| (m.modified().unwrap_or(std::time::UNIX_EPOCH), m.len(), e.path()))
            })
            .collect();
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        if total <= self.limit {
            return;
        }
        files.sort();
        for (_, len, path) in files {
            if total <= self.limit {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 30, 30]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn pictures_shrink_to_the_size_asked_and_never_grow() {
        let big = png(600, 300);
        let small = resize(&big, 150).unwrap().unwrap();
        let back = image::load_from_memory(&small).unwrap();
        assert_eq!((back.width(), back.height()), (150, 75), "aspect kept");
        assert!(resize(&big, 600).unwrap().is_none(), "no larger than it is");
        assert_eq!(sniff(&big), "image/png");
    }

    #[test]
    fn the_cache_keeps_to_its_limit_oldest_out() {
        let dir = std::env::temp_dir().join(format!("cratarr-art-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cache = Cache::new(Some(dir.clone()), 25);
        cache.put("a", &[1; 10]);
        std::thread::sleep(std::time::Duration::from_millis(20));
        cache.put("b", &[2; 10]);
        std::thread::sleep(std::time::Duration::from_millis(20));
        cache.put("c", &[3; 10]);
        assert_eq!(cache.get("a"), None, "the oldest went");
        assert_eq!(cache.get("c"), Some(vec![3; 10]));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_cover_is_picked_by_name_and_fingerprinted() {
        let dir = std::env::temp_dir().join(format!("cratarr-cover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Folder.jpg"), b"x").unwrap();
        std::fs::write(dir.join("cover.png"), b"y").unwrap();
        std::fs::write(dir.join("01.flac"), b"z").unwrap();
        assert_eq!(cover_file(&dir).unwrap().file_name().unwrap(), "cover.png");
        let p = album_print(&dir, None).unwrap();
        assert_eq!(p.len(), 10);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
