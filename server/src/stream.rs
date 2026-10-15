//! Local playback's audio (decision 0022): a track's file as the browser's continuous stream (MSE) takes it.
//!
//! - **FLAC** is repackaged, never re-encoded, into fragmented MP4 (browsers take FLAC in MSE only that way): an init
//!   segment (`ftyp`, `moov` with an `fLaC` sample entry carrying the file's STREAMINFO in `dfLa`), then fragments of
//!   about a second (`moof` + `mdat`), each holding the FLAC frames exactly as they are in the file. symphonia reads
//!   the frames one at a time, so memory stays at about one fragment (production runs within 64 MB).
//! - **MP3** goes to the stream as it is; the page trims it by its LAME header.
//!
//! The FLAC-in-ISOBMFF layout follows the "Encapsulation of FLAC in ISO Base Media File Format" specification
//! (xiph.org); the boxes follow ISO/IEC 14496-12.

use anyhow::{bail, Context, Result};
use std::fs::File;
use std::path::Path;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Timestamp;

/// A fragment holds about this much audio: small enough for the memory limit, large enough to keep the box overhead
/// negligible (a few dozen bytes a second).
const FRAGMENT_SECONDS: u64 = 1;

/// What the page needs to place a track in the stream.
#[derive(Debug, Clone, PartialEq)]
pub struct FlacInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    /// Samples per channel in the whole track (STREAMINFO), when the file says.
    pub total_samples: Option<u64>,
}

/// A FLAC file read frame by frame and handed out as fragmented MP4: [`FlacToMp4::init`] once, then
/// [`FlacToMp4::next_fragment`] until `None`.
pub struct FlacToMp4 {
    reader: Box<dyn FormatReader>,
    track: u32,
    info: FlacInfo,
    stream_info: Box<[u8]>,
    sequence: u32,
}

