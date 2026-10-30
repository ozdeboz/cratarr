//! Smaller copies for the app's downloads (decision 0033, step 4): a lossless track as Ogg Opus, made as it is sent.
//! Decoding and resampling are the converter's (decision 0013); the frames are libopus's, the reference encoder; the
//! Ogg pages (RFC 3533, RFC 7845) are written here. Lossy files are never made into copies: they are small already,
//! and a second lossy pass only loses more.
//!
//! Memory is bounded and small, whatever the length of the track: audio streams through a few thousand frames at a
//! time and each Ogg page goes out as soon as it is full.
//!
//! The album's cover goes inside the copy too (the owner, 2026-10-19: a cover inside every file that leaves Cratarr),
//! as Ogg's comments carry one; the comment header then spans pages.
//!
//! Blocking; run on a blocking thread.

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use lofty::file::TaggedFileExt;
use lofty::tag::Accessor;

use crate::convert;

/// The app's copies' bit rate: transparent for most music to most ears, about a sixth of a CD FLAC.
pub const BITRATE: i32 = 160_000;
/// Opus always runs at 48 kHz.
const RATE: u32 = 48_000;
/// 20 ms frames, the encoder's best for music.
const FRAME: usize = 960;
/// An Opus packet is at most this long.
const MAX_PACKET: usize = 4000;
/// Audio packets per Ogg page: about a second.
const PAGE_PACKETS: usize = 50;
const VENDOR: &str = concat!("Cratarr ", env!("CARGO_PKG_VERSION"), " (libopus)");

/// At most this many copies or conversions are made at once (the app's downloads, an export's): each keeps a core busy.
pub static COPIES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// Writes `input` to `out` as Ogg Opus, with its ReplayGain (track and album gain in dB, decision 0035) as Opus's
/// R128 gain tags when known. `false` (nothing written) when it is not a lossless file.
pub fn copy(input: &Path, out: impl Write, gains: (Option<f64>, Option<f64>)) -> Result<bool> {
    copy_at(input, out, gains, BITRATE)
}

/// [`copy`] at another bit rate (bits a second): an export's (decision 0058).
pub fn copy_at(input: &Path, out: impl Write, gains: (Option<f64>, Option<f64>), bitrate: i32) -> Result<bool> {
    let Some(mut source) = convert::Source::open(input)? else { return Ok(false) };
    let format = source.format.clone();
    // Opus's simple channel mapping holds one or two; a surround file keeps its front pair.
    let channels = format.channels.min(2);
    if channels == 0 {
        bail!("{} has no audio channels", input.display());
    }
    let mut encoder = Encoder::new(channels, bitrate)?;
    let pre_skip = encoder.lookahead()?;
    let mut ogg = Ogg::new(out, serial(input));
    ogg.page(&[&opus_head(channels, pre_skip, format.sample_rate)], 0, BOS)?;
    ogg.packet(&opus_tags(input, gains), 0)?;

    // Interleaved samples waiting for a whole frame; the frames' packets waiting for their page.
    let mut pcm: Vec<f32> = Vec::with_capacity(FRAME * channels * 2);
    let mut packets: Vec<Vec<u8>> = Vec::with_capacity(PAGE_PACKETS);
    let mut encoded = 0u64;
    let mut sink = |planes: &[Vec<f32>], start: usize, len: usize| -> Result<()> {
        for i in start..start + len {
            for p in &planes[..channels] {
                pcm.push(p[i]);
            }
        }
        while pcm.len() >= FRAME * channels {
            packets.push(encoder.encode(&pcm[..FRAME * channels])?);
            pcm.drain(..FRAME * channels);
            encoded += FRAME as u64;
            if packets.len() == PAGE_PACKETS {
                let refs: Vec<&[u8]> = packets.iter().map(Vec::as_slice).collect();
                ogg.page(&refs, pre_skip as u64 + encoded, 0)?;
                packets.clear();
            }
        }
        Ok(())
    };
    let mut scratch = Vec::new();
    let frames = convert::stream(|planes| source.read(planes, &mut scratch), format.channels, format.sample_rate, RATE, &mut sink)?;
    drop(sink);

    // The encoder's delay still holds the music's last samples: silence after it lets them out. The last frame
    // padded too; the last page's position says where the music ends, so a player trims the padding (RFC 7845 §4).
    drain_encoder(&mut encoder, &mut pcm, channels, pre_skip, &mut packets)?;
    let length = convert::output_frames(frames, format.sample_rate, RATE);
    let refs: Vec<&[u8]> = packets.iter().map(Vec::as_slice).collect();
    ogg.page(&refs, pre_skip as u64 + length, EOS)?;
    ogg.out.flush()?;
    Ok(true)
}

