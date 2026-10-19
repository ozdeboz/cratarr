//! Covers inside the files and next to them (the owner, 2026-10-19: "I want the already existing cover art to be
//! embedded as well as saved next to the content files of an album"). Before Keep or Replace moves an album into
//! Music (decision 0013), its folder is made so: a cover file from the picture inside a track when it has none, and
//! the folder's cover inside each track that has no picture. Done in the album's own folder, so the cover file moves
//! with it and an Undo takes it back too.
//!
//! The audio is never touched: a track is written to a copy first, the copy's audio compared byte for byte with the
//! original's, and only then put in the original's place. FLAC and MP3 only (the library's formats); any other track
//! is left as it is, and said so.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// What was done to an album's folder.
#[derive(Debug, Default)]
pub struct Done {
    /// The cover file written, where the folder had none.
    pub cover_file: Option<PathBuf>,
    /// The tracks that got the cover inside.
    pub embedded: Vec<PathBuf>,
    /// The tracks without a picture left as they were, and why.
    pub left: Vec<(PathBuf, String)>,
}

/// An album's tracks: their extensions.
const AUDIO: [&str; 9] = ["flac", "mp3", "m4a", "ogg", "opus", "wav", "aiff", "aif", "alac"];

/// Makes an album's folder hold its cover twice: as a file next to its tracks and inside each of them. Changes
/// nothing when there is no cover anywhere.
pub fn complete(dir: &Path) -> Result<Done> {
    let mut done = Done::default();
    let tracks = tracks_under(dir);
    if tracks.is_empty() {
        return Ok(done);
    }
    let cover = match crate::serve::art::cover_file(dir) {
        Some(file) => file,
        None => {
            // No cover file: the picture inside the first track that has one.
            let Some((mime, data)) = tracks.iter().find_map(|t| crate::serve::art::embedded(t)) else { return Ok(done) };
            let ext = match mime.as_str() {
                "image/png" => "png",
                "image/webp" => "webp",
                "image/gif" => "gif",
                _ => "jpg",
            };
            let file = dir.join(format!("cover.{ext}"));
            write_new(&file, &data)?;
            done.cover_file = Some(file.clone());
            file
        }
    };
    let data = std::fs::read(&cover).with_context(|| format!("reading {}", cover.display()))?;
    let mime = crate::serve::art::sniff(&data);
    for t in tracks {
        if crate::serve::art::embedded(&t).is_some() {
            continue;
        }
        match embed(&t, &data, mime) {
            Ok(()) => done.embedded.push(t),
            Err(e) => done.left.push((t, format!("{e:#}"))),
        }
    }
    Ok(done)
}

/// The album's audio files, in its folder and any folders in it (a disc each), in name order.
fn tracks_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.filter_map(|e| e.ok()) {
            let p = e.path();
            // A hidden file is no track (a copy being written is one).
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                todo.push(p);
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| AUDIO.contains(&x.to_ascii_lowercase().as_str())) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// A new file (never over one), readable by the library's group as its other files are.
fn write_new(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(path).with_context(|| format!("writing {}", path.display()))?;
    f.write_all(data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o664))?;
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Flac,
    Mp3,
}

