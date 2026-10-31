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
        // Only what `edit` changed is written (0.13): `edit` sees the format's own tag as lofty's generic one, and
        // the fields of each key it changed are replaced in the format's own tag; every other field stays as it was.
        // Through the generic tag alone, every field lofty has no key for was dropped (Vorbis STYLE,
        // MUSICBRAINZ_ALBUMTYPE, RELEASESTATUS…), and a key's second spelling folded into its first (ORIGINALYEAR into
        // ORIGINALDATE).
        let options = ParseOptions::new().read_properties(false);
        match kind {
            Kind::Flac => {
                let mut file = lofty::flac::FlacFile::read_from(&mut std::fs::File::open(&copy)?, options)?;
                let own = file.remove_vorbis_comments().unwrap_or_default();
                let (_, before) = own.clone().split_tag();
                let mut after = before.clone();
                edit(&mut after);
                file.set_vorbis_comments(vorbis_edited(own, &before, &after)?);
                file.save_to_path(&copy, WriteOptions::default())?;
            }
            Kind::Mp3 => {
                let mut file = lofty::mpeg::MpegFile::read_from(&mut std::fs::File::open(&copy)?, options)?;
                let own = file.remove_id3v2().unwrap_or_default();
                let (_, before) = own.clone().split_tag();
                let mut after = before.clone();
                edit(&mut after);
                file.set_id3v2(id3_edited(own, &before, &after));
                file.save_to_path(&copy, WriteOptions::default())?;
            }
        }
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

/// The keys an edit changed: each whose values differ before and after.
fn changed_keys(before: &Tag, after: &Tag) -> Vec<ItemKey> {
    let values = |t: &Tag, k: ItemKey| t.items().filter(|i| i.key() == k).map(|i| i.value().clone()).collect::<Vec<_>>();
    let mut out: Vec<ItemKey> = Vec::new();
    for k in before.items().chain(after.items()).map(|i| i.key()) {
        if !out.contains(&k) && values(before, k) != values(after, k) {
            out.push(k);
        }
    }
    out
}

/// A FLAC's own comments with what an edit changed: every field of a changed key goes, however it is spelled, and the
/// edited values come in under the key's own name; the pictures replaced where they changed.
fn vorbis_edited(mut own: lofty::ogg::VorbisComments, before: &Tag, after: &Tag) -> Result<lofty::ogg::VorbisComments> {
    use lofty::ogg::OggPictureStorage;
    use lofty::tag::{ItemValue, TagType};
    let changed = changed_keys(before, after);
    let gone: Vec<String> = own
        .items()
        .map(|(k, _)| k)
        .filter(|k| ItemKey::from_key(TagType::VorbisComments, k).is_some_and(|i| changed.contains(&i)))
        .map(str::to_string)
        .collect();
    for k in gone {
        own.remove(&k).for_each(drop);
    }
    for item in after.items().filter(|i| changed.contains(&i.key())) {
        if let (Some(k), ItemValue::Text(v)) = (item.key().map_key(TagType::VorbisComments), item.value()) {
            own.push(k.to_string(), v.clone());
        }
    }
    if before.pictures() != after.pictures() {
        own.remove_pictures();
        for p in after.pictures() {
            own.insert_picture(p.clone(), None)?;
        }
    }
    Ok(own)
}

/// An MP3's own ID3v2 tag with what an edit changed: every frame of a changed key goes (the pictures where they
/// changed), and the edited values come in as lofty makes their frames; every other frame stays as it was.
fn id3_edited(mut own: lofty::id3::v2::Id3v2Tag, before: &Tag, after: &Tag) -> lofty::id3::v2::Id3v2Tag {
    use lofty::id3::v2::Id3v2Tag;
    use lofty::prelude::*;
    use lofty::tag::TagType;
    let changed = changed_keys(before, after);
    let pictures = before.pictures() != after.pictures();
    let of_changed = |f: &lofty::id3::v2::Frame<'static>| {
        if f.id().as_str() == "APIC" {
            return pictures;
        }
        let mut one = Id3v2Tag::new();
        one.insert(f.clone());
        one.split_tag().1.items().any(|i| changed.contains(&i.key()))
    };
    own.retain(|f| !of_changed(f));
    let mut only = Tag::new(TagType::Id3v2);
    for item in after.items().filter(|i| changed.contains(&i.key())) {
        only.push_unchecked(item.clone());
    }
    if pictures {
        for p in after.pictures() {
            only.push_picture(p.clone());
        }
    }
    for f in Id3v2Tag::from(only) {
        own.insert(f);
    }
    own
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

    #[test]
    fn fields_lofty_has_no_key_for_are_kept() {
        use lofty::config::{ParseOptions, WriteOptions};
        use lofty::prelude::*;
        let d = dir("retag-keep");
        let f = flac(&d, "one");
        let read = |f: &Path| lofty::flac::FlacFile::read_from(&mut std::fs::File::open(f).unwrap(), ParseOptions::new()).unwrap();
        let mut file = read(&f);
        let mut vc = file.remove_vorbis_comments().unwrap_or_default();
        vc.push("STYLE".into(), "Dub Techno".into());
        vc.push("MUSICBRAINZ_ALBUMTYPE".into(), "album".into());
        vc.push("RELEASESTATUS".into(), "official".into());
        vc.push("ARTIST".into(), "Ott".into());
        vc.push("ORIGINALDATE".into(), "2010-05-03".into());
        vc.push("ORIGINALYEAR".into(), "2010".into());
        file.set_vorbis_comments(vc);
        file.save_to_path(&f, WriteOptions::default()).unwrap();
        set_bpm(&f, Some(120)).unwrap();
        let vc = read(&f).remove_vorbis_comments().unwrap();
        for (k, v) in [("STYLE", "Dub Techno"), ("MUSICBRAINZ_ALBUMTYPE", "album"), ("RELEASESTATUS", "official"), ("ARTIST", "Ott"), ("BPM", "120")] {
            assert_eq!(vc.get(k), Some(v), "{k}");
        }
        // A key's two spellings stay apart, each as it was.
        assert_eq!(vc.get_all("ORIGINALDATE").collect::<Vec<_>>(), ["2010-05-03"]);
        assert_eq!(vc.get_all("ORIGINALYEAR").collect::<Vec<_>>(), ["2010"]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_mp3_s_frames_lofty_does_not_map_are_kept() {
        use lofty::config::{ParseOptions, WriteOptions};
        use lofty::prelude::*;
        let d = dir("retag-keep-mp3");
        let m = d.join("two.mp3");
        mp3(&m);
        let read = |m: &Path| lofty::mpeg::MpegFile::read_from(&mut std::fs::File::open(m).unwrap(), ParseOptions::new()).unwrap();
        let mut file = read(&m);
        let mut tag = file.remove_id3v2().unwrap_or_default();
        tag.insert_user_text("Fancy Field".into(), "kept".into());
        tag.insert_user_text("MusicBrainz Album Status".into(), "official".into());
        tag.set_artist("Ott".into());
        file.set_id3v2(tag);
        file.save_to_path(&m, WriteOptions::default()).unwrap();
        set_bpm(&m, Some(120)).unwrap();
        let tag = read(&m).remove_id3v2().unwrap();
        assert_eq!(tag.get_user_text("Fancy Field"), Some("kept"));
        assert_eq!(tag.get_user_text("MusicBrainz Album Status"), Some("official"));
        assert_eq!(tag.artist().as_deref(), Some("Ott"));
        assert_eq!(bpm(&m).as_deref(), Some("120"));
        std::fs::remove_dir_all(&d).ok();
    }
}

