//! Forms of a lossless track for browsers whose media source takes neither FLAC nor Opus in MP4 (0.9.8, the owner's
//! go: iOS's ManagedMediaSource refused both): ALAC in fragmented MP4, still lossless (Apple's encoder ported to Rust),
//! and MP3 (LAME, constant 320 kb/s), which every browser plays. Made as they are sent, from any moment (`from_ms`),
//! as the Opus stream is (`opus_copy::stream_mp4`); `head` hears the headers that place the stream before its first
//! byte. Read through the converter (decision 0013): 16 bits, at the track's own rate up to 48 kHz.
//!
//! Blocking; run on a blocking thread. Memory stays small whatever the track's length.

use std::io::Write;
use std::path::Path;

use anyhow::{Result, anyhow, bail};

use crate::convert;

/// ALAC's frames a packet (the encoder's own default).
const ALAC_PACKET: usize = alac_encoder::DEFAULT_FRAMES_PER_PACKET as usize;
/// Packets a fragment: about a second at 44.1 kHz.
const ALAC_FRAGMENT: usize = 11;

/// The headers a form's stream is placed by, as `/api/stream` sends them.
pub type Head = Vec<(String, String)>;

/// The rate a form is made at: the track's own up to 48 kHz; a higher one brought down within its family.
fn rate_of(source: u32) -> u32 {
    match source {
        r if r <= 48_000 => r,
        r if r % 48_000 == 0 => 48_000,
        _ => convert::CD_RATE,
    }
}

/// A sample at 16 bits (a 16-bit source comes back exactly as it was).
fn to_i16(x: f32) -> i16 {
    (x * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16
}

/// The source, its rate out, the frame it starts at (where `from_ms` is) and its length out; `None` when it is not a
/// lossless file.
fn open(input: &Path, from_ms: u64) -> Result<Option<(convert::Source, u32, u64, Option<u64>)>> {
    let Some(mut source) = convert::Source::open(input)? else { return Ok(None) };
    let format = source.format.clone();
    if format.channels == 0 {
        bail!("{} has no audio channels", input.display());
    }
    let rate = rate_of(format.sample_rate);
    let from = if from_ms > 0 { source.seek(from_ms * u64::from(format.sample_rate) / 1000)? } else { 0 };
    let start = convert::output_frames(from, format.sample_rate, rate);
    let total = format.frames.map(|f| convert::output_frames(f, format.sample_rate, rate));
    Ok(Some((source, rate, start, total)))
}

/// ALAC in fragmented MP4; `false` (nothing written) when it is not a lossless file.
pub fn alac_mp4(input: &Path, from_ms: u64, head: impl FnOnce(Head), mut out: impl Write) -> Result<bool> {
    let Some((mut source, rate, start, total)) = open(input, from_ms)? else { return Ok(false) };
    let (in_rate, in_channels) = (source.format.sample_rate, source.format.channels);
    let channels = in_channels.min(2);
    let alac = alac_encoder::FormatDescription::alac(f64::from(rate), ALAC_PACKET as u32, channels as u32);
    let pcm = alac_encoder::FormatDescription::pcm::<i16>(f64::from(rate), channels as u32);
    let mut encoder = alac_encoder::AlacEncoder::new(&alac);
    let mut hs: Head = vec![
        ("content-type".into(), "audio/mp4; codecs=\"alac\"".into()),
        ("x-sample-rate".into(), rate.to_string()),
        ("x-pre-skip".into(), "0".into()),
    ];
    if let Some(t) = total {
        hs.push(("x-total-samples".into(), t.to_string()));
    }
    head(hs);
    out.write_all(&crate::stream::init_segment(rate, &alac_entry(channels as u16, rate, &encoder.magic_cookie())))?;

    let mut packet: Vec<u8> = vec![0; alac.max_packet_size()];
    let mut interleaved: Vec<u8> = Vec::with_capacity(ALAC_PACKET * channels * 2);
    let (mut samples, mut data): (Vec<(u32, u32)>, Vec<u8>) = (Vec::new(), Vec::new());
    let (mut sequence, mut at) = (0u32, start);
    let mut flush = |samples: &mut Vec<(u32, u32)>, data: &mut Vec<u8>, out: &mut dyn Write| -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        sequence += 1;
        out.write_all(&crate::stream::fragment(sequence, at, samples, data))?;
        at += samples.iter().map(|s| u64::from(s.0)).sum::<u64>();
        samples.clear();
        data.clear();
        Ok(())
    };
    let mut encode = |interleaved: &mut Vec<u8>, samples: &mut Vec<(u32, u32)>, data: &mut Vec<u8>| {
        let frames = interleaved.len() / (channels * 2);
        let n = encoder.encode(&pcm, interleaved, &mut packet);
        data.extend_from_slice(&packet[..n]);
        samples.push((frames as u32, n as u32));
        interleaved.clear();
    };
    let mut sink = |planes: &[Vec<f32>], first: usize, len: usize| -> Result<()> {
        for i in first..first + len {
            for p in &planes[..channels] {
                interleaved.extend(to_i16(p[i]).to_ne_bytes());
            }
            if interleaved.len() == ALAC_PACKET * channels * 2 {
                encode(&mut interleaved, &mut samples, &mut data);
                if samples.len() == ALAC_FRAGMENT {
                    flush(&mut samples, &mut data, &mut out)?;
                }
            }
        }
        Ok(())
    };
    let mut scratch = Vec::new();
    convert::stream(|planes| source.read(planes, &mut scratch), in_channels, in_rate, rate, &mut sink)?;
    drop(sink);
    // The last packet, short.
    if !interleaved.is_empty() {
        encode(&mut interleaved, &mut samples, &mut data);
    }
    flush(&mut samples, &mut data, &mut out)?;
    out.flush()?;
    Ok(true)
}