/// The picture inside one track, its front cover: through a copy whose audio must be the original's, byte for byte.
fn embed(track: &Path, data: &[u8], mime: &str) -> Result<()> {
    use lofty::config::{ParseOptions, WriteOptions};
    use lofty::picture::{MimeType, Picture, PictureType};
    use lofty::prelude::*;
    use lofty::tag::Tag;
    let kind = match track.extension().and_then(|x| x.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("flac") => Kind::Flac,
        Some("mp3") => Kind::Mp3,
        _ => bail!("only FLAC and MP3 get a picture inside"),
    };
    let original = std::fs::read(track).with_context(|| format!("reading {}", track.display()))?;
    let before = audio_of(&original, kind).context("its audio could not be found")?;
    // The copy keeps the track's extension: the tag reader knows a file's format by it.
    let stem = track.file_stem().and_then(|n| n.to_str()).unwrap_or("track");
    let ext = track.extension().and_then(|x| x.to_str()).unwrap_or("");
    let copy = track.with_file_name(format!(".{stem}.cratarr-cover.{ext}"));
    std::fs::write(&copy, &original)?;
    let checked = (|| -> Result<()> {
        let mut file = lofty::probe::Probe::open(&copy)?.options(ParseOptions::new().read_properties(false)).read()?;
        if file.primary_tag_mut().is_none() {
            let kind = file.primary_tag_type();
            file.insert_tag(Tag::new(kind));
        }
        let tag = file.primary_tag_mut().context("no tag to hold the picture")?;
        tag.push_picture(Picture::unchecked(data.to_vec()).pic_type(PictureType::CoverFront).mime_type(MimeType::from_str(mime)).build());
        file.save_to_path(&copy, WriteOptions::default())?;
        let after = std::fs::read(&copy)?;
        if audio_of(&after, kind).context("its audio could not be found after writing")? != before {
            bail!("writing the picture would have changed the audio");
        }
        if crate::serve::art::embedded(&copy).is_none() {
            bail!("the picture could not be read back");
        }
        Ok(())
    })();
    if let Err(e) = checked {
        std::fs::remove_file(&copy).ok();
        return Err(e);
    }
    std::fs::set_permissions(&copy, std::fs::metadata(track)?.permissions())?;
    std::fs::rename(&copy, track)?;
    Ok(())
}