/// How a phone's stream places itself (headers before its audio): where its first packet starts (48 kHz frames from
/// the track's start), the encoder's delay to drop before it, and the track's whole length at 48 kHz when known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamHead {
    pub start: u64,
    pub pre_skip: u16,
    pub total: Option<u64>,
}

/// A lossless track as Opus in fragmented MP4, for a phone's stream (the owner, 2026-10-24: Opus on phones, about a
/// sixth of FLAC's data): an init segment, then fragments of about a second, made as they are sent. `from_ms` starts at
/// the packet holding that moment, as the FLAC stream does (`stream::FlacToMp4`). `head` hears the placing before the
/// first byte goes out. `lossy_too`: a lossy file the browser cannot take as it is (AAC, Ogg Vorbis) is made one too.
/// `false` (nothing written) when it is not a file it makes one of.
///
/// The fragments' times are the encoder's own, so its delay comes first; the page drops it (its offset), as it drops an
/// MP3's. Opus-in-ISOBMFF would declare it in `dOps` instead, but whether a browser then trims it is the browser's to
/// say: declared as none, nothing is trimmed twice.
pub fn stream_mp4(input: &Path, from_ms: u64, lossy_too: bool, head: impl FnOnce(StreamHead), mut out: impl Write) -> Result<bool> {
    let opened = if lossy_too { convert::Source::open_any(input)? } else { convert::Source::open(input)? };
    let Some(mut source) = opened else { return Ok(false) };
    let format = source.format.clone();
    let channels = format.channels.min(2);
    if channels == 0 {
        bail!("{} has no audio channels", input.display());
    }
    let from = if from_ms > 0 { source.seek(from_ms * u64::from(format.sample_rate) / 1000)? } else { 0 };
    let mut encoder = Encoder::new(channels, BITRATE)?;
    let pre_skip = encoder.lookahead()?;
    let start = convert::output_frames(from, format.sample_rate, RATE);
    let total = format.frames.map(|f| convert::output_frames(f, format.sample_rate, RATE));
    head(StreamHead { start, pre_skip, total });
    out.write_all(&crate::stream::init_segment(RATE, &opus_entry(channels as u16)))?;

    let mut pcm: Vec<f32> = Vec::with_capacity(FRAME * channels * 2);
    let mut packets: Vec<Vec<u8>> = Vec::with_capacity(PAGE_PACKETS);
    let mut sequence = 0u32;
    let mut at = start;
    let mut sink = |planes: &[Vec<f32>], first: usize, len: usize| -> Result<()> {
        for i in first..first + len {
            for p in &planes[..channels] {
                pcm.push(p[i]);
            }
        }
        while pcm.len() >= FRAME * channels {
            packets.push(encoder.encode(&pcm[..FRAME * channels])?);
            pcm.drain(..FRAME * channels);
            if packets.len() == PAGE_PACKETS {
                fragment_out(&mut out, &mut sequence, &mut at, &mut packets)?;
            }
        }
        Ok(())
    };
    let mut scratch = Vec::new();
    convert::stream(|planes| source.read(planes, &mut scratch), format.channels, format.sample_rate, RATE, &mut sink)?;
    drop(sink);
    // The encoder's delay still holds the music's last samples: silence after it lets them out, the last frame padded;
    // the page ends the track at its length (appendWindowEnd).
    drain_encoder(&mut encoder, &mut pcm, channels, pre_skip, &mut packets)?;
    if !packets.is_empty() {
        fragment_out(&mut out, &mut sequence, &mut at, &mut packets)?;
    }
    out.flush()?;
    Ok(true)
}

/// The end of a track: `pre_skip` samples of silence after the music, so the encoder lets out the last of it (its
/// delay held them back: a stream ended 6.5 ms short), then the last frame padded.
fn drain_encoder(encoder: &mut Encoder, pcm: &mut Vec<f32>, channels: usize, pre_skip: u16, packets: &mut Vec<Vec<u8>>) -> Result<()> {
    pcm.resize(pcm.len() + usize::from(pre_skip) * channels, 0.0);
    pcm.resize(pcm.len().div_ceil(FRAME * channels) * FRAME * channels, 0.0);
    for frame in pcm.chunks(FRAME * channels) {
        packets.push(encoder.encode(frame)?);
    }
    pcm.clear();
    Ok(())
}

