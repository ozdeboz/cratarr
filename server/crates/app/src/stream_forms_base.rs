//! `stream_forms`' MP3 file, in base: a lossless track as an MP3 file with its tags and LAME's header, as an export
//! makes it (decision 0058), and the reading every form shares. `stream_forms` re-exports it and keeps the streams.
//!
//! Blocking; run on a blocking thread.

use std::io::Write;
use std::path::Path;

use anyhow::{Result, anyhow, bail};

use crate::convert;

/// The rate a form is made at: the track's own up to 48 kHz; a higher one brought down within its family.
fn rate_of(source: u32) -> u32 {
    match source {
        r if r <= 48_000 => r,
        r if r % 48_000 == 0 => 48_000,
        _ => convert::CD_RATE,
    }
}

/// A sample at 16 bits (a 16-bit source comes back exactly as it was).
pub(crate) fn to_i16(x: f32) -> i16 {
    (x * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16
}

/// The source, its rate out, the frame it starts at (where `from_ms` is) and its length out; `None` when it is not a
/// lossless file.
pub(crate) fn open(input: &Path, from_ms: u64) -> Result<Option<(convert::Source, u32, u64, Option<u64>)>> {
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