/// A file's audio, without its tags: FLAC's frames after its metadata blocks; MP3's frames between a leading ID3v2
/// tag and trailing APEv2 and ID3v1 tags.
fn audio_of(b: &[u8], kind: Kind) -> Option<&[u8]> {
    match kind {
        Kind::Flac => {
            if b.get(..4)? != b"fLaC" {
                return None;
            }
            let mut at = 4;
            loop {
                let head = b.get(at..at + 4)?;
                let len = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
                at += 4 + len;
                if head[0] & 0x80 != 0 {
                    return b.get(at..);
                }
            }
        }
        Kind::Mp3 => {
            let mut start = 0;
            if b.get(..3)? == b"ID3" {
                let h = b.get(..10)?;
                let size = h[6..10].iter().fold(0usize, |s, &x| (s << 7) | (x & 0x7f) as usize);
                start = 10 + size + if h[5] & 0x10 != 0 { 10 } else { 0 };
            }
            let mut end = b.len();
            if end >= 128 && &b[end - 128..end - 125] == b"TAG" {
                end -= 128;
            }
            if end >= 32 && &b[end - 32..end - 24] == b"APETAGEX" {
                let size = u32::from_le_bytes(b[end - 20..end - 16].try_into().ok()?) as usize;
                let header = if b[end - 9] & 0x80 != 0 { 32 } else { 0 };
                end = end.checked_sub(size + header)?;
            }
            b.get(start..end)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-covers-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A second of a tone as a 16-bit stereo WAV.
    fn wav(path: &Path) {
        let rate = 44_100u32;
        let n = rate as usize;
        let mut data = Vec::with_capacity(n * 4);
        for i in 0..n {
            let s = ((i as f64 * 440.0 * std::f64::consts::TAU / rate as f64).sin() * 8000.0) as i16;
            data.extend_from_slice(&s.to_le_bytes());
            data.extend_from_slice(&s.to_le_bytes());
        }
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 4).to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(&data);
        std::fs::write(path, b).unwrap();
    }

    fn flac(dir: &Path, name: &str) -> PathBuf {
        let w = dir.join(format!("{name}.wav"));
        wav(&w);
        let f = dir.join(format!("{name}.flac"));
        crate::convert::to_cd_flac(&w, &f).unwrap();
        std::fs::remove_file(&w).unwrap();
        f
    }

    /// Twenty MPEG-1 Layer III frames (128 kb/s, 44.1 kHz, silence), with an ID3v1 tag at the end.
    fn mp3(path: &Path) {
        let mut b = Vec::new();
        for _ in 0..20 {
            b.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
            b.extend(std::iter::repeat_n(0u8, 417 - 4));
        }
        let mut v1 = vec![0u8; 128];
        v1[..3].copy_from_slice(b"TAG");
        v1[3..8].copy_from_slice(b"Title");
        b.extend_from_slice(&v1);
        std::fs::write(path, b).unwrap();
    }

    fn jpeg(path: &Path) -> Vec<u8> {
        let img = image::RgbImage::from_fn(64, 64, |x, y| image::Rgb([x as u8 * 4, y as u8 * 4, 128]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90).encode_image(&img).unwrap();
        std::fs::write(path, out.get_ref()).unwrap();
        out.into_inner()
    }

    /// Whether a FLAC holds a PICTURE metadata block (type 6).
    fn has_picture_block(b: &[u8]) -> bool {
        let mut at = 4;
        loop {
            let head = &b[at..at + 4];
            if head[0] & 0x7f == 6 {
                return true;
            }
            at += 4 + u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
            if head[0] & 0x80 != 0 {
                return false;
            }
        }
    }

    #[test]
    fn the_folders_cover_goes_inside_each_track_without_a_picture_the_audio_untouched() {
        let d = dir("inside");
        let one = flac(&d, "01");
        let two = d.join("02.mp3");
        mp3(&two);
        let cover = jpeg(&d.join("cover.jpg"));
        let audio = [audio_of(&std::fs::read(&one).unwrap(), Kind::Flac).unwrap().to_vec(), audio_of(&std::fs::read(&two).unwrap(), Kind::Mp3).unwrap().to_vec()];
        let done = complete(&d).unwrap();
        assert!(done.cover_file.is_none(), "the folder had one");
        assert_eq!(done.embedded.len(), 2, "{:?}", done.left);
        for (t, kind, a) in [(&one, Kind::Flac, &audio[0]), (&two, Kind::Mp3, &audio[1])] {
            let b = std::fs::read(t).unwrap();
            assert_eq!(audio_of(&b, kind).unwrap(), a.as_slice(), "the audio as it was");
            assert_eq!(crate::serve::art::embedded(t).unwrap().1, cover, "the folder's cover inside");
        }
        assert!(has_picture_block(&std::fs::read(&one).unwrap()), "FLAC's own picture block");
        assert!(std::fs::read(&two).unwrap().ends_with(&{
            let mut v1 = vec![0u8; 128];
            v1[..3].copy_from_slice(b"TAG");
            v1[3..8].copy_from_slice(b"Title");
            v1
        }), "the ID3v1 tag kept");
        assert!(!std::fs::read_dir(&d).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains(".cratarr-cover")), "no copy left");
        // Done again: nothing more to do.
        let again = complete(&d).unwrap();
        assert!(again.embedded.is_empty() && again.cover_file.is_none());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_folder_without_a_cover_file_gets_one_from_the_picture_inside_its_tracks() {
        let d = dir("file");
        let one = flac(&d, "01");
        let cover = jpeg(&d.join("cover.jpg"));
        complete(&d).unwrap();
        std::fs::remove_file(d.join("cover.jpg")).unwrap();
        let two = flac(&d, "02");
        let done = complete(&d).unwrap();
        assert_eq!(done.cover_file.as_deref(), Some(d.join("cover.jpg").as_path()));
        assert_eq!(std::fs::read(d.join("cover.jpg")).unwrap(), cover);
        assert_eq!(done.embedded, vec![two], "the track without one got it too");
        assert!(crate::serve::art::embedded(&one).is_some());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn without_a_cover_anywhere_nothing_changes_and_other_formats_are_left_as_they_are() {
        let d = dir("none");
        let one = flac(&d, "01");
        let before = std::fs::read(&one).unwrap();
        let done = complete(&d).unwrap();
        assert!(done.cover_file.is_none() && done.embedded.is_empty() && done.left.is_empty());
        assert_eq!(std::fs::read(&one).unwrap(), before);
        jpeg(&d.join("cover.jpg"));
        std::fs::write(d.join("03.ogg"), b"OggS not really").unwrap();
        let done = complete(&d).unwrap();
        assert_eq!(done.embedded, vec![one]);
        assert_eq!(done.left.len(), 1);
        assert!(done.left[0].1.contains("only FLAC and MP3"));
        std::fs::remove_dir_all(&d).ok();
    }
}
