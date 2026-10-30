//! Hi-res audio to CD format: 16-bit FLAC at 44.1 kHz (decision 0013). Pure Rust, so the image stays small: decode
//! (symphonia), resample (rubato), TPDF dither to 16 bit, encode (flacenc). Tags and pictures are copied (lofty).
//!
//! Memory is bounded and small, whatever the length of the track: audio streams through in chunks of a few thousand
//! frames and the FLAC frames go straight to disk. Only the source's tags and pictures are held whole.
//!
//! Blocking file operations; run on a blocking thread.
//!
//! The shape follows decision 0011: the decisions (what needs converting, to what rate, how long the output is, how
//! a sample is quantised) are pure functions; `to_cd_flac` is the shell that reads and writes around them.


use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use flacenc::component::{BitRepr, StreamInfo};
use flacenc::error::{Verified, Verify};
use flacenc::source::{Context as FlacContext, Fill, FrameBuf};
use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::flac::FlacFile;
use lofty::ogg::OggPictureStorage;
use lofty::ogg::tag::VorbisComments;
use lofty::probe::Probe;
use lofty::tag::{Tag, TagExt};
use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Indexing, Resampler, WindowFunction};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Timestamp;

/// Inside each Incoming folder: where converted files are made, before they take the originals' place. Hidden, so
/// media servers, Lidarr and the catalogue leave it alone.
pub const STAGING_DIR: &str = ".cratarr-convert";

/// The CD sample rate.
pub const CD_RATE: u32 = 44_100;
/// The CD bit depth.
pub const CD_BITS: u32 = 16;

/// FLAC block size of the output, in frames: the reference encoder's for 44.1 kHz.
const BLOCK_SIZE: usize = 4096;
/// Input frames per resampler call, roughly (see [`resampler_chunk`]). The FFT resampler's anti-alias filter is as
/// long as one call, so this sets its steepness: about 8k frames puts the passband edge near 21.9 kHz with the stop
/// band from Nyquist on.
const RESAMPLER_CHUNK: usize = 8192;
/// Written into the Vorbis comment block's vendor field: who made the audio frames.
const VENDOR: &str = concat!("Cratarr ", env!("CARGO_PKG_VERSION"), " (flacenc)");

/// The container of a lossless source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Flac,
    Wav,
    Aiff,
    /// MP4/M4A holding ALAC.
    Mp4,
    /// A lossy file (AAC, Ogg Vorbis), opened only to stream it ([`Source::open_any`]).
    Other,
}

/// A lossless source's audio format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFormat {
    pub container: Container,
    pub sample_rate: u32,
    pub bits_per_sample: u32,
    pub channels: usize,
    /// Length in frames, when the header says.
    pub frames: Option<u64>,
}

/// What a conversion did.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub source: SourceFormat,
    /// The output's sample rate: [`CD_RATE`], or the source's when that is lower.
    pub sample_rate: u32,
    /// The output's length in frames.
    pub frames: u64,
    /// The highest absolute sample value before quantising, in dBFS (above 0 means it had to clip).
    pub peak_dbfs: f64,
    /// Output samples that hit the 16-bit limits and were clamped.
    pub clipped: u64,
}

// ---- The functional core ----

/// Hi-res: more than CD in either sample rate or bit depth.
pub fn is_hires(sample_rate: u32, bits_per_sample: u32) -> bool {
    sample_rate > CD_RATE || bits_per_sample > CD_BITS
}

/// The output rate: CD's, unless the source is already lower (that is not upsampled; only the bit depth drops).
pub fn target_rate(source_rate: u32) -> u32 {
    source_rate.min(CD_RATE)
}

/// The output length for `frames` input frames: the same duration, rounded to the nearest frame.
pub fn output_frames(frames: u64, from_rate: u32, to_rate: u32) -> u64 {
    let (f, a, b) = (u128::from(frames), u128::from(from_rate), u128::from(to_rate));
    ((f * b + a / 2) / a) as u64
}

/// The resampler's input chunk: whole blocks of `from / gcd` frames (one block maps to exactly `to / gcd` output
/// frames), about [`RESAMPLER_CHUNK`] long. The block count is chosen so the output side is even: the filter delays by
/// half of it, and an odd size would leave the output half a sample late (at 192 kHz, 13 blocks of 147 would).
pub fn resampler_chunk(from_rate: u32, to_rate: u32) -> usize {
    let (mut a, mut b) = (from_rate as usize, to_rate as usize);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let (min_in, min_out) = (from_rate as usize / a, to_rate as usize / a);
    let mut blocks = RESAMPLER_CHUNK.div_ceil(min_in);
    if blocks * min_out % 2 == 1 {
        blocks += 1;
    }
    blocks * min_in
}

/// One sample, full scale ±1.0, plus `dither` in 16-bit steps, to 16 bit. True when it had to clamp.
pub fn quantise(x: f32, dither: f64) -> (i32, bool) {
    let y = (f64::from(x) * 32768.0 + dither).round();
    if y > 32767.0 {
        (32767, true)
    } else if y < -32768.0 {
        (-32768, true)
    } else {
        (y as i32, false)
    }
}