/// The ALAC sample entry (Apple's "ALAC in ISO base media"): an AudioSampleEntry named `alac` holding an `alac` full
/// box with the encoder's magic cookie (ALACSpecificConfig).
fn alac_entry(channels: u16, rate: u32, cookie: &[u8]) -> Vec<u8> {
    let mut inner = Vec::with_capacity(12 + cookie.len());
    inner.extend(((12 + cookie.len()) as u32).to_be_bytes());
    inner.extend(b"alac");
    inner.extend(0u32.to_be_bytes()); // version and flags
    inner.extend_from_slice(cookie);
    let entry = crate::stream::audio_entry(channels, 16, rate, &inner);
    let mut b = Vec::with_capacity(8 + entry.len());
    b.extend(((8 + entry.len()) as u32).to_be_bytes());
    b.extend(b"alac");
    b.extend(entry);
    b
}

/// MP3 at a constant 320 kb/s (a moment is a place in the file); `false` (nothing written) when it is not a lossless file. No ID3 tag and no LAME
/// header: a stream from any moment is frames only. The headers: its rate and its whole length (`x-duration-ms`).
pub fn mp3(input: &Path, from_ms: u64, head: impl FnOnce(Head), mut out: impl Write) -> Result<bool> {
    use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, MonoPcm, Quality};
    let Some((mut source, rate, _, total)) = open(input, from_ms)? else { return Ok(false) };
    let (in_rate, in_channels) = (source.format.sample_rate, source.format.channels);
    let channels = in_channels.min(2);
    let mut b = Builder::new().ok_or_else(|| anyhow!("LAME could not be started"))?;
    b.set_num_channels(channels as u8).map_err(|e| anyhow!("LAME: {e:?}"))?;
    b.set_sample_rate(rate).map_err(|e| anyhow!("LAME: {e:?}"))?;
    b.set_brate(Bitrate::Kbps320).map_err(|e| anyhow!("LAME: {e:?}"))?;
    b.set_quality(Quality::NearBest).map_err(|e| anyhow!("LAME: {e:?}"))?;
    b.set_to_write_vbr_tag(false).map_err(|e| anyhow!("LAME: {e:?}"))?;
    let mut lame = b.build().map_err(|e| anyhow!("LAME: {e:?}"))?;
    let mut hs: Head = vec![
        ("content-type".into(), "audio/mpeg".into()),
        ("x-sample-rate".into(), rate.to_string()),
        // What a LAME header's encoder delay would say (the page trims by it as by one).
        ("x-pre-skip".into(), lame_delay(rate, channels).to_string()),
    ];
    if let Some(t) = total {
        hs.push(("x-duration-ms".into(), (t * 1000 / u64::from(rate)).to_string()));
    }
    head(hs);

    let mut encoded: Vec<u8> = Vec::new();
    let (mut left, mut right): (Vec<i16>, Vec<i16>) = (Vec::new(), Vec::new());
    let mut sink = |planes: &[Vec<f32>], first: usize, len: usize| -> Result<()> {
        left.clear();
        right.clear();
        left.extend(planes[0][first..first + len].iter().map(|x| to_i16(*x)));
        // LAME writes into the room made here (it takes no room as unlimited): as much as these samples can need.
        encoded.reserve(mp3lame_encoder::max_required_buffer_size(len));
        if channels == 2 {
            right.extend(planes[1][first..first + len].iter().map(|x| to_i16(*x)));
            lame.encode_to_vec(DualPcm { left: &left, right: &right }, &mut encoded).map_err(|e| anyhow!("LAME: {e:?}"))?;
        } else {
            lame.encode_to_vec(MonoPcm(&left), &mut encoded).map_err(|e| anyhow!("LAME: {e:?}"))?;
        }
        out.write_all(&encoded)?;
        encoded.clear();
        Ok(())
    };
    let mut scratch = Vec::new();
    convert::stream(|planes| source.read(planes, &mut scratch), in_channels, in_rate, rate, &mut sink)?;
    drop(sink);
    // The last frames: at most 7200 bytes (LAME's own bound).
    encoded.reserve(7200);
    lame.flush_to_vec::<FlushNoGap>(&mut encoded).map_err(|e| anyhow!("LAME: {e:?}"))?;
    out.write_all(&encoded)?;
    out.flush()?;
    Ok(true)
}

