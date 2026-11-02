//! Health's fixes for tracks whose sound ends before their length (0.9.14, the owner: "The different issues have
//! different causes and different solutions, and we should offer all"), one per track, applied as Health's other fixes
//! are (an administrator's tap, Undo after):
//! - **reread** (`files:length-misread`): its file gives its true length and Cratarr read another: read again;
//! - **length** (`files:length-guessed`): an MP3 that gives no length: an info frame written before its sound, giving
//!   its frames and bytes;
//! - **trim** (`files:cut-short`, `files:sound-breaks`): plays to where its sound ends, its length set to that: an
//!   MP3's info frame's counts (one written where it has none), a FLAC's STREAMINFO total. Replacing the track is the
//!   album's single-track replacement (`track_fetch`), which the finding's items lead to.
//!
//! A file is written in place (its folder's owner and group kept), as a run on the one path (`fix_plans`, decision
//! 0092): the run keeps its original first, and its Undo copies it back (`file_runs`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::fixes::Fix;
use super::health::{SHORT_SLACK_MS, clock, short_cause};
use crate::serve::Serve;

/// The fields these fixes have (`<field>:<song id>`); `seektable` a FLAC's broken seek table made padding (0.14).
pub const FIELDS: [&str; 4] = ["reread", "length", "trim", "seektable"];

fn field_of(key: &str) -> Option<&'static str> {
    match key {
        "files:length-misread" => Some("reread"),
        "files:length-guessed" => Some("length"),
        "files:cut-short" | "files:sound-breaks" => Some("trim"),
        "files:bad_seektable" => Some("seektable"),
        _ => None,
    }
}

/// A finding's fixes, one per track it lists. Blocking.
pub fn of_finding(key: &str) -> Result<Vec<Fix>> {
    let Some(field) = field_of(key) else { return Ok(Vec::new()) };
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let mut out = Vec::new();
    if field == "seektable" {
        for (folder, rel) in served.catalogue().with_issue("bad_seektable")? {
            let Some((song, ..)) = served.library.song_at(folder, &rel)? else { continue };
            if let Some(f) = fix(&served, field, &song)? {
                out.push(f);
            }
        }
        return Ok(out);
    }
    for (folder, rel, path, _, heard) in served.catalogue().shorter_than_heard(SHORT_SLACK_MS)? {
        if short_cause(&path, heard) != key {
            continue;
        }
        let Some((song, ..)) = served.library.song_at(folder, &rel)? else { continue };
        if let Some(f) = fix(&served, field, &song)? {
            out.push(f);
        }
    }
    Ok(out)
}

/// The file a song is.
fn file_of(served: &Serve, song: &str) -> Result<Option<(i64, String, PathBuf, String, String)>> {
    let Some((folder, rel, album, title)) = served.library.song_file(song)? else { return Ok(None) };
    let dir = usize::try_from(folder).ok().and_then(|i| served.catalogue().folders().get(i).cloned()).context("its folder is not in use")?;
    Ok(Some((folder, rel.clone(), dir.join(&rel), album, title)))
}

fn is_flac(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("flac"))
}

/// An MP3's sound as its frames run: its length (ms). None where its frames cannot be followed.
fn walked_ms(path: &Path) -> Option<i64> {
    let info = crate::stream::mp3_info(path)?;
    let (_, frames, _) = crate::stream::mp3_walk(path)?;
    (frames > 0).then(|| (frames * info.samples * 1000 / info.rate) as i64)
}

/// One fix (`field` for `song`), as it stands now; none where it does not apply. Blocking.
pub fn fix(served: &Serve, field: &str, song: &str) -> Result<Option<Fix>> {
    let Some((folder, rel, path, album, title)) = file_of(served, song)? else { return Ok(None) };
    if field == "seektable" {
        let Some(_) = crate::catalogue::tags::flac_seektable(&path).and_then(|t| t.wrong()) else { return Ok(None) };
        return Ok(Some(Fix {
            id: format!("seektable:{song}"),
            album,
            field: "seektable",
            from: Some("A broken seek table".into()),
            to: "Seeks by its frames".into(),
            source: "its file".into(),
            certainty: "certain",
            why: format!("{title}: its seek table sends a far seek to the wrong place, so the track ends early: the table taken out (its audio untouched)."),
            writes: Vec::new(),
            candidates: Vec::new(),
        }));
    }
    let given = served.catalogue().duration_of(folder, &rel).map(|d| d as i64);
    let flac = is_flac(&path);
    let (field, to, source, why): (&'static str, Option<i64>, &str, &str) = match field {
        "reread" => ("reread", crate::stream::mp3_info(&path).and_then(|i| i.length_ms()).map(|m| m as i64), "its file's own header", "Cratarr read its length wrong: read again from its file."),
        "length" => ("length", walked_ms(&path), "its sound", "Its file gives no length: written into it, in an info frame before its sound."),
        "trim" if flac => ("trim", served.catalogue().heard_ms(folder, &rel), "its sound", "Plays to where its sound breaks off: its length set to that."),
        "trim" => ("trim", walked_ms(&path), "its sound", "Plays to where its sound ends: its length set to that."),
        _ => return Ok(None),
    };
    let Some(to) = to else { return Ok(None) };
    Ok(Some(Fix {
        id: format!("{field}:{song}"),
        album,
        field,
        from: given.map(clock),
        to: clock(to),
        source: source.into(),
        certainty: "certain",
        why: format!("{title}: {why}"),
        writes: Vec::new(),
        candidates: Vec::new(),
    }))
}