/// TPDF dither: the difference of two uniform values, so triangular over ±1 step (the least that makes the error
/// independent of the signal, without noise shaping). A small xorshift generator, seeded fixed: the same input
/// converts to the same bytes.
pub(crate) struct Tpdf(u64);

impl Tpdf {
    pub(crate) fn new() -> Self {
        Tpdf(0x9E37_79B9_7F4A_7C15)
    }

    fn uniform(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    pub(crate) fn next(&mut self) -> f64 {
        self.uniform() - self.uniform()
    }
}

// ---- The shell ----

/// `Some` when `path` is a lossless file (FLAC, WAV, AIFF, ALAC in MP4) above CD format; `None` for CD format or
/// lower, and for lossy or unknown files. Reads only the headers.
pub fn needs_conversion(path: &Path) -> Result<Option<SourceFormat>> {
    let Some(source) = Source::open(path)? else { return Ok(None) };
    let f = source.format;
    Ok(is_hires(f.sample_rate, f.bits_per_sample).then_some(f))
}

/// Convert a lossless file to 16-bit FLAC at 44.1 kHz (or the source's rate when lower), with all its tags and
/// pictures. The output appears whole or not at all: it is written to a temporary file next to it, then renamed.
/// `output` may be `input` itself. It gets the input's permissions, as a copy would (a new file's own were 644 where
/// the library's files are 664, so the group could not write the converted ones: NAS, 2026-10-15).
/// (The queue converts through `to_cd_flac_told`, to show its progress; this is the tests'.)
#[cfg(test)]
pub fn to_cd_flac(input: &Path, output: &Path) -> Result<Report> {
    to_cd_flac_told(input, output, |_| {})
}

/// [`to_cd_flac`], telling `progress` how far it is (0 to 1 of the input read) at each whole percent, when the
/// file's header gives its length.
pub fn to_cd_flac_told(input: &Path, output: &Path, progress: impl FnMut(f32)) -> Result<Report> {
    let dir = match output.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let name = output.file_name().context("the output has no file name")?;
    let temp = dir.join(format!(".{}.cratarr-part", name.to_string_lossy()));
    let result = convert_to(input, &temp, None, progress);
    match result {
        Ok(report) => {
            if let Ok(m) = std::fs::metadata(input) {
                std::fs::set_permissions(&temp, m.permissions()).with_context(|| format!("giving {} the input's permissions", temp.display()))?;
            }
            std::fs::rename(&temp, output).with_context(|| format!("renaming into {}", output.display()))?;
            Ok(report)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&temp);
            Err(e)
        }
    }
}

/// The conversion itself, into `out` (overwritten). With `raw`, also writes the 32-bit float signal just before
/// dithering, interleaved little-endian: the measurement hook for the SoX comparison.
fn convert_to(input: &Path, out: &Path, raw: Option<&Path>, mut progress: impl FnMut(f32)) -> Result<Report> {
    let mut source = Source::open(input)?.with_context(|| format!("{} is not a lossless audio file", input.display()))?;
    let format = source.format.clone();
    if format.channels == 0 || format.channels > 8 {
        bail!("{} channels: FLAC takes 1 to 8", format.channels);
    }
    let rate = target_rate(format.sample_rate);
    let file = File::create(out).with_context(|| format!("creating {}", out.display()))?;
    let mut sink = DitherSink::new(FlacWriter::new(file, rate, format.channels, CD_BITS as usize)?, format.channels);
    let mut raw_out = raw.map(File::create).transpose()?.map(BufWriter::new);
    let mut scratch = Vec::new();
    let mut tee = |planes: &[Vec<f32>], start: usize, len: usize| -> Result<()> {
        if let Some(w) = raw_out.as_mut() {
            for i in start..start + len {
                for p in planes {
                    w.write_all(&p[i].to_le_bytes())?;
                }
            }
        }
        sink.push(planes, start, len)
    };
    let (mut read, mut told) = (0u64, 0u64);
    stream(
        |planes| {
            let before = planes[0].len();
            let more = source.read(planes, &mut scratch)?;
            read += (planes[0].len() - before) as u64;
            if let Some(total) = format.frames.filter(|t| *t > 0) {
                let percent = (read * 100 / total).min(100);
                if percent > told {
                    told = percent;
                    progress(percent as f32 / 100.0);
                }
            }
            Ok(more)
        },
        format.channels,
        format.sample_rate,
        rate,
        &mut tee,
    )?;
    if let Some(mut w) = raw_out {
        w.flush()?;
    }
    let (frames, peak, clipped) = (sink.frames, sink.peak, sink.clipped);
    sink.finish()?;
    drop(source);
    copy_tags(input, format.container, out)?;
    File::open(out)?.sync_all()?;
    Ok(Report {
        source: format,
        sample_rate: rate,
        frames,
        peak_dbfs: 20.0 * f64::from(peak.max(1e-10)).log10(),
        clipped,
    })
}