/// `packets` (20 ms each) out as one fragment starting at `at`.
fn fragment_out(out: &mut impl Write, sequence: &mut u32, at: &mut u64, packets: &mut Vec<Vec<u8>>) -> Result<()> {
    *sequence += 1;
    let samples: Vec<(u32, u32)> = packets.iter().map(|p| (FRAME as u32, p.len() as u32)).collect();
    out.write_all(&crate::stream::fragment(*sequence, *at, &samples, &packets.concat()))?;
    *at += (FRAME * packets.len()) as u64;
    packets.clear();
    Ok(())
}

/// The Opus sample entry (Opus in ISOBMFF, §4.3): an AudioSampleEntry named `Opus` holding `dOps`, the identification
/// header's fields (big-endian here), its pre-skip declared as none (see [`stream_mp4`]).
fn opus_entry(channels: u16) -> Vec<u8> {
    let mut dops = vec![0u8, channels as u8];
    dops.extend(0u16.to_be_bytes()); // pre-skip: the page's
    // The original rate is for information only (RFC 7845 §5.1); Chrome refuses a stream whose dOps rate is not the
    // sample entry's 48 kHz.
    dops.extend(RATE.to_be_bytes());
    dops.extend(0i16.to_be_bytes()); // output gain
    dops.push(0); // channel mapping family 0: mono or stereo
    let mut b = Vec::with_capacity(8 + dops.len());
    b.extend(((8 + dops.len()) as u32).to_be_bytes());
    b.extend(b"dOps");
    b.extend(dops);
    let entry = crate::stream::audio_entry(channels, 16, RATE, &b);
    let mut opus = Vec::with_capacity(8 + entry.len());
    opus.extend(((8 + entry.len()) as u32).to_be_bytes());
    opus.extend(b"Opus");
    opus.extend(entry);
    opus
}

/// A stream serial number, the same for the same file (copies of a track are alike).
fn serial(input: &Path) -> u32 {
    crc32(input.to_string_lossy().as_bytes())
}

/// The identification header (RFC 7845 §5.1).
fn opus_head(channels: usize, pre_skip: u16, input_rate: u32) -> Vec<u8> {
    let mut b = Vec::with_capacity(19);
    b.extend_from_slice(b"OpusHead");
    b.push(1);
    b.push(channels as u8);
    b.extend_from_slice(&pre_skip.to_le_bytes());
    b.extend_from_slice(&input_rate.to_le_bytes());
    b.extend_from_slice(&0i16.to_le_bytes());
    b.push(0);
    b
}

/// Opus's R128 gain tag from a ReplayGain gain: Q7.8 dB from EBU R128's −23 LUFS, not ReplayGain's −18 (RFC 7845 §5.2.1).
fn r128(replay_gain_db: f64) -> String {
    (((replay_gain_db - 5.0) * 256.0).round() as i32).clamp(i16::MIN as i32, i16::MAX as i32).to_string()
}

/// The comment header (RFC 7845 §5.2): the track's title, artist, album, album artist, number and year, so the copy
/// names itself wherever it ends up, and its gains, so a player levels it offline too.
fn opus_tags(input: &Path, (track, album): (Option<f64>, Option<f64>)) -> Vec<u8> {
    let mut comments: Vec<String> = Vec::new();
    if let Some(g) = track {
        comments.push(format!("R128_TRACK_GAIN={}", r128(g)));
    }
    if let Some(g) = album {
        comments.push(format!("R128_ALBUM_GAIN={}", r128(g)));
    }
    let options = lofty::config::ParseOptions::new().read_properties(false);
    if let Ok(tagged) = lofty::probe::Probe::open(input).and_then(|p| p.options(options).read())
        && let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag())
    {
        let mut add = |key: &str, value: Option<String>| {
            if let Some(v) = value.filter(|v| !v.is_empty()) {
                comments.push(format!("{key}={v}"));
            }
        };
        add("TITLE", tag.title().map(|v| v.to_string()));
        add("ARTIST", tag.artist().map(|v| v.to_string()));
        add("ALBUM", tag.album().map(|v| v.to_string()));
        add("ALBUMARTIST", tag.get_string(lofty::tag::ItemKey::AlbumArtist).map(str::to_string));
        add("TRACKNUMBER", tag.track().map(|n| n.to_string()));
        add("DISCNUMBER", tag.disk().map(|n| n.to_string()));
        add("DATE", tag.date().map(|d| d.to_string()));
    }
    if let Some(cover) = cover_comment(input) {
        comments.push(cover);
    }
    let mut b = Vec::new();
    b.extend_from_slice(b"OpusTags");
    b.extend_from_slice(&(VENDOR.len() as u32).to_le_bytes());
    b.extend_from_slice(VENDOR.as_bytes());
    b.extend_from_slice(&(comments.len() as u32).to_le_bytes());
    for c in comments {
        b.extend_from_slice(&(c.len() as u32).to_le_bytes());
        b.extend_from_slice(c.as_bytes());
    }
    b
}

