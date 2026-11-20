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

use anyhow::{Result, bail};

use crate::convert;
pub use crate::opus_copy_base::{BITRATE, COPIES, copy, copy_at, crc32};
use crate::opus_copy_base::{Encoder, FRAME, PAGE_PACKETS, RATE, drain_encoder};

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

#[cfg(any(test, feature = "test-support"))]
pub(crate) mod tests {
    // Shared with the tests of the crates above (test-support): the helpers only its own tests call are idle there.
    #![cfg_attr(not(test), allow(dead_code, unused_imports))]
    use super::*;
    pub(crate) use crate::opus_copy_base::tests::wav;
    use crate::opus_copy_base::{BOS, CONTINUED, EOS, Ogg};

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