/// Pull audio through the resampler into `sink`, a chunk at a time. `read` appends decoded frames to the per-channel
/// buffers and says false at the end. Returns the number of input frames. Also the downloads' copies' (`opus_copy`).
pub(crate) fn stream(
    mut read: impl FnMut(&mut [Vec<f32>]) -> Result<bool>,
    channels: usize,
    from_rate: u32,
    to_rate: u32,
    sink: &mut impl FnMut(&[Vec<f32>], usize, usize) -> Result<()>,
) -> Result<u64> {
    let mut input: Vec<Vec<f32>> = vec![Vec::new(); channels];
    let mut total_in = 0u64;
    if from_rate == to_rate {
        while read(&mut input)? {
            let n = input[0].len();
            sink(&input, 0, n)?;
            total_in += n as u64;
            input.iter_mut().for_each(Vec::clear);
        }
        return Ok(total_in);
    }

    let mut rs = Fft::<f32>::new_custom(
        from_rate as usize,
        to_rate as usize,
        resampler_chunk(from_rate, to_rate),
        1,
        channels,
        WindowFunction::BlackmanHarris2,
        FixedSync::Both,
    )
    .map_err(|e| anyhow!("resampler: {e}"))?;
    let n_in = rs.input_frames_next();
    let n_out = rs.output_frames_max();
    let mut output = vec![vec![0f32; n_out]; channels];
    // The filter delays everything by this many output frames: drop them, so the output lines up with the input.
    let mut skip = rs.output_delay();
    let mut emitted = 0u64;
    let mut emit = |output: &[Vec<f32>], produced: usize, skip: &mut usize, limit: u64| -> Result<u64> {
        let drop = (*skip).min(produced);
        *skip -= drop;
        let n = ((produced - drop) as u64).min(limit) as usize;
        if n > 0 {
            sink(output, drop, n)?;
        }
        Ok(n as u64)
    };

    while read(&mut input)? {
        let avail = input[0].len();
        let mut pos = 0;
        while avail - pos >= n_in {
            let from = SequentialSliceOfVecs::new(&input, channels, avail).map_err(|e| anyhow!("{e}"))?;
            let mut to = SequentialSliceOfVecs::new_mut(&mut output, channels, n_out).map_err(|e| anyhow!("{e}"))?;
            let (used, produced) = rs
                .process_into_buffer(&from, &mut to, Some(&Indexing::new().input_offset(pos)))
                .map_err(|e| anyhow!("resampler: {e}"))?;
            pos += used;
            total_in += used as u64;
            emitted += emit(&output, produced, &mut skip, u64::MAX)?;
        }
        input.iter_mut().for_each(|c| {
            c.drain(..pos);
        });
    }

    // The tail: what is left of the input, then silence until the filter has given back everything.
    let rest = input[0].len();
    total_in += rest as u64;
    let expected = output_frames(total_in, from_rate, to_rate);
    let mut partial = rest;
    while emitted < expected {
        let from = SequentialSliceOfVecs::new(&input, channels, rest).map_err(|e| anyhow!("{e}"))?;
        let mut to = SequentialSliceOfVecs::new_mut(&mut output, channels, n_out).map_err(|e| anyhow!("{e}"))?;
        let (_, produced) = rs
            .process_into_buffer(&from, &mut to, Some(&Indexing::new().partial_len(partial)))
            .map_err(|e| anyhow!("resampler: {e}"))?;
        partial = 0;
        emitted += emit(&output, produced, &mut skip, expected - emitted)?;
    }
    Ok(total_in)
}

/// A decoder over one lossless file.
pub(crate) struct Source {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    pub(crate) format: SourceFormat,
}

impl Source {
    /// `None` when the file is not one of the lossless kinds this converts.
    pub(crate) fn open(path: &Path) -> Result<Option<Source>> {
        Self::open_with(path, false)
    }

    /// Any file a decoder here reads, lossy ones too (AAC, Ogg Vorbis): a browser's stream of a track it cannot take
    /// as it is (`opus_copy::stream_mp4`). Never for a copy or a conversion: a second lossy pass only loses more.
    pub(crate) fn open_any(path: &Path) -> Result<Option<Source>> {
        Self::open_with(path, true)
    }