impl FlacToMp4 {
    /// Opens a FLAC file; `from_ms` (milliseconds) starts the output at the frame that holds that moment, for seeking.
    /// Milliseconds, not samples: the page does not know the file's sample rate.
    pub fn open(path: &Path, from_ms: u64) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::from_source(Box::new(file), from_ms)
    }

    fn from_source(source: Box<dyn symphonia::core::io::MediaSource>, from_ms: u64) -> Result<Self> {
        let mss = MediaSourceStream::new(source, Default::default());
        let mut hint = Hint::new();
        hint.with_extension("flac");
        let mut reader = symphonia::default::get_probe()
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .context("reading the FLAC file")?;
        let track = reader.default_track(TrackType::Audio).context("no audio track")?;
        let total_samples = track.num_frames;
        let Some(params) = track.codec_params.as_ref().and_then(|p| p.audio()).cloned() else { bail!("not audio") };
        if params.codec != symphonia::core::codecs::audio::well_known::CODEC_ID_FLAC {
            bail!("not FLAC");
        }
        let stream_info = params.extra_data.clone().context("the file has no STREAMINFO")?;
        if stream_info.len() != 34 {
            bail!("STREAMINFO is {} bytes, not 34", stream_info.len());
        }
        let info = FlacInfo {
            sample_rate: params.sample_rate.context("the sample rate is unknown")?,
            channels: params.channels.as_ref().map(|c| c.count() as u16).context("the channels are unknown")?,
            bits_per_sample: params.bits_per_sample.context("the bit depth is unknown")? as u16,
            total_samples,
        };
        let track = track.id;
        let from = from_ms * u64::from(info.sample_rate) / 1000;
        if from > 0 {
            // The reader stops at the frame holding `from`; each fragment starts at its first frame's own time.
            reader
                .seek(SeekMode::Accurate, SeekTo::Timestamp { ts: Timestamp::from(from as i64), track_id: track })
                .context("seeking in the FLAC file")?;
        }
        Ok(FlacToMp4 { reader, track, info, stream_info, sequence: 0 })
    }

    pub fn info(&self) -> &FlacInfo {
        &self.info
    }

    /// The init segment: `ftyp` and `moov` (one audio track, FLAC, timescale = the sample rate).
    pub fn init(&self) -> Vec<u8> {
        let i = &self.info;
        let ftyp = boxed(b"ftyp", &[b"iso6".as_slice(), &0u32.to_be_bytes(), b"iso6", b"mp41"].concat());
        let unity: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];
        let matrix: Vec<u8> = unity.iter().flat_map(|v| v.to_be_bytes()).collect();

        let mut mvhd = Vec::new();
        mvhd.extend(0u32.to_be_bytes()); // creation
        mvhd.extend(0u32.to_be_bytes()); // modification
        mvhd.extend(i.sample_rate.to_be_bytes()); // timescale
        mvhd.extend(0u32.to_be_bytes()); // duration: in the fragments
        mvhd.extend(0x0001_0000u32.to_be_bytes()); // rate 1.0
        mvhd.extend(0x0100u16.to_be_bytes()); // volume 1.0
        mvhd.extend([0u8; 10]); // reserved
        mvhd.extend(&matrix);
        mvhd.extend([0u8; 24]); // pre_defined
        mvhd.extend(2u32.to_be_bytes()); // next track id
        let mvhd = full(b"mvhd", 0, 0, &mvhd);

        let mut tkhd = Vec::new();
        tkhd.extend(0u32.to_be_bytes()); // creation
        tkhd.extend(0u32.to_be_bytes()); // modification
        tkhd.extend(1u32.to_be_bytes()); // track id
        tkhd.extend(0u32.to_be_bytes()); // reserved
        tkhd.extend(0u32.to_be_bytes()); // duration
        tkhd.extend([0u8; 8]); // reserved
        tkhd.extend(0u16.to_be_bytes()); // layer
        tkhd.extend(0u16.to_be_bytes()); // alternate group
        tkhd.extend(0x0100u16.to_be_bytes()); // volume
        tkhd.extend(0u16.to_be_bytes()); // reserved
        tkhd.extend(&matrix);
        tkhd.extend(0u32.to_be_bytes()); // width
        tkhd.extend(0u32.to_be_bytes()); // height
        let tkhd = full(b"tkhd", 0, 0x000003, &tkhd); // enabled, in movie

        let mut mdhd = Vec::new();
        mdhd.extend(0u32.to_be_bytes());
        mdhd.extend(0u32.to_be_bytes());
        mdhd.extend(i.sample_rate.to_be_bytes());
        mdhd.extend(0u32.to_be_bytes());
        mdhd.extend(0x55c4u16.to_be_bytes()); // language "und"
        mdhd.extend(0u16.to_be_bytes());
        let mdhd = full(b"mdhd", 0, 0, &mdhd);

        let mut hdlr = Vec::new();
        hdlr.extend(0u32.to_be_bytes());
        hdlr.extend(b"soun");
        hdlr.extend([0u8; 12]);
        hdlr.extend(b"SoundHandler\0");
        let hdlr = full(b"hdlr", 0, 0, &hdlr);

        let smhd = full(b"smhd", 0, 0, &[0u8; 4]);
        let dref = full(b"dref", 0, 0, &[1u32.to_be_bytes().as_slice(), &full(b"url ", 0, 1, &[])].concat());
        let dinf = boxed(b"dinf", &dref);

        // The FLAC sample entry: an AudioSampleEntry named fLaC, holding dfLa with the STREAMINFO block (marked last).
        let mut dfla = vec![0x80, 0, 0, 34];
        dfla.extend_from_slice(&self.stream_info);
        let dfla = full(b"dfLa", 0, 0, &dfla);
        let mut entry = Vec::new();
        entry.extend([0u8; 6]); // reserved
        entry.extend(1u16.to_be_bytes()); // data reference index
        entry.extend([0u8; 8]); // reserved
        entry.extend(i.channels.to_be_bytes());
        entry.extend(i.bits_per_sample.to_be_bytes());
        entry.extend(0u16.to_be_bytes()); // pre_defined
        entry.extend(0u16.to_be_bytes()); // reserved
        // The 16.16 rate field cannot hold rates above 65535 Hz; the spec says 0 then (dfLa has the real rate).
        let rate = if i.sample_rate < 65536 { i.sample_rate << 16 } else { 0 };
        entry.extend(rate.to_be_bytes());
        entry.extend(&dfla);
        let flac = boxed(b"fLaC", &entry);
        let stsd = full(b"stsd", 0, 0, &[1u32.to_be_bytes().as_slice(), &flac].concat());
        let empty = |kind: &[u8; 4]| full(kind, 0, 0, &0u32.to_be_bytes());
        let stsz = full(b"stsz", 0, 0, &[0u32.to_be_bytes(), 0u32.to_be_bytes()].concat());
        let stbl = boxed(b"stbl", &[stsd, empty(b"stts"), empty(b"stsc"), stsz, empty(b"stco")].concat());
        let minf = boxed(b"minf", &[smhd, dinf, stbl].concat());
        let mdia = boxed(b"mdia", &[mdhd, hdlr, minf].concat());
        let trak = boxed(b"trak", &[tkhd, mdia].concat());

        let mut trex = Vec::new();
        trex.extend(1u32.to_be_bytes()); // track id
        trex.extend(1u32.to_be_bytes()); // sample description index
        trex.extend(0u32.to_be_bytes()); // default duration
        trex.extend(0u32.to_be_bytes()); // default size
        trex.extend(0u32.to_be_bytes()); // default flags
        let mvex = boxed(b"mvex", &full(b"trex", 0, 0, &trex));

        let moov = boxed(b"moov", &[mvhd, trak, mvex].concat());
        [ftyp, moov].concat()
    }

    /// The next fragment (`moof` + `mdat`) of about [`FRAGMENT_SECONDS`]; `None` at the end of the file.
    pub fn next_fragment(&mut self) -> Result<Option<Vec<u8>>> {
        let target = self.info.sample_rate as u64 * FRAGMENT_SECONDS;
        let mut samples: Vec<(u32, u32)> = Vec::new(); // (duration, size) per FLAC frame
        let mut data: Vec<u8> = Vec::new();
        let mut length = 0u64;
        let mut base = None;
        while length < target {
            let Some(packet) = self.reader.next_packet()? else { break };
            if packet.track_id != self.track {
                continue;
            }
            base.get_or_insert(u64::try_from(packet.pts.get()).unwrap_or(0));
            let dur = packet.dur.get() as u32;
            samples.push((dur, packet.data.len() as u32));
            data.extend_from_slice(&packet.data);
            length += dur as u64;
        }
        let Some(base) = base else { return Ok(None) };
        self.sequence += 1;
        Ok(Some(fragment(self.sequence, base, &samples, &data)))
    }
}