/// A cover larger than this goes into a copy as a smaller picture ([`COVER_SIDE`] on its longer side), so the copy stays
/// small: a cover is a few hundred kB, a minute of the copy about 1.2 MB.
const COVER_MAX: usize = 300 * 1024;
const COVER_SIDE: u32 = 1000;

/// The album's cover as Ogg's comments carry one (`METADATA_BLOCK_PICTURE`: a FLAC picture block in base64, the
/// front cover): the folder's cover file, else the picture inside the track, as Cratarr shows covers (decision 0030).
/// `None` when the album has none.
fn cover_comment(input: &Path) -> Option<String> {
    use base64::Engine;
    let (mut mime, mut data) = match input.parent().and_then(crate::serve::art::cover_file) {
        Some(file) => {
            let data = std::fs::read(file).ok()?;
            (crate::serve::art::sniff(&data).to_string(), data)
        }
        None => crate::serve::art::embedded(input)?,
    };
    if data.len() > COVER_MAX
        && let Ok(Some(small)) = crate::serve::art::resize(&data, COVER_SIDE)
    {
        (mime, data) = ("image/jpeg".to_string(), small);
    }
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(&data)).with_guessed_format().ok()?.into_dimensions().ok()?;
    let mut block = Vec::with_capacity(32 + mime.len() + data.len());
    block.extend_from_slice(&3u32.to_be_bytes());
    block.extend_from_slice(&(mime.len() as u32).to_be_bytes());
    block.extend_from_slice(mime.as_bytes());
    // No description; its size, 24 bits a pixel, no palette.
    block.extend_from_slice(&0u32.to_be_bytes());
    for v in [width, height, 24, 0] {
        block.extend_from_slice(&v.to_be_bytes());
    }
    block.extend_from_slice(&(data.len() as u32).to_be_bytes());
    block.extend_from_slice(&data);
    Some(format!("METADATA_BLOCK_PICTURE={}", base64::engine::general_purpose::STANDARD.encode(block)))
}

/// libopus's encoder, for music at a bit rate.
struct Encoder {
    raw: *mut opusic_sys::OpusEncoder,
    channels: usize,
    out: Vec<u8>,
}

impl Encoder {
    fn new(channels: usize, bitrate: i32) -> Result<Self> {
        let mut error = 0;
        // SAFETY: plain constructor; the result is checked before use and freed in `drop`.
        let raw = unsafe { opusic_sys::opus_encoder_create(RATE as i32, channels as i32, opusic_sys::OPUS_APPLICATION_AUDIO, &mut error) };
        if raw.is_null() || error != opusic_sys::OPUS_OK {
            bail!("the Opus encoder did not start (error {error})");
        }
        let me = Self { raw, channels, out: vec![0; MAX_PACKET] };
        me.set(opusic_sys::OPUS_SET_BITRATE_REQUEST, bitrate)?;
        me.set(opusic_sys::OPUS_SET_SIGNAL_REQUEST, opusic_sys::OPUS_SIGNAL_MUSIC)?;
        me.set(opusic_sys::OPUS_SET_COMPLEXITY_REQUEST, 10)?;
        Ok(me)
    }

    fn set(&self, request: i32, value: i32) -> Result<()> {
        // SAFETY: a SET request takes one opus_int32.
        let r = unsafe { opusic_sys::opus_encoder_ctl(self.raw, request, value) };
        if r != opusic_sys::OPUS_OK {
            bail!("the Opus encoder refused setting {request} (error {r})");
        }
        Ok(())
    }

    /// How many samples the encoder delays the music by: the stream's pre-skip.
    fn lookahead(&self) -> Result<u16> {
        let mut v: i32 = 0;
        // SAFETY: a GET request takes a pointer to one opus_int32.
        let r = unsafe { opusic_sys::opus_encoder_ctl(self.raw, opusic_sys::OPUS_GET_LOOKAHEAD_REQUEST, &mut v as *mut i32) };
        if r != opusic_sys::OPUS_OK {
            bail!("the Opus encoder did not tell its lookahead (error {r})");
        }
        u16::try_from(v).context("an Opus lookahead out of range")
    }

