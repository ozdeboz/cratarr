//! Album covers, read straight from the album folder in the music library, else the picture embedded in the track. The
//! folder is derived from the playing track's library entry (never from anything the client sends), only a fixed list
//! of file names is ever read, and nothing outside the music folder is served, symlinks included.

use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Cover file names, best first. Matched case-insensitively.
pub const CANDIDATES: [&str; 8] =
    ["cover.jpg", "cover.jpeg", "cover.png", "folder.jpg", "folder.jpeg", "folder.png", "front.jpg", "front.png"];

/// Larger files are refused: the container has a small memory budget and the file is read whole.
pub const MAX_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    NotFound,
    NotModified { etag: String },
    Image { content_type: &'static str, etag: String, bytes: Vec<u8> },
}

/// The album folder of a library entry (`Folder/File.mp3` gives `Folder`), relative to the music folder. `None` when
/// the entry could escape the folder (`..`, absolute, backslash, NUL, empty parts) or sits directly in the root.
pub fn album_dir(entry: &str) -> Option<PathBuf> {
    if entry.is_empty() || entry.contains(['\\', '\0']) || entry.starts_with('/') {
        return None;
    }
    let parts: Vec<&str> = entry.split('/').collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == "..") {
        return None;
    }
    let (_file, dirs) = parts.split_last()?;
    if dirs.is_empty() {
        return None;
    }
    let dir: PathBuf = dirs.iter().collect();
    // Belt and braces: Windows-style prefixes (`C:`) and anything else that is not a plain name.
    dir.components().all(|c| matches!(c, Component::Normal(_))).then_some(dir)
}

/// Rank of a file name in the candidate list, if it is one.
pub fn candidate_rank(name: &str) -> Option<usize> {
    let lower = name.to_lowercase();
    CANDIDATES.iter().position(|c| *c == lower)
}

pub fn content_type(name: &str) -> Option<&'static str> {
    match name.rsplit('.').next()?.to_lowercase().as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        _ => None,
    }
}

/// A strong-enough validator: modification time (nanosecond resolution) and size.
pub fn etag(modified: std::time::SystemTime, len: u64) -> String {
    let d = modified.duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("\"{:x}.{:x}-{:x}\"", d.as_secs(), d.subsec_nanos(), len)
}

/// Does an `If-None-Match` header value match this ETag? Handles lists, `*` and weak prefixes.
pub fn etag_matches(header: &str, etag: &str) -> bool {
    header.split(',').map(str::trim).any(|t| t == "*" || t.strip_prefix("W/").unwrap_or(t) == etag)
}

/// The best cover file directly inside `dir` (an existing album folder), by candidate order.
async fn pick_cover(dir: &Path) -> Option<PathBuf> {
    let mut rd = tokio::fs::read_dir(dir).await.ok()?;
    let mut best: Option<(usize, PathBuf)> = None;
    while let Ok(Some(e)) = rd.next_entry().await {
        let name = e.file_name();
        let Some(rank) = name.to_str().and_then(candidate_rank) else { continue };
        if best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, e.path()));
        }
    }
    best.map(|(_, p)| p)
}

/// Resolve the cover for a library entry. Everything that goes wrong is `NotFound`.
pub async fn resolve(music_root: &Path, entry: &str, if_none_match: Option<&str>) -> Outcome {
    let Some(rel) = album_dir(entry) else { return Outcome::NotFound };
    let Ok(root) = tokio::fs::canonicalize(music_root).await else { return Outcome::NotFound };
    // Canonicalise the album folder first so a symlinked folder cannot lead out of the library either.
    let Ok(dir) = tokio::fs::canonicalize(root.join(rel)).await else { return Outcome::NotFound };
    if !dir.starts_with(&root) {
        return Outcome::NotFound;
    }
    let Some(found) = pick_cover(&dir).await else { return embedded_cover(&root, entry, if_none_match).await };
    // A cover that is a symlink must still end up inside the library.
    let Ok(file) = tokio::fs::canonicalize(&found).await else { return Outcome::NotFound };
    if !file.starts_with(&root) {
        return Outcome::NotFound;
    }
    let Some(ct) = found.file_name().and_then(|n| n.to_str()).and_then(content_type) else { return Outcome::NotFound };
    let Ok(meta) = tokio::fs::metadata(&file).await else { return Outcome::NotFound };
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return Outcome::NotFound;
    }
    let etag = etag(meta.modified().unwrap_or(UNIX_EPOCH), meta.len());
    if if_none_match.is_some_and(|h| etag_matches(h, &etag)) {
        return Outcome::NotModified { etag };
    }
    match tokio::fs::read(&file).await {
        Ok(bytes) if bytes.len() as u64 <= MAX_BYTES => Outcome::Image { content_type: ct, etag, bytes },
        _ => Outcome::NotFound,
    }
}

/// Embedded pictures read at a time, at most: each may be megabytes, a screen of covers asks for dozens at once, and
/// the container's memory is small.
static EMBEDDED: std::sync::LazyLock<tokio::sync::Semaphore> = std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(2));