/// The MP3 files an export makes: a constant 320 kb/s, or LAME's variable rate at its best quality (V0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mp3Kind {
    Cbr320,
    V0,
}

/// A lossless track as an MP3 file, as players expect one: an ID3v2 tag with the source's tags and pictures (its
/// primary tag, through lofty's mapping), then LAME's header frame (Xing for V0, Info for 320: the length, the seek
/// table, the encoder delay and padding), then the frames. `false` (nothing written) when it is not a lossless file.
/// The header frame is known only once every frame is made: LAME leaves a frame's room for it at the start, and it is
/// written over that room at the end, as LAME's own command line does.
pub fn mp3_file(input: &Path, output: &Path, kind: Mp3Kind) -> Result<bool> {
    use std::io::{Seek, SeekFrom};

    use anyhow::Context;
    use lofty::tag::TagExt;
    use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushGap, MonoPcm, Quality, VbrMode};
    let Some((mut source, rate, _, _)) = open(input, 0)? else { return Ok(false) };
    let (in_rate, in_channels) = (source.format.sample_rate, source.format.channels);
    let channels = in_channels.min(2);
    let mut b = Builder::new().ok_or_else(|| anyhow!("LAME could not be started"))?;
    b.set_num_channels(channels as u8).map_err(|e| anyhow!("LAME: {e:?}"))?;
    b.set_sample_rate(rate).map_err(|e| anyhow!("LAME: {e:?}"))?;
    b.set_quality(Quality::NearBest).map_err(|e| anyhow!("LAME: {e:?}"))?;
    match kind {
        Mp3Kind::Cbr320 => b.set_brate(Bitrate::Kbps320).map_err(|e| anyhow!("LAME: {e:?}"))?,
        Mp3Kind::V0 => {
            b.set_vbr_mode(VbrMode::Mtrh).map_err(|e| anyhow!("LAME: {e:?}"))?;
            // V0: LAME's `-V 0` (its VBR quality 0, the new VBR code).
            b.set_vbr_quality(Quality::Best).map_err(|e| anyhow!("LAME: {e:?}"))?;
        }
    }
    b.set_to_write_vbr_tag(true).map_err(|e| anyhow!("LAME: {e:?}"))?;
    let mut lame = b.build().map_err(|e| anyhow!("LAME: {e:?}"))?;

    let file = std::fs::File::create(output).with_context(|| format!("creating {}", output.display()))?;
    let mut out = std::io::BufWriter::new(file);
    // The tag first; the frames start after it.
    let mut start = 0u64;
    if let Some(tag) = id3v2_of(input) {
        let mut bytes = Vec::new();
        match tag.dump_to(&mut bytes, lofty::config::WriteOptions::default()) {
            Ok(()) => {
                out.write_all(&bytes)?;
                start = bytes.len() as u64;
            }
            Err(e) => tracing::warn!("MP3: the tags of {} could not be written: {e}", input.display()),
        }
    }

    let mut encoded: Vec<u8> = Vec::new();
    let (mut left, mut right): (Vec<i16>, Vec<i16>) = (Vec::new(), Vec::new());
    let mut sink = |planes: &[Vec<f32>], first: usize, len: usize| -> Result<()> {
        left.clear();
        right.clear();
        left.extend(planes[0][first..first + len].iter().map(|x| to_i16(*x)));
        encoded.reserve(mp3lame_encoder::max_required_buffer_size(len));
        if channels == 2 {
            right.extend(planes[1][first..first + len].iter().map(|x| to_i16(*x)));
            lame.encode_to_vec(DualPcm { left: &left, right: &right }, &mut encoded).map_err(|e| anyhow!("LAME: {e:?}"))?;
        } else {
            lame.encode_to_vec(MonoPcm(&left), &mut encoded).map_err(|e| anyhow!("LAME: {e:?}"))?;
        }
        out.write_all(&encoded)?;
        encoded.clear();
        Ok(())
    };
    let mut scratch = Vec::new();
    convert::stream(|planes| source.read(planes, &mut scratch), in_channels, in_rate, rate, &mut sink)?;
    drop(sink);
    // The last frames (at most 7200 bytes), padded as a file ends.
    encoded.reserve(7200);
    lame.flush_to_vec::<FlushGap>(&mut encoded).map_err(|e| anyhow!("LAME: {e:?}"))?;
    out.write_all(&encoded)?;
    // The header frame, over the room left for it.
    let size = lame.lame_tag_size();
    if size > 0 {
        let mut frame = Vec::with_capacity(size);
        if lame.lame_tag_encode_to_vec(&mut frame).is_some() {
            out.seek(SeekFrom::Start(start))?;
            out.write_all(&frame)?;
        }
    }
    out.flush()?;
    Ok(true)
}