    fn open_with(path: &Path, lossy_too: bool) -> Result<Option<Source>> {
        let kind = Probe::open(path)
            .with_context(|| format!("opening {}", path.display()))?
            .guess_file_type()?
            .file_type();
        let container = match kind {
            Some(FileType::Flac) => Container::Flac,
            Some(FileType::Wav) => Container::Wav,
            Some(FileType::Aiff) => Container::Aiff,
            Some(FileType::Mp4) => Container::Mp4,
            _ if lossy_too => Container::Other,
            _ => return Ok(None),
        };
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let reader = symphonia::default::get_probe()
            .probe(&Hint::new(), mss, FormatOptions::default(), MetadataOptions::default())
            .with_context(|| format!("reading {}", path.display()))?;
        let track = reader.default_track(TrackType::Audio).context("no audio track")?;
        let Some(params) = track.codec_params.as_ref().and_then(|p| p.audio()).cloned() else { return Ok(None) };
        let codecs = symphonia::default::get_codecs();
        if codecs.get_audio_decoder(params.codec).is_none() {
            return Ok(None);
        }
        // AAC's and Vorbis's decoders are built in too (for streams), and MP4 holds AAC as often as ALAC: lossless is
        // FLAC, PCM, or ALAC in MP4.
        let lossless = match container {
            Container::Other => false,
            Container::Mp4 => params.codec == symphonia::core::codecs::audio::well_known::CODEC_ID_ALAC,
            _ => true,
        };
        if !lossless && !lossy_too {
            return Ok(None);
        }
        let sample_rate = params.sample_rate.context("the sample rate is unknown")?;
        // A lossy codec has no bit depth of its own.
        let bits_per_sample = match params.bits_per_sample.or_else(|| alac_bit_depth(params.extra_data.as_deref())) {
            Some(b) => b,
            None if !lossless => 16,
            None => bail!("the bit depth is unknown"),
        };
        let channels = params.channels.as_ref().map(|c| c.count()).context("the channels are unknown")?;
        let format = SourceFormat { container, sample_rate, bits_per_sample, channels, frames: track.num_frames };
        let track = track.id;
        let decoder = codecs.make_audio_decoder(&params, &AudioDecoderOptions::default())?;
        Ok(Some(Source { reader, decoder, track, format }))
    }

    /// Move to the packet holding input frame `frame`; returns the frame that packet starts at, where reading
    /// continues (a phone's stream after a seek, `opus_copy::OpusMp4`).
    pub(crate) fn seek(&mut self, frame: u64) -> Result<u64> {
        let to = self
            .reader
            .seek(SeekMode::Accurate, SeekTo::Timestamp { ts: Timestamp::from(frame as i64), track_id: self.track })
            .context("seeking")?;
        self.decoder.reset();
        Ok(u64::try_from(to.actual_ts.get()).unwrap_or(0))
    }

    /// Decode the next packet and append it to `planes`. False at the end. A damaged packet is an error, not a
    /// gap: the result replaces the original.
    pub(crate) fn read(&mut self, planes: &mut [Vec<f32>], scratch: &mut Vec<Vec<f32>>) -> Result<bool> {
        loop {
            let Some(packet) = self.reader.next_packet()? else { return Ok(false) };
            if packet.track_id != self.track {
                continue;
            }
            let decoded = self.decoder.decode(&packet)?;
            if decoded.frames() == 0 {
                continue;
            }
            decoded.copy_to_vecs_planar::<f32>(scratch);
            if scratch.len() != planes.len() {
                bail!("the channel count changed mid-stream");
            }
            for (p, s) in planes.iter_mut().zip(scratch.iter()) {
                p.extend_from_slice(s);
            }
            return Ok(true);
        }
    }
}

/// ALAC's bit depth from its magic cookie (ALACSpecificConfig: frame length, compatible version, then the bit depth),
/// which symphonia does not hand on; the cookie may still carry its `alac` atom's header.
fn alac_bit_depth(cookie: Option<&[u8]>) -> Option<u32> {
    let c = cookie?;
    let c = if c.len() >= 12 && &c[4..8] == b"alac" { &c[12..] } else { c };
    c.get(5).copied().filter(|b| matches!(b, 16 | 20 | 24 | 32)).map(u32::from)
}

/// Dithers float frames to 16 bit and hands them to the FLAC writer one block at a time.
struct DitherSink {
    flac: FlacWriter,
    block: Vec<i32>,
    channels: usize,
    dither: Tpdf,
    frames: u64,
    peak: f32,
    clipped: u64,
}

impl DitherSink {
    fn new(flac: FlacWriter, channels: usize) -> Self {
        DitherSink {
            flac,
            block: Vec::with_capacity(BLOCK_SIZE * channels),
            channels,
            dither: Tpdf::new(),
            frames: 0,
            peak: 0.0,
            clipped: 0,
        }
    }

    fn push(&mut self, planes: &[Vec<f32>], start: usize, len: usize) -> Result<()> {
        for i in start..start + len {
            for p in planes.iter().take(self.channels) {
                let x = p[i];
                self.peak = self.peak.max(x.abs());
                let (q, clipped) = quantise(x, self.dither.next());
                self.clipped += u64::from(clipped);
                self.block.push(q);
            }
            if self.block.len() == BLOCK_SIZE * self.channels {
                self.flac.write_block(&self.block)?;
                self.block.clear();
            }
        }
        self.frames += len as u64;
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        if !self.block.is_empty() {
            self.flac.write_block(&self.block)?;
        }
        self.flac.finish()
    }
}

fn flac_err(e: impl std::fmt::Debug) -> anyhow::Error {
    anyhow!("FLAC encoder: {e:?}")
}

/// STREAMINFO's 34 bytes.
fn info_bytes(info: &StreamInfo) -> Result<Vec<u8>> {
    let mut sink = flacenc::bitsink::ByteSink::new();
    info.write(&mut sink).map_err(flac_err)?;
    Ok(sink.into_inner())
}