/// No cover file in the album folder: the picture embedded in the track itself (many albums carry their art only
/// there), as the OpenSubsonic API's covers have it (`serve::art`). Its fingerprint is the track's: it changes with it.
async fn embedded_cover(root: &Path, entry: &str, if_none_match: Option<&str>) -> Outcome {
    let Ok(track) = tokio::fs::canonicalize(root.join(entry)).await else { return Outcome::NotFound };
    if !track.starts_with(root) {
        return Outcome::NotFound;
    }
    let Ok(meta) = tokio::fs::metadata(&track).await else { return Outcome::NotFound };
    if !meta.is_file() {
        return Outcome::NotFound;
    }
    let etag = etag(meta.modified().unwrap_or(UNIX_EPOCH), meta.len());
    if if_none_match.is_some_and(|h| etag_matches(h, &etag)) {
        return Outcome::NotModified { etag };
    }
    let Ok(_turn) = EMBEDDED.acquire().await else { return Outcome::NotFound };
    let Some((ct, bytes)) = tokio::task::spawn_blocking(move || crate::serve::art::embedded(&track)).await.ok().flatten() else {
        return Outcome::NotFound;
    };
    let Some(content_type) = ["image/jpeg", "image/png", "image/webp", "image/gif"].into_iter().find(|t| *t == ct) else { return Outcome::NotFound };
    Outcome::Image { content_type, etag, bytes }
}