/// One `moof` + `mdat`: the samples' durations and sizes in `trun`, `data_offset` pointing past `moof` into `mdat`.
fn fragment(sequence: u32, base: u64, samples: &[(u32, u32)], data: &[u8]) -> Vec<u8> {
    let mfhd = full(b"mfhd", 0, 0, &sequence.to_be_bytes());
    let tfhd = full(b"tfhd", 0, 0x02_0000, &1u32.to_be_bytes()); // default-base-is-moof, track 1
    let tfdt = full(b"tfdt", 1, 0, &base.to_be_bytes());
    let trun_len = 8 + 4 + 4 + 4 + 8 * samples.len(); // header, version/flags, count, data offset, entries
    let traf_len = 8 + tfhd.len() + tfdt.len() + trun_len;
    let moof_len = 8 + mfhd.len() + traf_len;
    let mut trun = Vec::with_capacity(trun_len);
    trun.extend((samples.len() as u32).to_be_bytes());
    trun.extend(((moof_len + 8) as i32).to_be_bytes()); // first sample: just past mdat's header
    for (dur, size) in samples {
        trun.extend(dur.to_be_bytes());
        trun.extend(size.to_be_bytes());
    }
    let trun = full(b"trun", 0, 0x00_0301, &trun); // data offset, sample duration, sample size
    let traf = boxed(b"traf", &[tfhd, tfdt, trun].concat());
    let moof = boxed(b"moof", &[mfhd, traf].concat());
    debug_assert_eq!(moof.len(), moof_len);
    let mut out = moof;
    out.extend(((data.len() + 8) as u32).to_be_bytes());
    out.extend(b"mdat");
    out.extend_from_slice(data);
    out
}