/// A streaming FLAC writer: the header first, frames as they come, then the header again with the totals and MD5.
struct FlacWriter {
    out: BufWriter<File>,
    info: StreamInfo,
    config: Verified<flacenc::config::Encoder>,
    framebuf: FrameBuf,
    context: FlacContext,
    bytes: flacenc::bitsink::ByteSink,
}

impl FlacWriter {
    fn new(file: File, sample_rate: u32, channels: usize, bits: usize) -> Result<Self> {
        let info = StreamInfo::new(sample_rate as usize, channels, bits).map_err(flac_err)?;
        let config = flacenc::config::Encoder::default().into_verified().map_err(|(_, e)| flac_err(e))?;
        let mut w = FlacWriter {
            out: BufWriter::new(file),
            info,
            config,
            framebuf: FrameBuf::with_size(channels, BLOCK_SIZE).map_err(flac_err)?,
            context: FlacContext::new(bits, channels),
            bytes: flacenc::bitsink::ByteSink::new(),
        };
        w.write_header()?;
        Ok(w)
    }

    /// `fLaC`, then STREAMINFO as the only (so last) metadata block. The tags go in afterwards.
    fn write_header(&mut self) -> Result<()> {
        self.out.write_all(b"fLaC")?;
        self.out.write_all(&[0x80, 0, 0, 34])?;
        self.out.write_all(&info_bytes(&self.info)?)?;
        Ok(())
    }

    /// One block of interleaved samples: `BLOCK_SIZE` frames, or fewer for the last.
    fn write_block(&mut self, interleaved: &[i32]) -> Result<()> {
        let mut fill = (&mut self.framebuf, &mut self.context);
        fill.fill_interleaved(interleaved).map_err(flac_err)?;
        let number = self.context.current_frame_number().context("no frame number")?;
        let frame =
            flacenc::encode_fixed_size_frame(&self.config, &self.framebuf, number, &self.info).map_err(flac_err)?;
        self.info.update_frame_info(&frame);
        self.bytes.clear();
        frame.write(&mut self.bytes).map_err(flac_err)?;
        self.out.write_all(self.bytes.as_slice())?;
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        // A fixed block size; the short last block does not count (as the reference encoder writes it).
        self.info.set_block_sizes(BLOCK_SIZE, BLOCK_SIZE).map_err(flac_err)?;
        self.info.set_md5_digest(&self.context.md5_digest());
        self.out.flush()?;
        let mut file = self.out.into_inner().map_err(|e| e.into_error())?;
        file.seek(SeekFrom::Start(8))?;
        file.write_all(&info_bytes(&self.info)?)?;
        file.flush()?;
        Ok(())
    }
}

/// Copy every tag and picture of `input` into the FLAC at `output`, as a Vorbis comment block and PICTURE blocks.
/// From FLAC the comments go over verbatim; other containers go through lofty's generic tag mapping.
fn copy_tags(input: &Path, container: Container, output: &Path) -> Result<()> {
    let options = ParseOptions::new().read_properties(false);
    let mut comments = match container {
        Container::Flac => {
            let mut f = File::open(input)?;
            let flac = FlacFile::read_from(&mut f, options).with_context(|| format!("reading tags of {}", input.display()))?;
            let mut vc = match (flac.vorbis_comments(), flac.id3v2()) {
                (Some(vc), _) => vc.clone(),
                (None, Some(id3)) => VorbisComments::from(Tag::from(id3.clone())),
                (None, None) => VorbisComments::default(),
            };
            for (picture, info) in flac.pictures() {
                vc.insert_picture(picture.clone(), Some(*info))?;
            }
            vc
        }
        _ => {
            let tagged = Probe::open(input)?.options(options).read().with_context(|| format!("reading tags of {}", input.display()))?;
            match tagged.primary_tag().or_else(|| tagged.first_tag()) {
                Some(tag) => VorbisComments::from(tag.clone()),
                None => VorbisComments::default(),
            }
        }
    };
    if comments.is_empty() {
        return Ok(());
    }
    comments.set_vendor(VENDOR.to_string());
    let mut f = OpenOptions::new().read(true).write(true).open(output)?;
    comments.save_to(&mut f, WriteOptions::default()).context("writing the tags")?;
    Ok(())
}