    /// One frame of interleaved samples into one packet.
    fn encode(&mut self, frame: &[f32]) -> Result<Vec<u8>> {
        debug_assert_eq!(frame.len(), FRAME * self.channels);
        // SAFETY: `frame` holds FRAME samples per channel, `out` MAX_PACKET bytes, as the call is told.
        let n = unsafe {
            opusic_sys::opus_encode_float(self.raw, frame.as_ptr(), FRAME as i32, self.out.as_mut_ptr(), MAX_PACKET as i32)
        };
        if n < 0 {
            bail!("the Opus encoder failed (error {n})");
        }
        Ok(self.out[..n as usize].to_vec())
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: created by opus_encoder_create, freed once.
        unsafe { opusic_sys::opus_encoder_destroy(self.raw) };
    }
}

const BOS: u8 = 0x02;
const EOS: u8 = 0x04;
/// A page going on with the packet the page before it began (RFC 3533).
const CONTINUED: u8 = 0x01;

/// One logical Ogg stream, written a page at a time.
struct Ogg<W: Write> {
    out: W,
    serial: u32,
    sequence: u32,
}

impl<W: Write> Ogg<W> {
    fn new(out: W, serial: u32) -> Self {
        Self { out, serial, sequence: 0 }
    }

    /// A page of whole packets, `granule` the position at the end of its last one.
    fn page(&mut self, packets: &[&[u8]], granule: u64, flags: u8) -> Result<()> {
        let mut lacing = Vec::new();
        for p in packets {
            lacing.extend(std::iter::repeat_n(255u8, p.len() / 255));
            lacing.push((p.len() % 255) as u8);
        }
        if lacing.len() > 255 {
            bail!("an Ogg page of {} segments", lacing.len());
        }
        self.raw(&lacing, packets, granule, flags)
    }

    /// One packet over as many pages as it needs (the comment header with a cover): each page as full as a page can
    /// be (255 segments), those after the first marked as going on with it (RFC 3533); `granule` on each (0: headers,
    /// RFC 7845 §3).
    fn packet(&mut self, p: &[u8], granule: u64) -> Result<()> {
        let mut lacing = vec![255u8; p.len() / 255];
        lacing.push((p.len() % 255) as u8);
        let mut at = 0;
        for (i, chunk) in lacing.chunks(255).enumerate() {
            let len: usize = chunk.iter().map(|&l| l as usize).sum();
            self.raw(chunk, &[&p[at..at + len]], granule, if i == 0 { 0 } else { CONTINUED })?;
            at += len;
        }
        Ok(())
    }

    /// A page as its segments say: `bodies` laid end to end are what `lacing` counts.
    fn raw(&mut self, lacing: &[u8], bodies: &[&[u8]], granule: u64, flags: u8) -> Result<()> {
        let mut page = Vec::with_capacity(27 + lacing.len() + bodies.iter().map(|p| p.len()).sum::<usize>());
        page.extend_from_slice(b"OggS");
        page.push(0);
        page.push(flags);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&self.serial.to_le_bytes());
        page.extend_from_slice(&self.sequence.to_le_bytes());
        page.extend_from_slice(&[0; 4]);
        page.push(lacing.len() as u8);
        page.extend_from_slice(lacing);
        for p in bodies {
            page.extend_from_slice(p);
        }
        let crc = crc32(&page);
        page[22..26].copy_from_slice(&crc.to_le_bytes());
        self.out.write_all(&page)?;
        self.sequence += 1;
        Ok(())
    }
}