/// Writes a length's change into a file in place (`op`: length, trim, seektable; `ms` a FLAC's length to set): the run on
/// the one path keeps its original first (`file_runs`). Blocking.
pub fn write(op: &str, path: &Path, ms: Option<i64>) -> Result<()> {
    match op {
        "seektable" => crate::catalogue::tags::flac_drop_seektable(path),
        "length" | "trim" if is_flac(path) => set_flac_total(path, ms.context("its sound's length is not known")?),
        "length" | "trim" => set_mp3_length(path),
        "reread" => Ok(()),
        _ => bail!("no such change: {op}"),
    }
}

/// A length's fix as a file of a run: its track, the length now → after (a seek table's broken → by its frames), and
/// what is done to it, its print as it stands. None where it no longer applies. Blocking.
pub fn file_write(served: &Serve, fix: &Fix) -> Result<Option<crate::journal::FileWrite>> {
    let song = fix.id.split_once(':').map(|(_, s)| s).context("not a fix")?;
    let Some((folder, rel, path, album, title)) = file_of(served, song)? else { return Ok(None) };
    let ms = (fix.field == "trim" && is_flac(&path)).then(|| served.catalogue().heard_ms(folder, &rel)).flatten();
    let field = if fix.field == "seektable" { "seek table" } else { "length" };
    Ok(Some(crate::journal::FileWrite {
        album,
        track: Some(title),
        before: vec![(field.to_string(), fix.from.clone().into_iter().collect())],
        after: vec![(field.to_string(), vec![fix.to.clone()])],
        from: vec![(field.to_string(), fix.source.clone())],
        undone: false,
        whole: Some(crate::journal::Whole { op: fix.field.to_string(), ms, kept: None, planned: super::file_runs::print(&path)?, wrote: None }),
        path,
    }))
}

/// An MP3's info frame set to the frames and bytes its sound has; one written before its sound where it has none.
fn set_mp3_length(path: &Path) -> Result<()> {
    use std::io::{Seek, SeekFrom, Write};
    let info = crate::stream::mp3_info(path).context("its frames cannot be read")?;
    let (_, frames, end) = crate::stream::mp3_walk(path).context("its frames cannot be followed")?;
    if frames == 0 {
        bail!("it has no whole frame");
    }
    let frames32 = u32::try_from(frames)?;
    if info.has_info {
        let at = info.frames_at.context("its info frame has no frame count to set")?;
        let mut f = std::fs::OpenOptions::new().write(true).open(path)?;
        f.seek(SeekFrom::Start(at))?;
        f.write_all(&frames32.to_be_bytes())?;
        if let Some(b) = info.bytes_at {
            f.seek(SeekFrom::Start(b))?;
            f.write_all(&u32::try_from(end - info.start)?.to_be_bytes())?;
        }
        f.flush()?;
        return Ok(());
    }
    // None: an info frame like its first sound frame (without a CRC), silent but for its Xing tag.
    let mut h = info.header;
    h[1] |= 1;
    let len = usize::try_from(info.first_len)?;
    let mpeg1 = (h[1] >> 3) & 3 == 3;
    let mono = h[3] >> 6 == 3;
    let side = match (mpeg1, mono) {
        (true, true) => 17,
        (true, false) => 32,
        (false, true) => 9,
        (false, false) => 17,
    };
    let mut frame = vec![0u8; len];
    frame[..4].copy_from_slice(&h);
    let x = 4 + side;
    if x + 16 > len {
        bail!("its frames are too small for an info frame");
    }
    frame[x..x + 4].copy_from_slice(b"Xing");
    frame[x + 4..x + 8].copy_from_slice(&3u32.to_be_bytes());
    frame[x + 8..x + 12].copy_from_slice(&frames32.to_be_bytes());
    frame[x + 12..x + 16].copy_from_slice(&u32::try_from(end - info.start + len as u64)?.to_be_bytes());
    let old = std::fs::read(path)?;
    let start = usize::try_from(info.start)?;
    let mut new = Vec::with_capacity(old.len() + len);
    new.extend_from_slice(&old[..start]);
    new.extend_from_slice(&frame);
    new.extend_from_slice(&old[start..]);
    // In place (its owner, group and rights kept), not a new file renamed over it.
    std::fs::write(path, new)?;
    Ok(())
}