/// For the SoX comparison: where an ignored test writes its files.
fn _tool_paths() -> Option<(PathBuf, PathBuf, Option<PathBuf>)> {
    let input = std::env::var_os("CRATARR_CONVERT_IN")?;
    let output = std::env::var_os("CRATARR_CONVERT_OUT")?;
    Some((input.into(), output.into(), std::env::var_os("CRATARR_CONVERT_RAW").map(PathBuf::from)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use lofty::picture::{MimeType, Picture, PictureType};
    use std::f64::consts::PI;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir() -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("cratarr-convert-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A sine of `freq` Hz at `amp` full scale, `secs` long, on every channel (the right one a quarter turn later).
    pub(crate) fn sine(rate: u32, freq: f64, amp: f64, secs: f64, channels: usize) -> Vec<Vec<f64>> {
        let n = (f64::from(rate) * secs) as usize;
        (0..channels)
            .map(|c| (0..n).map(|i| amp * (2.0 * PI * freq * i as f64 / f64::from(rate) + c as f64 * PI / 2.0).sin()).collect())
            .collect()
    }

    /// Write a test FLAC of any bit depth with this crate's own writer.
    fn write_flac(path: &Path, rate: u32, bits: usize, planes: &[Vec<f64>]) {
        let ch = planes.len();
        let scale = f64::from(1u32 << (bits - 1));
        let mut w = FlacWriter::new(File::create(path).unwrap(), rate, ch, bits).unwrap();
        let n = planes[0].len();
        let mut block = Vec::new();
        for i in 0..n {
            for p in planes {
                block.push(((p[i] * scale).round() as i32).clamp(-(scale as i32), scale as i32 - 1));
            }
            if block.len() == BLOCK_SIZE * ch || i == n - 1 {
                w.write_block(&block).unwrap();
                block.clear();
            }
        }
        w.finish().unwrap();
    }

    /// Write a 24-bit PCM WAV.
    pub(crate) fn write_wav24(path: &Path, rate: u32, planes: &[Vec<f64>]) {
        let ch = planes.len() as u16;
        let n = planes[0].len() as u32;
        let data_len = n * u32::from(ch) * 3;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&ch.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * u32::from(ch) * 3).to_le_bytes());
        b.extend_from_slice(&(ch * 3).to_le_bytes());
        b.extend_from_slice(&24u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..n as usize {
            for p in planes {
                let v = ((p[i] * 8_388_608.0).round() as i32).clamp(-8_388_608, 8_388_607);
                b.extend_from_slice(&v.to_le_bytes()[..3]);
            }
        }
        std::fs::write(path, b).unwrap();
    }

    /// Decode a file back to planes in full scale.
    fn decode(path: &Path) -> (SourceFormat, Vec<Vec<f32>>) {
        let mut s = Source::open(path).unwrap().unwrap();
        let mut planes = vec![Vec::new(); s.format.channels];
        let mut scratch = Vec::new();
        while s.read(&mut planes, &mut scratch).unwrap() {}
        (s.format.clone(), planes)
    }

    /// Frequency from upward zero crossings, interpolated, between the first and last crossing.
    fn frequency(x: &[f32], rate: u32) -> f64 {
        let mut crossings = Vec::new();
        for i in 1..x.len() {
            if x[i - 1] < 0.0 && x[i] >= 0.0 {
                let t = f64::from(x[i - 1]) / f64::from(x[i - 1] - x[i]);
                crossings.push(i as f64 - 1.0 + t);
            }
        }
        let (a, b) = (crossings[0], crossings[crossings.len() - 1]);
        (crossings.len() - 1) as f64 * f64::from(rate) / (b - a)
    }

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / x.len() as f64).sqrt()
    }

    #[test]
    fn decisions() {
        assert!(is_hires(96_000, 24));
        assert!(is_hires(48_000, 16));
        assert!(is_hires(44_100, 24));
        assert!(!is_hires(44_100, 16));
        assert_eq!(target_rate(192_000), 44_100);
        assert_eq!(target_rate(32_000), 32_000);
        assert_eq!(output_frames(96_000 * 60, 96_000, 44_100), 44_100 * 60);
        assert_eq!(output_frames(1, 48_000, 44_100), 1);
        assert_eq!(resampler_chunk(96_000, 44_100), 8320); // 26 blocks of 320 in, 147 out
        assert_eq!(resampler_chunk(192_000, 44_100), 8960); // 14 blocks of 640, not 13: an even output side
        assert_eq!(resampler_chunk(88_200, 44_100), 8192);
        assert_eq!(resampler_chunk(48_000, 44_100), 8320);
        assert_eq!(quantise(1.0, 0.0), (32767, true));
        assert_eq!(quantise(-1.0, 0.0), (-32768, false));
        assert_eq!(quantise(0.5 / 32768.0, 0.6), (1, false));
        let mut d = Tpdf::new();
        let v: Vec<f64> = (0..100_000).map(|_| d.next()).collect();
        assert!(v.iter().all(|x| x.abs() < 1.0));
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let var = v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64;
        assert!(mean.abs() < 0.01, "{mean}");
        assert!((var - 1.0 / 6.0).abs() < 0.01, "{var}"); // triangular over ±1: variance 1/6
    }

    #[test]
    fn hires_sine_to_cd() {
        let d = temp_dir();
        let (src, out) = (d.join("in.flac"), d.join("out.flac"));
        let secs = 3.0;
        write_flac(&src, 96_000, 24, &sine(96_000, 1000.0, 0.5, secs, 2));
        let found = needs_conversion(&src).unwrap().unwrap();
        assert_eq!((found.container, found.sample_rate, found.bits_per_sample, found.channels), (Container::Flac, 96_000, 24, 2));
        assert_eq!(found.frames, Some(288_000));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o664)).unwrap();
        }

        let report = to_cd_flac(&src, &out).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o664, "the input's permissions, as a copy");
        }
        assert_eq!(report.sample_rate, 44_100);
        assert_eq!(report.frames, 132_300);
        assert_eq!(report.clipped, 0);
        // -6 dBFS, plus the filter's ringing where the right channel jumps from silence to its crest at the start.
        assert!(report.peak_dbfs > -6.1 && report.peak_dbfs < -5.0, "{}", report.peak_dbfs);
        assert!(!d.join(".out.flac.cratarr-part").exists());

        let (fmt, planes) = decode(&out);
        assert_eq!((fmt.sample_rate, fmt.bits_per_sample, fmt.channels), (44_100, 16, 2));
        let len = planes[0].len() as i64;
        assert!((len - 132_300).abs() <= 2, "{len} frames");
        assert_eq!(needs_conversion(&out).unwrap(), None);
        for p in &planes {
            let f = frequency(&p[1000..p.len() - 1000], 44_100);
            assert!((f - 1000.0).abs() < 0.05, "{f} Hz");
            // Amplitude kept: a sine's RMS is its amplitude over √2.
            let r = rms(&p[1000..p.len() - 1000]);
            assert!((r - 0.5 / 2f64.sqrt()).abs() < 0.001, "rms {r}");
        }
        // Lined up with the source: no delay from the filter. At 1 kHz, the first upward crossing is at frame 0.
        let first = (1..2000).find(|&i| planes[0][i - 1] < 0.0 && planes[0][i] >= 0.0).unwrap();
        assert!((first as f64 - 44.1).abs() < 1.5, "first crossing at {first}");
        // The FLAC is valid: the header's totals and MD5 match what lofty and symphonia read.
        let flac = FlacFile::read_from(&mut File::open(&out).unwrap(), ParseOptions::new()).unwrap();
        assert_eq!(flac.properties().sample_rate(), 44_100);
        assert_eq!(flac.properties().bit_depth(), 16);
    }

    #[test]
    fn progress_rises_to_the_whole() {
        let d = temp_dir();
        let (src, out) = (d.join("in.flac"), d.join("out.flac"));
        write_flac(&src, 96_000, 24, &sine(96_000, 1000.0, 0.5, 3.0, 2));
        let mut told = Vec::new();
        to_cd_flac_told(&src, &out, |p| told.push(p)).unwrap();
        assert!(told.len() > 10, "{told:?}");
        assert!(told.windows(2).all(|w| w[0] < w[1]), "{told:?}");
        assert_eq!(told.last(), Some(&1.0));
    }

    #[test]
    fn the_48k_family_and_wav() {
        let d = temp_dir();
        for rate in [48_000u32, 192_000] {
            let (src, out) = (d.join(format!("in{rate}.wav")), d.join(format!("out{rate}.flac")));
            write_wav24(&src, rate, &sine(rate, 997.0, 0.25, 2.0, 2));
            let found = needs_conversion(&src).unwrap().unwrap();
            assert_eq!((found.container, found.sample_rate, found.bits_per_sample), (Container::Wav, rate, 24));
            let report = to_cd_flac(&src, &out).unwrap();
            assert_eq!(report.frames, 88_200);
            let (_, planes) = decode(&out);
            assert_eq!(planes[0].len(), 88_200);
            let f = frequency(&planes[1][500..88_000], 44_100);
            assert!((f - 997.0).abs() < 0.05, "{rate}: {f} Hz");
        }
    }

    #[test]
    fn filter_keeps_the_band_and_stops_aliases() {
        // Straight through the resampler, before dither: measure the filter itself.
        let run = |rate: u32, freq: f64| -> f64 {
            let tone = sine(rate, freq, 0.5, 1.0, 1);
            let mut fed = false;
            let mut got: Vec<f32> = Vec::new();
            stream(
                |planes| {
                    if fed {
                        return Ok(false);
                    }
                    fed = true;
                    planes[0].extend(tone[0].iter().map(|&v| v as f32));
                    Ok(true)
                },
                1,
                rate,
                44_100,
                &mut |p, start, len| {
                    got.extend_from_slice(&p[0][start..start + len]);
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(got.len(), 44_100);
            // In the band the output is the same sine sampled at 44.1 kHz, in phase: no delay, not even half a sample.
            if freq < 20_000.0 {
                let ideal = sine(44_100, freq, 0.5, 1.0, 1);
                let err: Vec<f32> = (4000..40_000).map(|i| got[i] - ideal[0][i] as f32).collect();
                let err_db = 20.0 * (rms(&err) / (0.5 / 2f64.sqrt())).log10();
                assert!(err_db < -80.0, "{rate} Hz, {freq} Hz tone off the ideal by {err_db} dB");
            }
            20.0 * (rms(&got[4000..40_000]) / (0.5 / 2f64.sqrt())).log10()
        };
        for rate in [48_000, 88_200, 96_000, 176_400, 192_000] {
            for freq in [100.0, 1000.0, 10_000.0, 20_000.0] {
                let gain = run(rate, freq);
                assert!(gain.abs() < 0.05, "{rate} Hz, {freq} Hz tone: {gain} dB");
            }
            for freq in [22_500.0, 23_000.0, 30_000.0].into_iter().filter(|&f| f < f64::from(rate) / 2.0) {
                let gain = run(rate, freq);
                assert!(gain < -100.0, "{rate} Hz, {freq} Hz tone leaks at {gain} dB");
            }
        }
    }

    #[test]
    fn tags_and_pictures_are_copied() {
        let d = temp_dir();
        let (src, out) = (d.join("in.flac"), d.join("out.flac"));
        write_flac(&src, 96_000, 24, &sine(96_000, 440.0, 0.3, 0.5, 2));
        let mut vc = VorbisComments::default();
        vc.push("TITLE".into(), "Test Tone".into());
        vc.push("ARTIST".into(), "First Artist".into());
        vc.push("ARTIST".into(), "Second Artist".into());
        vc.push("ALBUM".into(), "Synthetic".into());
        vc.push("TRACKNUMBER".into(), "3".into());
        vc.push("MUSICBRAINZ_TRACKID".into(), "00000000-0000-0000-0000-000000000001".into());
        vc.push("SOME_CUSTOM_FIELD".into(), "kept as is".into());
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x02\0\0\0\x90wS\xde\0\0\0\x0cIDATx\x9cc\xf8\x0f\0\0\x01\x01\0\x05\x18\xd8N\0\0\0\0IEND\xaeB`\x82";
        let front = Picture::unchecked(png.to_vec()).pic_type(PictureType::CoverFront).mime_type(MimeType::Png).build();
        let back = Picture::unchecked(png.to_vec()).pic_type(PictureType::CoverBack).mime_type(MimeType::Png).build();
        vc.insert_picture(front, None).unwrap();
        vc.insert_picture(back, None).unwrap();
        vc.save_to_path(&src, WriteOptions::default()).unwrap();

        to_cd_flac(&src, &out).unwrap();
        let flac = FlacFile::read_from(&mut File::open(&out).unwrap(), ParseOptions::new()).unwrap();
        let got = flac.vorbis_comments().unwrap();
        let mut want: Vec<(String, String)> = vc.items().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let mut have: Vec<(String, String)> = got.items().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        want.sort();
        have.sort();
        assert_eq!(have, want);
        assert_eq!(got.vendor(), VENDOR);
        let pics: Vec<_> = flac.pictures().iter().chain(got.pictures()).map(|(p, _)| (p.pic_type(), p.data().to_vec())).collect();
        assert_eq!(pics.len(), 2);
        assert!(pics.iter().any(|(t, data)| *t == PictureType::CoverFront && data == png));
        assert!(pics.iter().any(|(t, data)| *t == PictureType::CoverBack && data == png));
        // Audio intact behind the larger header.
        let (fmt, planes) = decode(&out);
        assert_eq!((fmt.sample_rate, planes[0].len()), (44_100, 22_050));
    }

    #[test]
    fn cd_format_and_lossy_need_nothing() {
        let d = temp_dir();
        let cd = d.join("cd.flac");
        write_flac(&cd, 44_100, 16, &sine(44_100, 440.0, 0.3, 0.2, 2));
        assert_eq!(needs_conversion(&cd).unwrap(), None);
        // 24 bit at 44.1 kHz only loses bits: no resampling, same length.
        let hi = d.join("hi.flac");
        write_flac(&hi, 44_100, 24, &sine(44_100, 440.0, 0.3, 0.2, 2));
        assert!(needs_conversion(&hi).unwrap().is_some());
        let report = to_cd_flac(&hi, &hi).unwrap();
        assert_eq!((report.sample_rate, report.frames), (44_100, 8820));
        assert_eq!(needs_conversion(&hi).unwrap(), None);
        // An MP3 (just a frame header and silence) is lossy: nothing to do.
        let mp3 = d.join("x.mp3");
        let mut b = vec![0xFF, 0xFB, 0x90, 0x64];
        b.resize(417 * 4, 0);
        std::fs::write(&mp3, b).unwrap();
        assert_eq!(needs_conversion(&mp3).unwrap(), None);
    }

    #[test]
    fn a_failed_conversion_leaves_nothing() {
        let d = temp_dir();
        let bad = d.join("bad.flac");
        std::fs::write(&bad, b"fLaC not really").unwrap();
        assert!(to_cd_flac(&bad, &d.join("out.flac")).is_err());
        assert!(!d.join("out.flac").exists());
        assert!(!d.join(".out.flac.cratarr-part").exists());
    }

    /// The measurement tool for the SoX comparison (decision 0013), not a test:
    /// `CRATARR_CONVERT_IN=a.flac CRATARR_CONVERT_OUT=b.flac [CRATARR_CONVERT_RAW=b.f32] cargo test -- --ignored tool`.
    /// Prints the report, the time taken and the process's peak resident memory.
    #[test]
    #[ignore]
    fn tool() {
        let Some((input, output, raw)) = _tool_paths() else { return };
        let start = std::time::Instant::now();
        let report = convert_to(&input, &output, raw.as_deref(), |_| {}).unwrap();
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let hwm = status.lines().find(|l| l.starts_with("VmHWM")).unwrap_or("VmHWM: ?").to_string();
        println!("REPORT {report:?} secs={:.2} {hwm}", start.elapsed().as_secs_f64());
    }
}