/// How a track reaches the page's stream.
pub enum Kind {
    /// Repackaged into fragmented MP4 ([`FlacToMp4`]).
    Flac,
    /// As it is; the page trims it by its LAME header.
    Mp3,
}

/// A track's file on Cratarr's own mount and how it is streamed; `None` when the catalogue does not know the track
/// or its file is gone. Blocking (the catalogue).
/// Where a server's track is on disk, its kind (None: not one that plays here), and its length as the catalogue read
/// it (milliseconds).
pub fn locate(cat: &crate::catalogue::Catalogue, source: &str, item: &str) -> Option<(std::path::PathBuf, Option<Kind>, Option<u64>)> {
    let track = cat.track_of_source(source, item)?;
    let (folder, rel) = cat.file_of(track)?;
    let duration_ms = cat.duration_of(folder, &rel);
    let path = cat.folders().get(folder as usize)?.join(rel);
    let kind = match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("flac") => Some(Kind::Flac),
        Some("mp3") => Some(Kind::Mp3),
        _ => None,
    };
    Some((path, kind, duration_ms))
}

fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(8 + body.len());
    b.extend(((8 + body.len()) as u32).to_be_bytes());
    b.extend(kind);
    b.extend_from_slice(body);
    b
}

fn full(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(4 + body.len());
    b.push(version);
    b.extend(&flags.to_be_bytes()[1..]);
    b.extend_from_slice(body);
    boxed(kind, &b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    use std::io::Cursor;
    use symphonia::core::codecs::audio::AudioDecoderOptions;

    /// A FLAC file in memory: `n` samples of a deterministic, non-repeating signal in stereo, 16 bit, 44.1 kHz.
    fn flac(n: usize) -> (Vec<u8>, Vec<i32>) {
        flac_at(n, 44100)
    }

    /// As `flac`, at sample rate `rate`.
    fn flac_at(n: usize, rate: usize) -> (Vec<u8>, Vec<i32>) {
        let mut x: u32 = 2463534242;
        let mut interleaved = Vec::with_capacity(n * 2);
        for _ in 0..n {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            let v = ((x >> 16) as i32 - 32768) / 4;
            interleaved.push(v);
            interleaved.push(-v);
        }
        let config = flacenc::config::Encoder::default().into_verified().unwrap();
        let source = flacenc::source::MemSource::from_samples(&interleaved, 2, 16, rate);
        let stream = flacenc::encode_with_fixed_block_size(&config, source, 4096).unwrap();
        let mut sink = flacenc::bitsink::ByteSink::new();
        stream.write(&mut sink).unwrap();
        let mut bytes = sink.as_slice().to_vec();
        // flacenc records the short last frame as the minimum block size, which the format excludes; symphonia then
        // takes the stream for a variable-block one and cannot read it. Correct it as the converter does
        // (convert.rs, `set_block_sizes`): STREAMINFO's minimum (bytes 8-9) = its maximum (bytes 10-11).
        let max = [bytes[10], bytes[11]];
        bytes[8..10].copy_from_slice(&max);
        (bytes, interleaved)
    }

    /// Decodes fragmented MP4 back to interleaved samples with symphonia's own MP4 reader and FLAC decoder.
    fn decode_mp4(bytes: Vec<u8>) -> Vec<i32> {
        let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("mp4");
        let mut reader = symphonia::default::get_probe().probe(&hint, mss, FormatOptions::default(), MetadataOptions::default()).unwrap();
        let track = reader.default_track(TrackType::Audio).unwrap();
        let params = track.codec_params.as_ref().and_then(|p| p.audio()).cloned().unwrap();
        let id = track.id;
        let mut decoder = symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).unwrap();
        let mut out = Vec::new();
        while let Some(packet) = reader.next_packet().unwrap() {
            if packet.track_id != id {
                continue;
            }
            let decoded = decoder.decode(&packet).unwrap();
            let mut buf = vec![0i32; decoded.frames() * 2];
            decoded.copy_to_slice_interleaved(&mut buf);
            out.extend(buf.iter().map(|v| v >> 16)); // symphonia gives 16-bit samples scaled to i32
        }
        out
    }

    fn remux(bytes: &[u8], from_ms: u64) -> (Vec<u8>, FlacInfo, usize) {
        let mut r = FlacToMp4::from_source(Box::new(Cursor::new(bytes.to_vec())), from_ms).unwrap();
        let mut out = r.init();
        let mut fragments = 0;
        while let Some(f) = r.next_fragment().unwrap() {
            out.extend(f);
            fragments += 1;
        }
        (out, r.info().clone(), fragments)
    }

    /// Not a check: repackages real files for the gapless lab (tools/gapless-lab), so the lab measures this
    /// converter's own output. `CRATARR_REMUX="in.flac:out.mp4,..."`, run with `--ignored`.
    #[test]
    #[ignore]
    fn remux_files_for_the_lab() {
        let pairs = std::env::var("CRATARR_REMUX").unwrap_or_default();
        for pair in pairs.split(',').filter(|p| !p.is_empty()) {
            let (input, output) = pair.split_once(':').unwrap();
            let mut r = FlacToMp4::open(Path::new(input), 0).unwrap();
            let mut out = r.init();
            while let Some(f) = r.next_fragment().unwrap() {
                out.extend(f);
            }
            std::fs::write(output, out).unwrap();
        }
    }

    #[test]
    fn flac_repackaged_as_mp4_decodes_to_the_same_samples() {
        let (file, original) = flac(100_000);
        let (mp4, info, fragments) = remux(&file, 0);
        assert_eq!(info, FlacInfo { sample_rate: 44100, channels: 2, bits_per_sample: 16, total_samples: Some(100_000) });
        assert_eq!(fragments, 3, "about a second per fragment: 100,000 samples at 44.1 kHz");
        assert_eq!(decode_mp4(mp4), original, "every sample comes back unchanged");
    }

    #[test]
    fn a_fragment_holds_the_frames_as_they_are() {
        // The mdat payloads, joined, are the file's frames: the file without its header and metadata.
        let (file, _) = flac(20_000);
        let (mp4, _, _) = remux(&file, 0);
        let mut payload = Vec::new();
        let mut i = 0;
        while i + 8 <= mp4.len() {
            let len = u32::from_be_bytes(mp4[i..i + 4].try_into().unwrap()) as usize;
            if &mp4[i + 4..i + 8] == b"mdat" {
                payload.extend_from_slice(&mp4[i + 8..i + len]);
            }
            i += len;
        }
        assert!(file.ends_with(&payload), "the frames are copied byte for byte");
    }

    #[test]
    fn starting_later_begins_at_the_frame_holding_that_moment() {
        let (file, original) = flac(100_000);
        let (mp4, _, _) = remux(&file, 1_134);
        let decoded = decode_mp4(mp4);
        // 1.134 s is sample 50,009; fixed 4096-sample frames: it is in the frame starting at 49,152.
        assert_eq!(decoded, original[49_152 * 2..], "from the start of that frame to the end");
    }

    #[test]
    fn a_moment_is_found_at_the_files_own_sample_rate() {
        // At 22.05 kHz, 2 s is sample 44,100 (the frame from 40,960), not sample 88,200 as at 44.1 kHz.
        let (file, original) = flac_at(100_000, 22050);
        let (mp4, info, _) = remux(&file, 2_000);
        assert_eq!(info.sample_rate, 22050);
        assert_eq!(decode_mp4(mp4), original[40_960 * 2..]);
    }
}