/// A FLAC's STREAMINFO total set to the samples `ms` of sound hold at its rate (its MD5 left as it is).
fn set_flac_total(path: &Path, ms: i64) -> Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let mut f = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
    let mut head = [0u8; 26];
    f.read_exact(&mut head)?;
    if &head[..4] != b"fLaC" || head[4] & 0x7f != 0 {
        bail!("not a FLAC that starts with its STREAMINFO");
    }
    let v = u64::from_be_bytes(head[18..26].try_into()?);
    let rate = (v >> 44) & 0xF_FFFF;
    if rate == 0 {
        bail!("its STREAMINFO gives no sample rate");
    }
    let total = u64::try_from(ms)? * rate / 1000;
    let v = (v & !0xF_FFFF_FFFF) | (total & 0xF_FFFF_FFFF);
    f.seek(SeekFrom::Start(18))?;
    f.write_all(&v.to_be_bytes())?;
    f.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str, bytes: &[u8]) -> PathBuf {
        let p = std::env::temp_dir().join(format!("cratarr-lenfix-{}-{name}", std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// CBR MPEG-1 Layer III at 128 kb/s, 44.1 kHz: `n` silent frames (417 bytes), a 10-byte ID3v2 tag before.
    fn mp3(n: usize, info: bool) -> Vec<u8> {
        let mut b = b"ID3\x04\x00\x00\x00\x00\x00\x00".to_vec();
        let frame = || {
            let mut f = vec![0xFF, 0xFB, 0x90, 0x64];
            f.resize(417, 0);
            f
        };
        if info {
            let mut f = frame();
            f[36..40].copy_from_slice(b"Info");
            f[40..44].copy_from_slice(&3u32.to_be_bytes());
            // It tells of 1000 frames and 417,000 bytes: more than it holds.
            f[44..48].copy_from_slice(&1000u32.to_be_bytes());
            f[48..52].copy_from_slice(&417_000u32.to_be_bytes());
            b.extend_from_slice(&f);
        }
        for _ in 0..n {
            b.extend_from_slice(&frame());
        }
        b
    }

    #[test]
    fn a_cut_mp3_is_set_to_the_frames_it_holds() {
        let p = temp("cut.mp3", &mp3(200, true));
        set_mp3_length(&p).unwrap();
        let info = crate::stream::mp3_info(&p).unwrap();
        assert_eq!(info.frames, Some(200));
        assert_eq!(info.bytes, Some(201 * 417));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn an_mp3_without_a_length_gets_an_info_frame() {
        let p = temp("bare.mp3", &mp3(300, false));
        set_mp3_length(&p).unwrap();
        let info = crate::stream::mp3_info(&p).unwrap();
        assert!(info.has_info);
        assert_eq!(info.frames, Some(300));
        assert_eq!(info.length_ms(), Some(300 * 1152 * 1000 / 44_100));
        // Its sound frames follow it, all of them.
        assert_eq!(crate::stream::mp3_walk(&p).unwrap().1, 300);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn a_flac_total_is_set_to_its_sound() {
        // fLaC, STREAMINFO's header, then its 34 bytes: 44.1 kHz, 2 channels, 16 bits, 1,000,000 samples.
        let mut b = b"fLaC\x00\x00\x00\x22".to_vec();
        let mut si = [0u8; 34];
        let v: u64 = (44_100u64 << 44) | (1 << 41) | (15 << 36) | 1_000_000;
        si[10..18].copy_from_slice(&v.to_be_bytes());
        b.extend_from_slice(&si);
        let p = temp("t.flac", &b);
        set_flac_total(&p, 10_000).unwrap();
        let out = std::fs::read(&p).unwrap();
        let v = u64::from_be_bytes(out[18..26].try_into().unwrap());
        assert_eq!(v & 0xF_FFFF_FFFF, 441_000);
        assert_eq!(v >> 36, (44_100u64 << 8) | (1 << 5) | 15, "rate, channels and bits as they were");
        let _ = std::fs::remove_file(&p);
    }
}