/// Jellyfin item ids are hex GUIDs; anything else is not even looked up.
pub fn valid_item_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("cratarr-cover-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn write(&self, name: &str, content: impl AsRef<[u8]>) {
            let p = self.0.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn image(o: Outcome) -> (&'static str, String, Vec<u8>) {
        match o {
            Outcome::Image { content_type, etag, bytes } => (content_type, etag, bytes),
            other => panic!("expected an image, got {other:?}"),
        }
    }

    #[test]
    fn album_dir_accepts_plain_entries() {
        assert_eq!(album_dir("Album (2020)/01 Song.mp3"), Some(PathBuf::from("Album (2020)")));
        assert_eq!(album_dir("A/B/c.mp3"), Some(PathBuf::from("A").join("B")));
    }

    #[test]
    fn album_dir_rejects_escapes_and_oddities() {
        for bad in [
            "", "Song.mp3", "../x/a.mp3", "A/../../x.mp3", "A/./b.mp3", "/etc/a.mp3", "A//b.mp3", "A\\..\\b.mp3",
            "A\\b.mp3", "A/b\0.mp3", "..\\a.mp3", "A/..",
        ] {
            assert_eq!(album_dir(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn candidates_ranked_case_insensitively() {
        assert_eq!(candidate_rank("cover.jpg"), Some(0));
        assert_eq!(candidate_rank("COVER.JPG"), Some(0));
        assert_eq!(candidate_rank("Folder.PNG"), Some(5));
        assert_eq!(candidate_rank("front.png"), Some(7));
        assert_eq!(candidate_rank("cover.gif"), None);
        assert_eq!(candidate_rank("album.jpg"), None);
        assert_eq!(candidate_rank("../cover.jpg"), None);
    }

    #[test]
    fn content_types() {
        assert_eq!(content_type("cover.JPG"), Some("image/jpeg"));
        assert_eq!(content_type("x.jpeg"), Some("image/jpeg"));
        assert_eq!(content_type("x.png"), Some("image/png"));
        assert_eq!(content_type("x.gif"), None);
    }

    #[test]
    fn etag_changes_with_time_and_size_and_matches() {
        let t = UNIX_EPOCH + std::time::Duration::new(1_700_000_000, 5);
        let e = etag(t, 10);
        assert_ne!(e, etag(t, 11));
        assert_ne!(e, etag(t + std::time::Duration::from_secs(1), 10));
        assert!(etag_matches(&e, &e));
        assert!(etag_matches(&format!("\"zzz\", {e}"), &e));
        assert!(etag_matches(&format!("W/{e}"), &e));
        assert!(etag_matches("*", &e));
        assert!(!etag_matches("\"other\"", &e));
    }

    #[test]
    fn item_ids() {
        assert!(valid_item_id("0123abcd-ef"));
        for bad in ["", "../x", "a/b", "a?b=c", "a b", "a\0"] {
            assert!(!valid_item_id(bad), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn serves_cover_with_headers_data() {
        let d = Dir::new();
        d.write("Album/cover.jpg", b"JPEGDATA");
        let (ct, etag, bytes) = image(resolve(&d.0, "Album/01 Song.mp3", None).await);
        assert_eq!(ct, "image/jpeg");
        assert_eq!(bytes, b"JPEGDATA");
        assert!(etag.starts_with('"'));
    }

    #[tokio::test]
    async fn conditional_request_gives_not_modified() {
        let d = Dir::new();
        d.write("Album/cover.png", b"PNG");
        let (ct, etag, _) = image(resolve(&d.0, "Album/a.mp3", None).await);
        assert_eq!(ct, "image/png");
        assert_eq!(resolve(&d.0, "Album/a.mp3", Some(&etag)).await, Outcome::NotModified { etag: etag.clone() });
        assert!(matches!(resolve(&d.0, "Album/a.mp3", Some("\"stale\"")).await, Outcome::Image { .. }));
        // A changed file (different size) invalidates the old ETag.
        d.write("Album/cover.png", b"PNG2");
        assert!(matches!(resolve(&d.0, "Album/a.mp3", Some(&etag)).await, Outcome::Image { .. }));
    }

    #[tokio::test]
    async fn picks_best_candidate_and_ignores_other_files() {
        let d = Dir::new();
        d.write("Album/Front.PNG", b"front");
        d.write("Album/Folder.jpg", b"folder");
        d.write("Album/notes.jpg", b"notes");
        assert_eq!(image(resolve(&d.0, "Album/a.mp3", None).await).2, b"folder");
        d.write("Album/COVER.PNG", b"cover");
        assert_eq!(image(resolve(&d.0, "Album/a.mp3", None).await).2, b"cover");
    }

    #[tokio::test]
    async fn not_found_cases() {
        let d = Dir::new();
        d.write("NoCover/a.mp3", b"x");
        d.write("Other/notes.jpg", b"x");
        d.write("cover.jpg", b"root cover");
        for entry in ["NoCover/a.mp3", "Other/a.mp3", "Missing/a.mp3", "a.mp3", "../a/b.mp3", "/x/y.mp3", ""] {
            assert_eq!(resolve(&d.0, entry, None).await, Outcome::NotFound, "{entry:?}");
        }
    }

    /// A short silent WAV with `picture` embedded as its front cover (an ID3v2 chunk).
    fn wav_with_picture(path: &Path, picture: &[u8]) {
        use lofty::picture::{MimeType, Picture, PictureType};
        use lofty::prelude::*;
        use lofty::tag::{Tag, TagType};
        let (rate, samples) = (8000u32, 800u32);
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + samples * 2).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(samples * 2).to_le_bytes());
        w.resize(w.len() + samples as usize * 2, 0);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, w).unwrap();
        let mut file = lofty::read_from_path(path).unwrap();
        let mut tag = Tag::new(TagType::Id3v2);
        tag.push_picture(Picture::unchecked(picture.to_vec()).pic_type(PictureType::CoverFront).mime_type(MimeType::Png).build());
        file.insert_tag(tag);
        file.save_to_path(path, lofty::config::WriteOptions::default()).unwrap();
    }

    #[tokio::test]
    async fn without_a_cover_file_the_tracks_embedded_picture() {
        let d = Dir::new();
        let png = b"\x89PNG\r\n\x1a\nembedded".to_vec();
        wav_with_picture(&d.0.join("Embedded/01 a.wav"), &png);
        let (ct, etag, bytes) = image(resolve(&d.0, "Embedded/01 a.wav", None).await);
        assert_eq!((ct, bytes), ("image/png", png.clone()));
        assert_eq!(resolve(&d.0, "Embedded/01 a.wav", Some(&etag)).await, Outcome::NotModified { etag });
        // A cover file in the folder still comes first.
        d.write("Embedded/cover.jpg", b"folder cover");
        assert_eq!(image(resolve(&d.0, "Embedded/01 a.wav", None).await).2, b"folder cover");
    }

    #[tokio::test]
    async fn oversized_cover_refused() {
        let d = Dir::new();
        d.write("Album/cover.jpg", vec![0u8; MAX_BYTES as usize + 1]);
        assert_eq!(resolve(&d.0, "Album/a.mp3", None).await, Outcome::NotFound);
        d.write("Album/cover.jpg", vec![0u8; MAX_BYTES as usize]);
        assert!(matches!(resolve(&d.0, "Album/a.mp3", None).await, Outcome::Image { .. }));
    }

    #[tokio::test]
    async fn directory_named_like_a_cover_is_not_served() {
        let d = Dir::new();
        d.write("Album/cover.jpg/inner.txt", b"x");
        assert_eq!(resolve(&d.0, "Album/a.mp3", None).await, Outcome::NotFound);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlinks_escaping_the_library_are_not_followed() {
        use std::os::unix::fs::symlink;
        let outside = Dir::new();
        outside.write("secret/cover.jpg", b"secret");
        outside.write("file.jpg", b"secret");
        let lib = Dir::new();
        // Album folder that is a symlink out of the library.
        symlink(outside.0.join("secret"), lib.0.join("Linked")).unwrap();
        assert_eq!(resolve(&lib.0, "Linked/a.mp3", None).await, Outcome::NotFound);
        // Cover file that is a symlink out of the library.
        std::fs::create_dir_all(lib.0.join("Album")).unwrap();
        symlink(outside.0.join("file.jpg"), lib.0.join("Album/cover.jpg")).unwrap();
        assert_eq!(resolve(&lib.0, "Album/a.mp3", None).await, Outcome::NotFound);
        // A symlink that stays inside the library is fine.
        lib.write("Real/cover.jpg", b"ok");
        symlink(lib.0.join("Real"), lib.0.join("Alias")).unwrap();
        assert_eq!(image(resolve(&lib.0, "Alias/a.mp3", None).await).2, b"ok");
    }
}
