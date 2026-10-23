//! Writing tags into a track without touching its audio (decision 0053's way, shared by the covers inside the files
//! and the tempo): the track is written to a copy first, the copy's audio compared byte for byte with the original's,
//! and only then put in the original's place. FLAC and MP3 only (the library's formats).

use std::path::Path;

use anyhow::{Context, Result, bail};
use lofty::tag::{ItemKey, Tag};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    Flac,
    Mp3,
}

/// Rewrites a track's tag with `edit` (its main tag, made where it has none), through a copy whose audio must be the
/// original's; `check` looks at the written copy before it takes the original's place.
pub fn rewrite(track: &Path, edit: impl FnOnce(&mut Tag), check: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    use lofty::config::{ParseOptions, WriteOptions};
    use lofty::prelude::*;
    let kind = match track.extension().and_then(|x| x.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("flac") => Kind::Flac,
        Some("mp3") => Kind::Mp3,
        _ => bail!("only FLAC and MP3 tracks are written to"),
    };
    let original = std::fs::read(track).with_context(|| format!("reading {}", track.display()))?;
    let before = audio_of(&original, kind).context("its audio could not be found")?;
    // The copy keeps the track's extension: the tag reader knows a file's format by it. Hidden: no walk takes it for a
    // track.
    let stem = track.file_stem().and_then(|n| n.to_str()).unwrap_or("track");
    let ext = track.extension().and_then(|x| x.to_str()).unwrap_or("");
    let copy = track.with_file_name(format!(".{stem}.cratarr-tags.{ext}"));
    std::fs::write(&copy, &original)?;
    let checked = (|| -> Result<()> {
        let mut file = lofty::probe::Probe::open(&copy)?.options(ParseOptions::new().read_properties(false)).read()?;
        if file.primary_tag_mut().is_none() {
            let kind = file.primary_tag_type();
            file.insert_tag(Tag::new(kind));
        }
        edit(file.primary_tag_mut().context("no tag to write to")?);
        file.save_to_path(&copy, WriteOptions::default())?;
        let after = std::fs::read(&copy)?;
        if audio_of(&after, kind).context("its audio could not be found after writing")? != before {
            bail!("writing the tag would have changed the audio");
        }
        check(&copy)
    })();
    if let Err(e) = checked {
        std::fs::remove_file(&copy).ok();
        return Err(e);
    }
    std::fs::set_permissions(&copy, std::fs::metadata(track)?.permissions())?;
    std::fs::rename(&copy, track)?;
    Ok(())
}

/// A track's BPM tag as written (none: it has none).
pub fn bpm(track: &Path) -> Option<String> {
    let (tags, _) = crate::catalogue::tags::read(track).ok()?;
    tags.items.into_iter().find(|(_, name, _)| *name == Some("bpm")).map(|(_, _, v)| v)
}

/// Sets a track's BPM tag to `to` (none: removes it), a whole number as BPM tags are: ID3v2's TBPM (lofty's whole-number
/// key), Vorbis's BPM.
pub fn set_bpm(track: &Path, to: Option<u32>) -> Result<()> {
    rewrite(
        track,
        |tag| {
            tag.remove_key(ItemKey::Bpm);
            tag.remove_key(ItemKey::IntegerBpm);
            if let Some(b) = to {
                let key = if tag.tag_type() == lofty::tag::TagType::Id3v2 { ItemKey::IntegerBpm } else { ItemKey::Bpm };
                tag.insert_text(key, b.to_string());
            }
        },
        |copy| {
            let got = bpm(copy).and_then(|v| v.trim().parse::<f64>().ok()).map(|b| b.round() as u32);
            if got != to {
                bail!("the BPM read back as {got:?}, not {to:?}");
            }
            Ok(())
        },
    )
}

/// A file's audio, without its tags: FLAC's frames after its metadata blocks; MP3's frames between a leading ID3v2
/// tag and trailing APEv2 and ID3v1 tags.
pub(crate) fn audio_of(b: &[u8], kind: Kind) -> Option<&[u8]> {
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

/// Small real tracks to write to, for this module's tests and the covers'.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::{Path, PathBuf};

    pub fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A second of a tone as a 16-bit stereo WAV.
    pub fn wav(path: &Path) {
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

    pub fn flac(dir: &Path, name: &str) -> PathBuf {
        let w = dir.join(format!("{name}.wav"));
        wav(&w);
        let f = dir.join(format!("{name}.flac"));
        crate::convert::to_cd_flac(&w, &f).unwrap();
        std::fs::remove_file(&w).unwrap();
        f
    }

    /// Twenty MPEG-1 Layer III frames (128 kb/s, 44.1 kHz, silence), with an ID3v1 tag at the end.
    pub fn mp3(path: &Path) {
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
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn a_bpm_is_written_read_back_and_removed_the_audio_untouched() {
        let d = dir("retag-bpm");
        let f = flac(&d, "one");
        let m = d.join("two.mp3");
        mp3(&m);
        for t in [&f, &m] {
            let before = std::fs::read(t).unwrap();
            set_bpm(t, Some(170)).unwrap();
            assert_eq!(bpm(t).as_deref(), Some("170"), "{}", t.display());
            set_bpm(t, Some(85)).unwrap();
            assert_eq!(bpm(t).as_deref(), Some("85"), "halved, one BPM tag: {}", t.display());
            set_bpm(t, None).unwrap();
            assert_eq!(bpm(t), None, "{}", t.display());
            let kind = if t.extension().unwrap() == "flac" { Kind::Flac } else { Kind::Mp3 };
            assert_eq!(audio_of(&std::fs::read(t).unwrap(), kind), audio_of(&before, kind), "the audio as it was");
        }
        assert!(std::fs::read_dir(&d).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with('.')), "no copy left");
        std::fs::remove_dir_all(&d).ok();
    }
}