/// The source's tags as an ID3v2 tag: its primary tag (else its first), every item lofty can map and its pictures
/// (the cover inside); `None` when it has none.
fn id3v2_of(input: &Path) -> Option<lofty::id3::v2::Id3v2Tag> {
    use lofty::file::TaggedFileExt;
    use lofty::tag::TagExt;
    let options = lofty::config::ParseOptions::new().read_properties(false);
    let tagged = lofty::probe::Probe::open(input).and_then(|p| p.options(options).read()).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?.clone();
    let id3 = lofty::id3::v2::Id3v2Tag::from(tag);
    (!id3.is_empty()).then_some(id3)
}

/// LAME's encoder delay at these settings (samples), as its Info header would carry it: asked of LAME itself, set up
/// as the stream's encoder is (the crate keeps its own encoder's handle to itself).
fn lame_delay(rate: u32, channels: usize) -> i32 {
    use mp3lame_encoder::ffi;
    // SAFETY: a handle of LAME's own making, set up, asked and closed here; null is checked.
    unsafe {
        let gf = ffi::lame_init();
        if gf.is_null() {
            return 0;
        }
        ffi::lame_set_in_samplerate(gf, rate as _);
        ffi::lame_set_num_channels(gf, channels as _);
        ffi::lame_set_brate(gf, 320);
        ffi::lame_set_quality(gf, 2);
        let delay = if ffi::lame_init_params(gf) >= 0 { ffi::lame_get_encoder_delay(gf) } else { 0 };
        ffi::lame_close(gf);
        delay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-forms-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("a.wav");
        crate::opus_copy::tests::wav(&p, 44_100, 2.0);
        p
    }

    #[test]
    fn alac_comes_as_fragmented_mp4_with_its_placing() {
        let p = wav("alac");
        let mut said = None;
        let mut out = Vec::new();
        assert!(alac_mp4(&p, 0, |h| said = Some(h), &mut out).unwrap());
        let said = said.unwrap();
        assert!(said.contains(&("content-type".to_string(), "audio/mp4; codecs=\"alac\"".to_string())));
        assert!(said.contains(&("x-sample-rate".to_string(), "44100".to_string())));
        assert!(said.contains(&("x-total-samples".to_string(), "88200".to_string())));
        assert_eq!(&out[4..8], b"ftyp");
        assert!(out.windows(4).any(|w| w == b"alac"), "the ALAC sample entry");
        assert!(out.windows(4).any(|w| w == b"moof"), "fragments");
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }

    #[test]
    fn mp3_comes_as_frames_from_the_first_byte() {
        let p = wav("mp3");
        let mut said = None;
        let mut out = Vec::new();
        assert!(mp3(&p, 0, |h| said = Some(h), &mut out).unwrap());
        let said = said.unwrap();
        assert!(said.contains(&("x-duration-ms".to_string(), "2000".to_string())));
        let delay: i32 = said.iter().find(|(k, _)| k == "x-pre-skip").unwrap().1.parse().unwrap();
        assert!((500..1200).contains(&delay), "LAME's own encoder delay: {delay}");
        assert_eq!(out[0], 0xff, "a frame's sync, no tag before it");
        assert_eq!(out[1] & 0xe0, 0xe0);
        // About 320 kb/s for two seconds.
        assert!((70_000..95_000).contains(&out.len()), "{}", out.len());
        // From a moment on: shorter.
        let mut later = Vec::new();
        assert!(mp3(&p, 1000, |_| {}, &mut later).unwrap());
        assert!(later.len() < out.len() * 3 / 4, "{} of {}", later.len(), out.len());
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }

    #[test]
    fn an_mp3_file_has_its_tags_then_lames_header_frame() {
        use lofty::file::TaggedFileExt;
        use lofty::tag::{Accessor, TagExt};
        let p = wav("mp3-file");
        let mut tag = lofty::id3::v2::Id3v2Tag::new();
        tag.set_title("Tone".into());
        tag.set_artist("Sine".into());
        tag.save_to_path(&p, lofty::config::WriteOptions::default()).unwrap();
        for (kind, name, want) in [(Mp3Kind::Cbr320, "320.mp3", b"Info"), (Mp3Kind::V0, "v0.mp3", b"Xing")] {
            let out = p.with_file_name(name);
            assert!(mp3_file(&p, &out, kind).unwrap());
            let b = std::fs::read(&out).unwrap();
            assert_eq!(&b[..3], b"ID3", "{kind:?}: the tag first");
            // The tag's size: four 7-bit bytes, after its 10-byte header.
            let size = b[6..10].iter().fold(0usize, |n, x| (n << 7) | usize::from(*x & 0x7f)) + 10;
            assert_eq!((b[size], b[size + 1] & 0xe0), (0xff, 0xe0), "{kind:?}: a frame right after it");
            let first = &b[size..size + 200];
            assert!(first.windows(4).any(|w| w == want), "{kind:?}: its first frame is LAME's header");
            assert!(first.windows(4).any(|w| w == b"LAME"));
            let read = lofty::read_from_path(&out).unwrap();
            let t = read.primary_tag().unwrap();
            assert_eq!((t.title().as_deref(), t.artist().as_deref()), (Some("Tone"), Some("Sine")), "{kind:?}");
        }
        let (cbr, v0) = (std::fs::metadata(p.with_file_name("320.mp3")).unwrap().len(), std::fs::metadata(p.with_file_name("v0.mp3")).unwrap().len());
        assert!((70_000..95_000).contains(&cbr), "about 320 kb/s for two seconds: {cbr}");
        assert!(v0 < cbr, "a sine needs less than 320 kb/s at V0: {v0}");
        assert!(!mp3_file(&p.with_file_name("320.mp3"), &p.with_file_name("again.mp3"), Mp3Kind::V0).unwrap(), "not lossless: nothing");
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }
}