/// Ogg's CRC-32: polynomial 0x04C11DB7, not reflected, starting at 0.
pub(crate) fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut r = (i as u32) << 24;
            for _ in 0..8 {
                r = if r & 0x8000_0000 != 0 { (r << 1) ^ 0x04C1_1DB7 } else { r << 1 };
            }
            *e = r;
        }
        t
    });
    data.iter().fold(0u32, |crc, &b| (crc << 8) ^ table[((crc >> 24) as u8 ^ b) as usize])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A 16-bit stereo WAV of a 440 Hz sine, `secs` long.
    pub(crate) fn wav(path: &Path, rate: u32, secs: f64) {
        let n = (f64::from(rate) * secs) as u32;
        let data_len = n * 4;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 4).to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..n {
            let v = (0.5 * (2.0 * std::f64::consts::PI * 440.0 * f64::from(i) / f64::from(rate)).sin() * 32767.0) as i16;
            b.extend_from_slice(&v.to_le_bytes());
            b.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    /// The pages of an Ogg stream: (flags, granule, sequence, packets), each page's CRC checked.
    fn pages(mut b: &[u8]) -> Vec<(u8, u64, u32, Vec<Vec<u8>>)> {
        let mut out = Vec::new();
        while !b.is_empty() {
            assert_eq!(&b[..4], b"OggS");
            let n = b[26] as usize;
            let lacing = &b[27..27 + n];
            let body: usize = lacing.iter().map(|&l| l as usize).sum();
            let len = 27 + n + body;
            let mut page = b[..len].to_vec();
            let crc = u32::from_le_bytes(page[22..26].try_into().unwrap());
            page[22..26].copy_from_slice(&[0; 4]);
            assert_eq!(crc32(&page), crc, "page CRC");
            let mut packets = Vec::new();
            let (mut at, mut cur) = (27 + n, Vec::new());
            for &l in lacing {
                cur.extend_from_slice(&b[at..at + l as usize]);
                at += l as usize;
                if l < 255 {
                    packets.push(std::mem::take(&mut cur));
                }
            }
            // A packet going on on the next page: its part on this one.
            if !cur.is_empty() {
                packets.push(cur);
            }
            out.push((b[5], u64::from_le_bytes(b[6..14].try_into().unwrap()), u32::from_le_bytes(b[18..22].try_into().unwrap()), packets));
            b = &b[len..];
        }
        out
    }

    /// A fragmented MP4's top-level boxes, and per fragment its start (`tfdt`) and its samples' durations (`trun`).
    fn fragments(b: &[u8]) -> (Vec<[u8; 4]>, Vec<(u64, Vec<u32>)>) {
        let (mut kinds, mut out, mut i) = (Vec::new(), Vec::new(), 0);
        while i + 8 <= b.len() {
            let len = u32::from_be_bytes(b[i..i + 4].try_into().unwrap()) as usize;
            let kind: [u8; 4] = b[i + 4..i + 8].try_into().unwrap();
            if &kind == b"moof" {
                let moof = &b[i..i + len];
                let at = |name: &[u8]| moof.windows(4).position(|w| w == name).unwrap() + 4;
                let t = at(b"tfdt");
                let base = u64::from_be_bytes(moof[t + 4..t + 12].try_into().unwrap());
                let r = at(b"trun");
                let n = u32::from_be_bytes(moof[r + 4..r + 8].try_into().unwrap()) as usize;
                let durations = (0..n).map(|k| u32::from_be_bytes(moof[r + 12 + 8 * k..r + 16 + 8 * k].try_into().unwrap())).collect();
                out.push((base, durations));
            }
            kinds.push(kind);
            i += len;
        }
        (kinds, out)
    }

    /// Not a check: which files the converter takes as lossless and which it streams (`CRATARR_FORMATS="a.m4a,b.ogg"`),
    /// each stream written beside it (`<file>.mp4`, its head in `<file>.mp4.json`). Run with `--ignored`.
    #[test]
    #[ignore]
    fn formats_for_the_lab() {
        for input in std::env::var("CRATARR_FORMATS").unwrap_or_default().split(',').filter(|p| !p.is_empty()) {
            let lossless = convert::Source::open(Path::new(input)).unwrap().is_some();
            let mut head = None;
            let mut out = Vec::new();
            let streamed = stream_mp4(Path::new(input), 0, true, |h| head = Some(h), &mut out).unwrap();
            println!("{input}: lossless {lossless}, streamed {streamed}, head {head:?}, {} bytes", out.len());
            if let Some(h) = head {
                std::fs::write(format!("{input}.mp4"), &out).unwrap();
                std::fs::write(format!("{input}.mp4.json"), format!("{{\"start\":{},\"pre_skip\":{},\"total\":{}}}", h.start, h.pre_skip, h.total.unwrap_or(0))).unwrap();
            }
        }
    }

    /// Not a check: a phone's stream of real files for a browser to try (`CRATARR_OPUS="in.flac:out.mp4:from_ms,..."`),
    /// each with its head beside it (`out.mp4.json`). Run with `--ignored`.
    #[test]
    #[ignore]
    fn phone_streams_for_the_lab() {
        let pairs = std::env::var("CRATARR_OPUS").unwrap_or_default();
        for spec in pairs.split(',').filter(|p| !p.is_empty()) {
            let mut parts = spec.split(':');
            let (input, output) = (parts.next().unwrap(), parts.next().unwrap());
            let from: u64 = parts.next().map_or(0, |f| f.parse().unwrap());
            let mut head = None;
            let mut out = Vec::new();
            assert!(stream_mp4(Path::new(input), from, true, |h| head = Some(h), &mut out).unwrap());
            let h = head.unwrap();
            std::fs::write(output, out).unwrap();
            std::fs::write(format!("{output}.json"), format!("{{\"start\":{},\"pre_skip\":{},\"total\":{}}}", h.start, h.pre_skip, h.total.unwrap_or(0))).unwrap();
        }
    }

    #[test]
    fn a_phones_stream_is_opus_in_fragments_of_a_second_placed_by_its_head() {
        let dir = std::env::temp_dir().join(format!("cratarr-opus-mp4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("a.wav");
        wav(&input, 44_100, 3.0);
        let mut heard = None;
        let mut out = Vec::new();
        assert!(stream_mp4(&input, 0, false, |h| heard = Some(h), &mut out).unwrap());
        let head = heard.unwrap();
        assert_eq!((head.start, head.total), (0, Some(144_000)), "3 s at 48 kHz");
        assert!(head.pre_skip > 0, "the encoder's delay is told");
        let (kinds, frags) = fragments(&out);
        assert_eq!(&kinds[..2], &[*b"ftyp", *b"moov"]);
        let moov = &out[..];
        assert!(moov.windows(4).any(|w| w == b"Opus") && moov.windows(4).any(|w| w == b"dOps"), "an Opus sample entry");
        assert_eq!(frags.iter().map(|f| f.0).collect::<Vec<_>>(), [0, 48_000, 96_000, 144_000], "a second each, back to back, then the encoder's delay let out");
        assert!(frags.iter().all(|f| f.1.iter().all(|&d| d == 960)), "20 ms packets");
        assert_eq!(frags.iter().map(|f| f.1.len()).sum::<usize>(), 151, "the whole track: 3 s of 20 ms packets, and one that lets the encoder's delay out");

        // From 1.5 s: the packet holding that moment, no later, and the fragments go on from there.
        let mut heard = None;
        let mut out = Vec::new();
        assert!(stream_mp4(&input, 1_500, false, |h| heard = Some(h), &mut out).unwrap());
        let head = heard.unwrap();
        assert!(head.start <= 72_000 && head.start > 60_000, "starts at {}", head.start);
        let (_, frags) = fragments(&out);
        assert_eq!(frags[0].0, head.start);
        assert_eq!(frags[1].0, head.start + 48_000);

        // A lossy file is not made into a stream: nothing written, nothing told.
        let mp3 = dir.join("b.mp3");
        std::fs::write(&mp3, [0xFF, 0xFB, 0x90, 0x00]).unwrap();
        let mut out = Vec::new();
        assert!(!stream_mp4(&mp3, 0, false, |_| panic!("told"), &mut out).unwrap_or(false));
        assert!(out.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_crc_is_oggs() {
        assert_eq!(crc32(b""), 0);
        // CRC-32/CKSUM's check value (0x765E7680) without its final inversion: Ogg's parameters.
        assert_eq!(crc32(b"123456789"), 0x89A1_897F);
    }

    #[test]
    fn a_lossless_track_becomes_a_well_formed_ogg_opus_copy_of_the_same_length() {
        let dir = std::env::temp_dir().join(format!("cratarr-opus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("a.wav");
        wav(&input, 44_100, 3.0);
        let mut out = Vec::new();
        assert!(copy(&input, &mut out, (Some(-7.5), None)).unwrap());
        let pages = pages(&out);
        let (flags, granule, seq, head) = &pages[0];
        assert_eq!((*flags, *granule, *seq), (BOS, 0, 0));
        assert_eq!(&head[0][..8], b"OpusHead");
        assert_eq!(head[0][9], 2, "stereo");
        let pre_skip = u64::from(u16::from_le_bytes([head[0][10], head[0][11]]));
        assert_eq!(u32::from_le_bytes(head[0][12..16].try_into().unwrap()), 44_100, "the original rate, for information");
        assert_eq!(&pages[1].3[0][..8], b"OpusTags");
        // ReplayGain's -7.5 dB from -18 LUFS is R128's -12.5 dB from -23: -3200 in Q7.8.
        assert!(pages[1].3[0].windows(21).any(|w| w == b"R128_TRACK_GAIN=-3200"));
        let last = pages.last().unwrap();
        assert_eq!(last.0, EOS);
        assert_eq!(last.1, pre_skip + 3 * 48_000, "ends exactly where the music does");
        assert!(pages.windows(2).all(|w| w[1].2 == w[0].2 + 1), "pages in sequence");
        let audio: usize = pages[2..].iter().map(|p| p.3.len()).sum();
        assert_eq!(audio, (3 * 48_000 + pre_skip as usize).div_ceil(FRAME), "one packet per 20 ms, the encoder's delay let out after the music");
        // About 160 kb/s: 3 s near 60 kB, far below the 529 kB of the WAV.
        assert!(out.len() > 30_000 && out.len() < 90_000, "{} bytes", out.len());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_albums_cover_goes_inside_the_copy_over_as_many_pages_as_it_needs() {
        use base64::Engine;
        let dir = std::env::temp_dir().join(format!("cratarr-opus-cover-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("a.wav");
        wav(&input, 44_100, 1.0);
        // A large cover (noise does not compress): it goes in as a 1000 px JPEG.
        let mut seed = 1u32;
        let img = image::RgbImage::from_fn(1200, 1200, |_, _| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            image::Rgb([(seed >> 24) as u8, (seed >> 16) as u8, (seed >> 8) as u8])
        });
        img.save(dir.join("cover.png")).unwrap();
        let mut out = Vec::new();
        assert!(copy(&input, &mut out, (None, None)).unwrap());
        let pages = pages(&out);
        // The comment header: page 1 and those going on with it.
        let mut tags = pages[1].3.concat();
        let mut next = 2;
        while pages[next].0 & CONTINUED != 0 {
            assert_eq!(pages[next].1, 0, "a header page");
            tags.extend(pages[next].3.concat());
            next += 1;
        }
        assert!(next > 3, "the comment header spans pages ({next})");
        assert_eq!(&tags[..8], b"OpusTags");
        let key = b"METADATA_BLOCK_PICTURE=";
        let at = tags.windows(key.len()).position(|w| w == key).expect("a picture") + key.len();
        let len = u32::from_le_bytes(tags[at - key.len() - 4..at - key.len()].try_into().unwrap()) as usize - key.len();
        let block = base64::engine::general_purpose::STANDARD.decode(&tags[at..at + len]).unwrap();
        let word = |i: usize| u32::from_be_bytes(block[i..i + 4].try_into().unwrap()) as usize;
        assert_eq!(word(0), 3, "the front cover");
        let mime_len = word(4);
        assert_eq!(&block[8..8 + mime_len], b"image/jpeg");
        let after = 8 + mime_len + 4 + word(8 + mime_len);
        assert_eq!((word(after), word(after + 4)), (1000, 1000), "made smaller");
        let data = &block[after + 20..];
        assert_eq!(data.len(), word(after + 16));
        assert_eq!(&data[..3], &[0xFF, 0xD8, 0xFF], "a JPEG");
        // The music after it, as without a cover.
        let pre_skip = usize::from(u16::from_le_bytes([pages[0].3[0][10], pages[0].3[0][11]]));
        let audio: usize = pages[next..].iter().map(|p| p.3.len()).sum();
        assert_eq!(audio, (48_000 + pre_skip).div_ceil(FRAME));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_long_packet_spans_full_pages_and_the_rest() {
        let mut out = Vec::new();
        let mut ogg = Ogg::new(&mut out, 7);
        let p: Vec<u8> = (0..255 * 255 + 300).map(|i| i as u8).collect();
        ogg.packet(&p, 0).unwrap();
        let pages = pages(&out);
        assert_eq!(pages.len(), 2);
        assert_eq!((pages[0].0, pages[1].0), (0, CONTINUED));
        assert_eq!(pages[0].3[0].len(), 255 * 255, "a full page of it");
        assert_eq!(pages[1].3[0].len(), 300, "the packet's end on the second");
    }

    /// A copy of a real file, to listen to or check with another decoder: CRATARR_OPUS_IN to CRATARR_OPUS_OUT.
    #[test]
    #[ignore]
    fn copy_a_file() {
        let (Some(input), Some(output)) = (std::env::var_os("CRATARR_OPUS_IN"), std::env::var_os("CRATARR_OPUS_OUT")) else { return };
        let out = std::io::BufWriter::new(std::fs::File::create(output).unwrap());
        assert!(copy(Path::new(&input), out, (None, None)).unwrap(), "not a lossless file");
    }

    #[test]
    fn a_lossy_track_is_not_copied() {
        let dir = std::env::temp_dir().join(format!("cratarr-opus-mp3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("a.mp3");
        // An MPEG-1 Layer III frame header and some padding: not lossless, whatever follows.
        let mut b = vec![0xFF, 0xFB, 0x90, 0x64];
        b.resize(4096, 0);
        std::fs::write(&input, b).unwrap();
        let mut out = Vec::new();
        assert!(!copy(&input, &mut out, (None, None)).unwrap_or(false));
        assert!(out.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
