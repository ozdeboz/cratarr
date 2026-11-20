//! `opus_copy`'s file encoder, in base: a lossless track as an Ogg Opus file (decision 0033, step 4), as an export makes
//! its copies (decision 0058) and the app's downloads do. `opus_copy` re-exports it and keeps the phone's stream.
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
pub const RATE: u32 = 48_000;

/// 20 ms frames, the encoder's best for music.
pub const FRAME: usize = 960;

/// An Opus packet is at most this long.
const MAX_PACKET: usize = 4000;

/// Audio packets per Ogg page: about a second.
pub const PAGE_PACKETS: usize = 50;

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

/// The end of a track: `pre_skip` samples of silence after the music, so the encoder lets out the last of it (its
/// delay held them back: a stream ended 6.5 ms short), then the last frame padded.
pub fn drain_encoder(encoder: &mut Encoder, pcm: &mut Vec<f32>, channels: usize, pre_skip: u16, packets: &mut Vec<Vec<u8>>) -> Result<()> {
    pcm.resize(pcm.len() + usize::from(pre_skip) * channels, 0.0);
    pcm.resize(pcm.len().div_ceil(FRAME * channels) * FRAME * channels, 0.0);
    for frame in pcm.chunks(FRAME * channels) {
        packets.push(encoder.encode(frame)?);
    }
    pcm.clear();
    Ok(())
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
pub struct Encoder {
    raw: *mut opusic_sys::OpusEncoder,
    channels: usize,
    out: Vec<u8>,
}

impl Encoder {
    pub fn new(channels: usize, bitrate: i32) -> Result<Self> {
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
    pub fn lookahead(&self) -> Result<u16> {
        let mut v: i32 = 0;
        // SAFETY: a GET request takes a pointer to one opus_int32.
        let r = unsafe { opusic_sys::opus_encoder_ctl(self.raw, opusic_sys::OPUS_GET_LOOKAHEAD_REQUEST, &mut v as *mut i32) };
        if r != opusic_sys::OPUS_OK {
            bail!("the Opus encoder did not tell its lookahead (error {r})");
        }
        u16::try_from(v).context("an Opus lookahead out of range")
    }

    /// One frame of interleaved samples into one packet.
    pub fn encode(&mut self, frame: &[f32]) -> Result<Vec<u8>> {
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

pub const BOS: u8 = 0x02;

pub const EOS: u8 = 0x04;

/// A page going on with the packet the page before it began (RFC 3533).
pub const CONTINUED: u8 = 0x01;

/// One logical Ogg stream, written a page at a time.
pub struct Ogg<W: Write> {
    out: W,
    serial: u32,
    sequence: u32,
}

impl<W: Write> Ogg<W> {
    pub fn new(out: W, serial: u32) -> Self {
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
    pub fn packet(&mut self, p: &[u8], granule: u64) -> Result<()> {
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

/// Ogg's CRC-32 has one home, below the app, with the served files' ETags (`serve::opus_copy_crc`).
pub use crate::serve::opus_copy_crc::crc32;

#[cfg(any(test, feature = "test-support"))]
pub mod tests {
    // Shared with the tests of the modules and crates above (test-support): idle where no test calls it.
    #![cfg_attr(not(test), allow(dead_code, unused_imports))]
    use std::path::Path;

    /// A 16-bit stereo WAV of a 440 Hz sine, `secs` long.
    pub fn wav(path: &Path, rate: u32, secs: f64) {
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
}
