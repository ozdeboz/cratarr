//! Reading a file's tags and audio properties. Two layers are kept (decision 0008): each item's key as the format
//! stores it (ID3v2 "TPE1", Vorbis "ARTIST", …) and a normalised name in Picard's vocabulary ("artist"), with every
//! value as stored: nothing is split or merged here (see `normalise`). Cover art is not read.

/// Bumped when reading improves, so files that failed before are tried again (see `Catalogue::open`). Reader 10 names every role
/// tag Picard writes and reads ID3v2's TIPL, TMCL and IPLS pairs itself (SCENARIOS-CRED CRED.4, T-CRED-25).
pub const READER_VERSION: u32 = 10;

/// What is wrong with a file that could still be read (a file that cannot be read at all is an error instead).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// A tag field is malformed and was left out; the rest was read.
    DamagedTag,
    /// The file has no tags at all.
    NoTags,
    /// No audio length could be read.
    NoLength,
    /// A FLAC's seek table sends a seek to the wrong place (2026-11-02, the owner's DARKSIDE "Nothing": every point at
    /// its first byte, so a far seek played from the start and the track ended early, the next playing).
    BadSeektable,
}

impl Issue {
    pub fn code(self) -> &'static str {
        match self {
            Issue::DamagedTag => "damaged_tag",
            Issue::NoTags => "no_tags",
            Issue::NoLength => "no_length",
            Issue::BadSeektable => "bad_seektable",
        }
    }
}

use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use lofty::config::{ParseOptions, ParsingMode};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::ItemKey;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileTags {
    /// "FLAC", "MP3", …
    pub format: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub duration_ms: Option<u64>,
    pub sample_rate_hz: Option<u32>,
    pub bit_depth: Option<u8>,
    pub channels: Option<u8>,
    /// (raw key, normalised name if known, value), in the file's order.
    pub items: Vec<(String, Option<&'static str>, String)>,
}

/// Picard-style name for the keys Cratarr understands; others keep only their raw key.
fn normalised(key: ItemKey) -> Option<&'static str> {
    Some(match key {
        ItemKey::TrackTitle => "title",
        ItemKey::TrackArtist => "artist",
        ItemKey::TrackArtists => "artists",
        ItemKey::AlbumTitle => "album",
        ItemKey::AlbumArtist => "albumartist",
        ItemKey::TrackNumber => "tracknumber",
        ItemKey::TrackTotal => "totaltracks",
        ItemKey::DiscNumber => "discnumber",
        ItemKey::DiscTotal => "totaldiscs",
        ItemKey::RecordingDate => "date",
        ItemKey::Year => "date",
        ItemKey::OriginalReleaseDate => "originaldate",
        ItemKey::ReleaseDate => "releasedate",
        ItemKey::Genre => "genre",
        // The library's words (0.9.7): a file's moods and grouping.
        ItemKey::Mood => "mood",
        ItemKey::ContentGroup => "grouping",
        ItemKey::Performer => "performer",
        ItemKey::Composer => "composer",
        // The role tags (CRED.4, T-CRED-25): each a credit of its file's track, in Picard's vocabulary. ID3v2's TIPL pairs
        // (producer, arranger, engineer, DJ-mix, mix) are read as pairs instead (`pairs`), so each keeps its role in the raw layer.
        ItemKey::Lyricist => "lyricist",
        ItemKey::Writer => "writer",
        ItemKey::Arranger => "arranger",
        ItemKey::Conductor => "conductor",
        ItemKey::Producer => "producer",
        ItemKey::Engineer => "engineer",
        ItemKey::MixEngineer => "mixer",
        ItemKey::MixDj => "djmixer",
        ItemKey::Remixer => "remixer",
        ItemKey::Label => "label",
        ItemKey::CatalogNumber => "catalognumber",
        ItemKey::Bpm | ItemKey::IntegerBpm => "bpm",
        ItemKey::ReplayGainTrackGain => "replaygain_track_gain",
        ItemKey::ReplayGainTrackPeak => "replaygain_track_peak",
        ItemKey::ReplayGainAlbumGain => "replaygain_album_gain",
        ItemKey::ReplayGainAlbumPeak => "replaygain_album_peak",
        ItemKey::MusicBrainzRecordingId => "musicbrainz_trackid",
        ItemKey::MusicBrainzReleaseId => "musicbrainz_albumid",
        ItemKey::MusicBrainzReleaseGroupId => "musicbrainz_releasegroupid",
        ItemKey::MusicBrainzArtistId => "musicbrainz_artistid",
        ItemKey::MusicBrainzReleaseArtistId => "musicbrainz_albumartistid",
        // The release type (0.13, decision 0092: written into files): Vorbis RELEASETYPE, ID3v2 TXXX:MusicBrainz Album Type.
        ItemKey::MusicBrainzReleaseType => "releasetype",
        _ => return None,
    })
}

/// Names for keys lofty has no item for, by the raw key's last part: Vorbis `ALBUMARTISTS`, MP4
/// `----:com.apple.iTunes:ALBUMARTISTS` (Picard writes the album's artists apart there, decision 0016); the gain tags
/// where lofty does not map them (Opus's `R128_*`, a lower-case MP4 freeform), decision 0035.
fn by_raw_key(raw: &str) -> Option<&'static str> {
    let last = raw.rsplit(':').next().unwrap_or(raw);
    const NAMED: [&str; 10] = [
        "albumartists",
        // Picard's writer where the format has no field for it: ID3v2's TXXX:WRITER, an MP4 freeform; Vorbis's WRITER or AUTHOR,
        // read by its own key (CRED.1).
        "writer",
        "author",
        // A style (0.9.7), as Vorbis `STYLE`, ID3v2 `TXXX:STYLE` or an MP4 freeform keeps it.
        "style",
        "replaygain_track_gain",
        "replaygain_track_peak",
        "replaygain_album_gain",
        "replaygain_album_peak",
        "r128_track_gain",
        "r128_album_gain",
    ];
    NAMED.into_iter().find(|n| last.eq_ignore_ascii_case(n)).map(|n| if n == "author" { "writer" } else { n })
}

/// The role pairs lofty makes items of from ID3v2's TIPL (its `TIPL_MAPPINGS`): read as pairs instead (`pair_name`).
const TIPL_ITEMS: [ItemKey; 5] = [ItemKey::Producer, ItemKey::Arranger, ItemKey::Engineer, ItemKey::MixDj, ItemKey::MixEngineer];

/// The name of an ID3v2 TIPL or TMCL pair, by its raw key (`TIPL:producer`, `TMCL:piano`; ID3v2.3's IPLS is TIPL): TIPL's involvement
/// roles by Picard's names (producer, arranger, engineer, DJ-mix as djmixer, mix as mixer), any other pair a performer on the
/// instrument its key names (TMCL's, and Picard's ID3v2.3 performer pairs in TIPL/IPLS), the instrument kept in the raw key. None for
/// any other key (T-CRED-25).
pub(super) fn pair_name(raw: &str) -> Option<&'static str> {
    let (frame, key) = raw.split_once(':')?;
    if !matches!(frame, "TIPL" | "TMCL") || key.trim().is_empty() {
        return None;
    }
    Some(match (frame, key.trim().to_lowercase().as_str()) {
        ("TIPL", "producer") => "producer",
        ("TIPL", "arranger") => "arranger",
        ("TIPL", "engineer") => "engineer",
        ("TIPL", "dj-mix") => "djmixer",
        ("TIPL", "mix") => "mixer",
        _ => "performer",
    })
}

/// Read a file and say what, if anything, is wrong with it. The normal mode is tried first; if it fails, the relaxed
/// mode tells a damaged field (seen: a TIPL frame with a broken UTF-16 byte-order mark, which the normal mode gives
/// up the whole file for) from a file that cannot be read at all.
pub fn read(path: &Path) -> Result<(FileTags, Option<(Issue, String)>)> {
    read_tags(path).map(|(mut t, mut issue)| {
        // An MP3's length from its own info frame where it has one (0.9.14): lofty missed some, reading them as a
        // constant bitrate of their info frame's (32 kb/s: ten times their length).
        if t.format.as_deref() == Some("MP3")
            && let Some(ms) = mp3_info(path).and_then(|i| i.length_ms())
        {
            t.duration_ms = Some(ms);
        }
        // A FLAC's seek table, where it has one, as the players follow it (0.14).
        if issue.is_none()
            && t.format.as_deref() == Some("FLAC")
            && let Some(why) = flac_seektable(path).and_then(|s| s.wrong())
        {
            issue = Some((Issue::BadSeektable, why));
        }
        (t, issue)
    })
}

/// A FLAC's seek table as written: where its block's header is and its body's length, its points (the sample, and its
/// frame's offset from the first frame; placeholders left out), the stream's total samples (0: not known) and its
/// audio's bytes. None where it has none, or is no FLAC.
#[derive(Debug, Clone, PartialEq)]
pub struct SeekTable {
    pub at: u64,
    pub len: u32,
    pub points: Vec<(u64, u64)>,
    pub total: u64,
    pub audio: u64,
}

impl SeekTable {
    /// What is wrong with it, where something is: a point after another at its byte or before it, or past the audio or
    /// the stream's end. A player trusts it, so a far seek lands wrong and the track ends early.
    pub fn wrong(&self) -> Option<String> {
        let said = "its seek table sends a far seek to the wrong place: the track ends early";
        let mut last: Option<(u64, u64)> = None;
        for &(sample, offset) in &self.points {
            let past = offset >= self.audio || (self.total > 0 && sample >= self.total);
            let behind = last.is_some_and(|(s, o)| sample <= s || offset <= o);
            if past || behind {
                return Some(said.to_string());
            }
            last = Some((sample, offset));
        }
        None
    }
}

/// The seek table of a FLAC file (`SeekTable`).
pub fn flac_seektable(path: &Path) -> Option<SeekTable> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic).ok()?;
    if &magic != b"fLaC" {
        return None;
    }
    let (mut table, mut total) = (None, 0u64);
    loop {
        let at = f.stream_position().ok()?;
        let mut h = [0u8; 4];
        f.read_exact(&mut h).ok()?;
        let (last, kind, len) = (h[0] & 0x80 != 0, h[0] & 0x7f, u32::from_be_bytes([0, h[1], h[2], h[3]]));
        match kind {
            0 | 3 => {
                let mut body = vec![0u8; len as usize];
                f.read_exact(&mut body).ok()?;
                if kind == 0 && body.len() >= 18 {
                    total = u64::from_be_bytes(body[10..18].try_into().ok()?) & ((1 << 36) - 1);
                } else if kind == 3 {
                    let word = |b: &[u8]| u64::from_be_bytes(b.try_into().unwrap_or([0; 8]));
                    let points = body.chunks_exact(18).map(|p| (word(&p[0..8]), word(&p[8..16]))).filter(|(s, _)| *s != u64::MAX).collect();
                    table = Some((at, len, points));
                }
            }
            127 => return None,
            _ => {
                f.seek(SeekFrom::Current(i64::from(len))).ok()?;
            }
        }
        if last {
            break;
        }
    }
    let audio = size.saturating_sub(f.stream_position().ok()?);
    let (at, len, points) = table?;
    Some(SeekTable { at, len, points, total, audio })
}

/// A FLAC's seek table made padding, in place: its block keeps its place and length, so no byte of the file moves and
/// its audio is untouched; players then find a time by the frames (as in a FLAC with no table).
pub fn flac_drop_seektable(path: &Path) -> Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let t = flac_seektable(path).context("it has no seek table")?;
    let mut f = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
    f.seek(SeekFrom::Start(t.at))?;
    let mut h = [0u8; 1];
    f.read_exact(&mut h)?;
    f.seek(SeekFrom::Start(t.at))?;
    f.write_all(&[(h[0] & 0x80) | 1])?;
    f.seek(SeekFrom::Start(t.at + 4))?;
    f.write_all(&vec![0u8; t.len as usize])?;
    f.sync_all()?;
    Ok(())
}

fn read_tags(path: &Path) -> Result<(FileTags, Option<(Issue, String)>)> {
    match read_with(path, ParsingMode::BestAttempt) {
        Ok(t) => {
            let issue = if t.items.is_empty() {
                Some((Issue::NoTags, "the file has no tags".to_string()))
            } else if t.duration_ms.is_none() {
                Some((Issue::NoLength, "no audio length could be read".to_string()))
            } else {
                None
            };
            Ok((t, issue))
        }
        Err(strict) => match read_with(path, ParsingMode::Relaxed) {
            Ok(t) => Ok((t, Some((Issue::DamagedTag, field_problem(&format!("{strict:#}")))))),
            // lofty can reject a whole MP3 for one frame in every mode (seen: TIPL); read the audio without its tags
            // and the text fields with the tolerant reader, skipping only what cannot be decoded.
            Err(_) => match (read_audio_only(path), super::id3::read(path)) {
                (Ok(mut t), Ok(Some(tolerant))) => {
                    t.items = tolerant.items;
                    let detail = if tolerant.skipped.is_empty() { field_problem(&format!("{strict:#}")) } else { tolerant.skipped.join("; ") };
                    Ok((t, Some((Issue::DamagedTag, detail))))
                }
                _ => Err(strict),
            },
        },
    }
}

/// The audio's properties only, with no tag parsed.
fn read_audio_only(path: &Path) -> Result<FileTags> {
    let file = Probe::open(path)?
        .options(ParseOptions::new().read_cover_art(false).read_tags(false).parsing_mode(ParsingMode::Relaxed))
        .guess_file_type()?
        .read()?;
    let props = file.properties();
    Ok(FileTags {
        format: Some(format!("{:?}", file.file_type()).to_uppercase()).map(|f| if f == "MPEG" { "MP3".into() } else { f }),
        bitrate_kbps: props.audio_bitrate().or(props.overall_bitrate()),
        duration_ms: Some(props.duration().as_millis() as u64).filter(|d| *d > 0),
        sample_rate_hz: props.sample_rate(),
        bit_depth: props.bit_depth(),
        channels: props.channels(),
        items: Vec::new(),
    })
}

/// The useful end of a parse error: "TIPL: failed to decode UTF-16 sequence: …" rather than the whole chain.
pub fn field_problem(error: &str) -> String {
    if let Some(i) = error.find("failed to parse frame '") {
        let rest = &error[i + "failed to parse frame '".len()..];
        if let Some((frame, why)) = rest.split_once("': ") {
            return format!("{frame}: {why}");
        }
    }
    error.rsplit(": failed to parse ").next().unwrap_or(error).to_string()
}

fn read_with(path: &Path, mode: ParsingMode) -> Result<FileTags> {
    let file = Probe::open(path)
        .with_context(|| format!("opening {}", path.display()))?
        .options(ParseOptions::new().read_cover_art(false).parsing_mode(mode))
        .guess_file_type()
        .context("guessing the file type")?
        .read()
        .context("cannot be read as audio")?;
    let props = file.properties();
    let mut out = FileTags {
        format: Some(format!("{:?}", file.file_type()).to_uppercase()).map(|f| if f == "MPEG" { "MP3".into() } else { f }),
        bitrate_kbps: props.audio_bitrate().or(props.overall_bitrate()),
        duration_ms: Some(props.duration().as_millis() as u64).filter(|d| *d > 0),
        sample_rate_hz: props.sample_rate(),
        bit_depth: props.bit_depth(),
        channels: props.channels(),
        items: Vec::new(),
    };
    for tag in file.tags() {
        let tag_type = tag.tag_type();
        for item in tag.items() {
            let Some(value) = item.value().text() else { continue };
            let key = item.key();
            // ID3v2's TIPL pairs lofty makes items of are read as pairs, with their role (`unmapped`): never twice, never as "TIPL".
            if tag_type == lofty::tag::TagType::Id3v2 && TIPL_ITEMS.contains(&key) {
                continue;
            }
            // Vorbis's WRITER and AUTHOR are one key to lofty, which names it AUTHOR: read by their own names instead (`unmapped`).
            if tag_type == lofty::tag::TagType::VorbisComments && key == ItemKey::Writer {
                continue;
            }
            let raw = key.map_key(tag_type).map(str::to_string).unwrap_or_else(|| format!("{key:?}"));
            // ID3v2's TEXT is the lyricist's frame ("Lyricist/Text writer"), whichever of the two keys lofty reads it as.
            let name = if tag_type == lofty::tag::TagType::Id3v2 && raw == "TEXT" { Some("lyricist") } else { normalised(key).or_else(|| by_raw_key(&raw)) };
            // ID3v2 keeps several values of one frame in one string, separated by NUL.
            for v in value.split('\0').map(str::trim).filter(|v| !v.is_empty()) {
                out.items.push((raw.clone(), name, v.to_string()));
            }
        }
    }
    // The fields lofty has no key for, which its generic tags leave out (0.13: STYLE was never read since lofty 0.25):
    // from the format's own tag, those Cratarr names. A failure here leaves the file read as above.
    for (raw, v) in unmapped(path, file.file_type(), mode) {
        if let Some(name) = pair_name(&raw).or_else(|| by_raw_key(&raw)) {
            for v in v.split('\0').map(str::trim).filter(|v| !v.is_empty()) {
                out.items.push((raw.clone(), Some(name), v.to_string()));
            }
        }
    }
    Ok(out)
}

/// A file's fields lofty has no key for, as (raw key, value): Vorbis comments' (FLAC, Ogg Vorbis, Opus) by their own
/// names; ID3v2's user text frames by their description and other text frames by their id; MP4's freeform atoms as
/// `----:mean:name`. Nothing where the file's own tag cannot be read.
fn unmapped(path: &Path, file_type: lofty::file::FileType, mode: ParsingMode) -> Vec<(String, String)> {
    use lofty::file::FileType;
    use lofty::tag::{SplitTag, TagType};
    let options = ParseOptions::new().read_cover_art(false).read_properties(false).parsing_mode(mode);
    let Ok(mut f) = std::fs::File::open(path) else { return Vec::new() };
    let vorbis = |vc: &lofty::ogg::tag::VorbisComments| -> Vec<(String, String)> {
        vc.items()
            .filter(|(k, _)| matches!(ItemKey::from_key(TagType::VorbisComments, k), None | Some(ItemKey::Writer)) && !k.starts_with("RATING:"))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    let mut read = || -> Option<Vec<(String, String)>> {
        Some(match file_type {
            FileType::Flac => vorbis(lofty::flac::FlacFile::read_from(&mut f, options).ok()?.vorbis_comments()?),
            FileType::Vorbis => vorbis(lofty::ogg::VorbisFile::read_from(&mut f, options).ok()?.vorbis_comments()),
            FileType::Opus => vorbis(lofty::ogg::OpusFile::read_from(&mut f, options).ok()?.vorbis_comments()),
            FileType::Mpeg => {
                use lofty::id3::v2::Frame;
                let tag = lofty::mpeg::MpegFile::read_from(&mut f, options).ok()?.id3v2()?.clone();
                // TIPL and TMCL (ID3v2.3's IPLS, which lofty reads as TIPL), each pair with its role or instrument in the raw key
                // (T-CRED-25): from the whole tag, as lofty's own items take some of TIPL's pairs out of what `split_tag` leaves.
                let pairs: Vec<(String, String)> = (&tag)
                    .into_iter()
                    .filter_map(|frame| match frame {
                        Frame::KeyValue(kv) if matches!(frame.id().as_str(), "TIPL" | "TMCL") => Some((frame.id().as_str().to_string(), kv)),
                        _ => None,
                    })
                    .flat_map(|(id, kv)| kv.key_value_pairs.iter().map(move |(k, v)| (format!("{id}:{}", k.trim()), v.to_string())).collect::<Vec<_>>())
                    .collect();
                let (rest, _) = tag.split_tag();
                let mut out: Vec<(String, String)> = (&*rest)
                    .into_iter()
                    .filter_map(|frame| match frame {
                        Frame::UserText(t) => Some((t.description.to_string(), t.content.to_string())),
                        Frame::Text(t) => Some((frame.id().as_str().to_string(), t.value.to_string())),
                        _ => None,
                    })
                    .collect();
                out.extend(pairs);
                out
            }
            FileType::Mp4 => {
                use lofty::mp4::{AtomData, AtomIdent};
                let ilst = lofty::mp4::Mp4File::read_from(&mut f, options).ok()?.ilst()?.clone();
                let (rest, _) = ilst.split_tag();
                (&*rest)
                    .into_iter()
                    .filter_map(|atom| match atom.ident() {
                        AtomIdent::Freeform { mean, name } => Some((format!("----:{mean}:{name}"), atom)),
                        _ => None,
                    })
                    .flat_map(|(raw, atom)| {
                        atom.data().filter_map(|d| if let AtomData::UTF8(v) = d { Some(v.clone()) } else { None }).map(move |v| (raw.clone(), v)).collect::<Vec<_>>()
                    })
                    .collect()
            }
            _ => return None,
        })
    };
    read().unwrap_or_default()
}

/// An MP3 frame header's length (bytes), samples, and sample rate; `None` where it is not a Layer III header.
pub fn mp3_frame(h: [u8; 4]) -> Option<(u64, u64, u64)> {
    const V1: [u64; 15] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];
    const V2: [u64; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    if h[0] != 0xFF || h[1] & 0xE0 != 0xE0 {
        return None;
    }
    // Version: 3 MPEG-1, 2 MPEG-2, 0 MPEG-2.5 (1 reserved); layer 1 is Layer III.
    let (version, layer) = ((h[1] >> 3) & 3, (h[1] >> 1) & 3);
    let (bi, si, pad) = (usize::from(h[2] >> 4), usize::from((h[2] >> 2) & 3), u64::from((h[2] >> 1) & 1));
    if version == 1 || layer != 1 || bi == 0 || bi == 15 || si == 3 {
        return None;
    }
    let kbps = if version == 3 { V1[bi] } else { V2[bi] };
    let rate = [44_100, 48_000, 32_000][si] >> if version == 3 { 0 } else if version == 2 { 1 } else { 2 };
    let samples = if version == 3 { 1152 } else { 576 };
    Some((samples / 8 * 1000 * kbps / rate + pad, samples, rate))
}

/// What an MP3's first frame says of the whole (its Xing/Info frame, where it has one): its frame count and its bytes,
/// each where given; the sample rate and samples a frame; and where its sound begins (after its ID3v2 tag).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mp3Info {
    pub start: u64,
    pub rate: u64,
    pub samples: u64,
    /// None: no info frame (a constant bitrate's, or a variable one whose length can only be guessed).
    pub frames: Option<u64>,
    pub bytes: Option<u64>,
    pub has_info: bool,
    /// Where in the file its info frame's frame count and byte count are (to set them; Health's fixes).
    pub frames_at: Option<u64>,
    pub bytes_at: Option<u64>,
    /// Its first frame's header and length (the info frame where it has one).
    pub header: [u8; 4],
    pub first_len: u64,
}

impl Mp3Info {
    /// The length its info frame gives (ms).
    pub fn length_ms(&self) -> Option<u64> {
        self.frames.map(|f| f * self.samples * 1000 / self.rate)
    }
}

/// Reads an MP3's first frame (0.9.14): its info frame found anywhere in it, as encoders place it (lofty looks at one
/// place only, and read some files as ten times their length: Night Lights, 2026-10-28).
pub fn mp3_info(path: &Path) -> Option<Mp3Info> {
    use std::io::{Read, Seek, SeekFrom};
    let start = crate::analysis::after_id3(path);
    let mut f = File::open(path).ok()?;
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut h = [0u8; 4];
    f.read_exact(&mut h).ok()?;
    let (len, samples, rate) = mp3_frame(h)?;
    let mut body = vec![0u8; usize::try_from(len.checked_sub(4)?).ok()?];
    f.read_exact(&mut body).ok()?;
    let at = body.windows(4).position(|w| w == b"Xing" || w == b"Info");
    let (mut frames, mut bytes, mut frames_at, mut bytes_at) = (None, None, None, None);
    if let Some(k) = at.filter(|k| k + 8 <= body.len()) {
        let word = |i: usize| body.get(i..i + 4).map(|b| u64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]])));
        let flags = word(k + 4)?;
        let mut p = k + 8;
        // Offsets in the file: the body starts after the 4-byte header.
        if flags & 1 != 0 {
            frames = word(p).filter(|n| *n > 0);
            frames_at = Some(start + 4 + p as u64);
            p += 4;
        }
        if flags & 2 != 0 {
            bytes = word(p).filter(|n| *n > 0);
            bytes_at = Some(start + 4 + p as u64);
        }
    }
    Some(Mp3Info { start, rate, samples, frames, bytes, has_info: at.is_some(), frames_at, bytes_at, header: h, first_len: len })
}

/// Small real tracks to write to, for the tests here, retag's and the covers'.
#[cfg(any(test, feature = "test-support"))]
pub mod fixtures {
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
    use super::*;

    #[test]
    fn a_style_is_read_from_the_format_s_own_tag() {
        use super::fixtures::*;
        use lofty::config::WriteOptions;
        let d = dir("tags-style");
        let f = flac(&d, "one");
        let mut file = lofty::flac::FlacFile::read_from(&mut std::fs::File::open(&f).unwrap(), ParseOptions::new()).unwrap();
        let mut vc = file.remove_vorbis_comments().unwrap_or_default();
        vc.push("STYLE".into(), "Dub Techno".into());
        vc.push("STYLE".into(), "Minimal".into());
        vc.push("SOMETHING_ELSE".into(), "x".into());
        vc.push("ARTIST".into(), "Ott".into());
        file.set_vorbis_comments(vc);
        file.save_to_path(&f, WriteOptions::default()).unwrap();
        let m = d.join("two.mp3");
        mp3(&m);
        let mut file = lofty::mpeg::MpegFile::read_from(&mut std::fs::File::open(&m).unwrap(), ParseOptions::new()).unwrap();
        let mut tag = file.remove_id3v2().unwrap_or_default();
        tag.insert_user_text("STYLE".into(), "Dub Techno".into());
        tag.set_artist("Ott".into());
        file.set_id3v2(tag);
        file.save_to_path(&m, WriteOptions::default()).unwrap();
        for (t, styles) in [(&f, vec!["Dub Techno", "Minimal"]), (&m, vec!["Dub Techno"])] {
            let (tags, issue) = read(t).unwrap();
            assert_eq!(issue, None, "{}", t.display());
            let got: Vec<&str> = tags.items.iter().filter(|(_, n, _)| *n == Some("style")).map(|(_, _, v)| v.as_str()).collect();
            assert_eq!(got, styles, "{}", t.display());
            assert!(tags.items.iter().any(|(_, n, v)| *n == Some("artist") && v == "Ott"), "the mapped fields as before: {}", t.display());
            assert!(!tags.items.iter().any(|(r, _, _)| r == "SOMETHING_ELSE"), "unnamed unknown fields stay out");
        }
        std::fs::remove_dir_all(&d).ok();
    }

    /// An ID3v2 frame as a tag body holds it: its id, its size (v2.3 plain, v2.4 syncsafe), no flags, its payload.
    fn id3_frame(version: u8, id: &str, payload: &[u8]) -> Vec<u8> {
        let mut f = id.as_bytes().to_vec();
        let n = payload.len() as u32;
        let size = if version == 4 { [(n >> 21) as u8 & 0x7f, (n >> 14) as u8 & 0x7f, (n >> 7) as u8 & 0x7f, n as u8 & 0x7f] } else { n.to_be_bytes() };
        f.extend_from_slice(&size);
        f.extend_from_slice(&[0, 0]);
        f.extend_from_slice(payload);
        f
    }

    /// A Latin-1 text payload, its values NUL-separated.
    fn latin1(values: &[&str]) -> Vec<u8> {
        let mut v = vec![0u8];
        v.extend(values.join("\0").chars().map(|c| c as u32 as u8));
        v
    }

    /// An MP3 whose ID3v2 tag is this body, written by hand (the shapes lofty will not write: a broken frame).
    fn mp3_with_tag(path: &Path, version: u8, body: &[u8]) {
        super::fixtures::mp3(path);
        let audio = std::fs::read(path).unwrap();
        let mut body = body.to_vec();
        body.extend([0u8; 64]);
        let n = body.len() as u32;
        let mut b = b"ID3".to_vec();
        b.extend_from_slice(&[version, 0, 0, (n >> 21) as u8 & 0x7f, (n >> 14) as u8 & 0x7f, (n >> 7) as u8 & 0x7f, n as u8 & 0x7f]);
        b.extend(body);
        b.extend(audio);
        std::fs::write(path, b).unwrap();
    }

    /// A file's named items, sorted: (raw key, name, value).
    fn named(t: &FileTags) -> Vec<(String, String, String)> {
        let mut v: Vec<(String, String, String)> = t.items.iter().filter_map(|(r, n, v)| n.map(|n| (r.clone(), n.to_string(), v.clone()))).collect();
        v.sort();
        v
    }

    fn has(got: &[(String, String, String)], raw: &str, name: &str, value: &str) -> bool {
        got.iter().any(|x| x == &(raw.to_string(), name.to_string(), value.to_string()))
    }

    /// CRED.4 (the reader's half), T-CRED-25: every role tag the reader now names is kept, its raw key and its name (decision 0008's
    /// two layers): composer, lyricist, writer, arranger, conductor, performer, producer, engineer, mixer, djmixer and remixer; ID3v2's
    /// TIPL and TMCL pairs (and ID3v2.3's IPLS, which lofty reads as TIPL) read by Cratarr's reader, each kept with its role in the raw
    /// layer (`TIPL:producer`, `TMCL:piano`, `TIPL:piano`); the tolerant reader's TCOM, TPE3, TEXT and a readable TMCL or IPLS beside a
    /// TIPL it can't decode. A FLAC (Vorbis), an ID3v2.4 MP3, a Picard ID3v2.3 MP3 and a tolerant-path MP3. Never a pair kept under the
    /// bare frame name, its role lost; never a role frame the tolerant reader can decode dropped. Red first: the reader names composer
    /// and performer alone (lofty's Producer, MixEngineer, MixDj, Engineer, Arranger, Lyricist, Writer, Conductor and Remixer fall to
    /// `None`), lofty has no item for TMCL, and the tolerant reader names TCOM alone and skips IPLS.
    #[test]
    fn the_files_role_tags_are_credits() {
        use super::fixtures::*;
        use lofty::config::WriteOptions;
        use lofty::id3::v2::{Frame, FrameId, KeyValueFrame, TextInformationFrame};
        use lofty::TextEncoding;
        use std::borrow::Cow;
        let d = dir("tags-roles");
        // The FLAC: Vorbis comments as Picard writes them.
        let f = flac(&d, "all");
        let mut file = lofty::flac::FlacFile::read_from(&mut std::fs::File::open(&f).unwrap(), ParseOptions::new()).unwrap();
        let mut vc = file.remove_vorbis_comments().unwrap_or_default();
        let vorbis = [
            ("COMPOSER", "composer", "Yann Tiersen"),
            ("LYRICIST", "lyricist", "Kerys Lae"),
            ("WRITER", "writer", "Kerys Lae"),
            ("ARRANGER", "arranger", "Kerys Lae"),
            ("CONDUCTOR", "conductor", "Loïc Brenn"),
            ("PERFORMER", "performer", "Yann Tiersen (piano)"),
            ("PRODUCER", "producer", "Gwen Arzel"),
            ("ENGINEER", "engineer", "Gwen Arzel"),
            ("MIXER", "mixer", "Gwen Arzel"),
            ("DJMIXER", "djmixer", "Gwen Arzel"),
            ("REMIXER", "remixer", "Gwen Arzel"),
        ];
        for (k, _, v) in vorbis {
            vc.push(k.into(), v.into());
        }
        file.set_vorbis_comments(vc);
        file.save_to_path(&f, WriteOptions::default()).unwrap();
        let got = named(&read(&f).unwrap().0);
        for (raw, name, value) in vorbis {
            assert!(has(&got, raw, name, value), "the FLAC's {raw} kept as {name}: {got:?}");
        }
        // (a) An ID3v2.4 MP3: TIPL's producer and mix, TMCL's piano, and TXXX:WRITER.
        let pairs = |p: &[(&str, &str)]| -> Cow<'static, [(Cow<'static, str>, Cow<'static, str>)]> {
            Cow::Owned(p.iter().map(|(k, v)| (Cow::Owned(k.to_string()), Cow::Owned(v.to_string()))).collect())
        };
        let m4 = d.join("v24.mp3");
        mp3(&m4);
        let mut file = lofty::mpeg::MpegFile::read_from(&mut std::fs::File::open(&m4).unwrap(), ParseOptions::new()).unwrap();
        let mut tag = file.remove_id3v2().unwrap_or_default();
        tag.insert(Frame::KeyValue(KeyValueFrame::new(FrameId::Valid("TIPL".into()), TextEncoding::UTF8, pairs(&[("producer", "Gwen Arzel"), ("mix", "Gwen Arzel")]))));
        tag.insert(Frame::KeyValue(KeyValueFrame::new(FrameId::Valid("TMCL".into()), TextEncoding::UTF8, pairs(&[("piano", "Yann Tiersen")]))));
        tag.set_artist("Yann Tiersen".into());
        tag.insert_user_text("WRITER".into(), "Kerys Lae".into());
        file.set_id3v2(tag);
        file.save_to_path(&m4, WriteOptions::default()).unwrap();
        let got = named(&read(&m4).unwrap().0);
        assert!(has(&got, "TIPL:producer", "producer", "Gwen Arzel"), "TIPL's producer pair, its role kept: {got:?}");
        assert!(has(&got, "TIPL:mix", "mixer", "Gwen Arzel"), "TIPL's mix pair: {got:?}");
        assert!(has(&got, "TMCL:piano", "performer", "Yann Tiersen"), "TMCL's piano pair, read by Cratarr's reader: {got:?}");
        assert!(got.iter().any(|(r, n, v)| r.ends_with("WRITER") && n == "writer" && v == "Kerys Lae"), "TXXX:WRITER: {got:?}");
        assert!(!got.iter().any(|(r, _, _)| r == "TIPL" || r == "TMCL" || r == "IPLS"), "never a pair under the bare frame name: {got:?}");
        assert_eq!(got.iter().filter(|(_, n, v)| n == "producer" && v == "Gwen Arzel").count(), 1, "one producer, not lofty's item beside the pair: {got:?}");
        // (e) Picard's ID3v2.3 MP3: TIPL written as an IPLS frame, which lofty reads as TIPL.
        let m3 = d.join("v23.mp3");
        mp3(&m3);
        let mut file = lofty::mpeg::MpegFile::read_from(&mut std::fs::File::open(&m3).unwrap(), ParseOptions::new()).unwrap();
        let mut tag = file.remove_id3v2().unwrap_or_default();
        tag.insert(Frame::KeyValue(KeyValueFrame::new(FrameId::Valid("TIPL".into()), TextEncoding::UTF16, pairs(&[("piano", "Yann Tiersen"), ("vocals", "Kerys Lae"), ("producer", "Gwen Arzel")]))));
        tag.insert(Frame::Text(TextInformationFrame::new(FrameId::Valid("TCOM".into()), TextEncoding::UTF16, "Yann Tiersen".to_string())));
        file.set_id3v2(tag);
        file.save_to_path(&m3, WriteOptions::default().use_id3v23(true)).unwrap();
        let got = named(&read(&m3).unwrap().0);
        assert!(has(&got, "TIPL:piano", "performer", "Yann Tiersen"), "IPLS's piano pair, as TIPL:piano: {got:?}");
        assert!(has(&got, "TIPL:vocals", "performer", "Kerys Lae"), "IPLS's vocals pair: {got:?}");
        assert!(has(&got, "TIPL:producer", "producer", "Gwen Arzel"), "IPLS's producer pair: {got:?}");
        assert!(has(&got, "TCOM", "composer", "Yann Tiersen"), "TCOM: {got:?}");
        assert!(!got.iter().any(|(r, _, _)| r == "TIPL" || r == "IPLS"), "never a pair under the bare frame name: {got:?}");
        // (f) The 175 files' shape: a TIPL lofty rejects (a broken UTF-16 byte-order mark), so the tolerant reader reads the rest:
        // TCOM, TPE3, TEXT and a readable TMCL; and its ID3v2.3 twin's readable IPLS beside the broken TIPL.
        let broken = [&[1u8, 0x41, 0x00][..], &[0x42, 0x00]].concat();
        let mut body = id3_frame(4, "TIPL", &broken);
        body.extend(id3_frame(4, "TCOM", &latin1(&["Yann Tiersen"])));
        body.extend(id3_frame(4, "TPE3", &latin1(&["Loïc Brenn"])));
        body.extend(id3_frame(4, "TEXT", &latin1(&["Kerys Lae"])));
        body.extend(id3_frame(4, "TMCL", &latin1(&["piano", "Yann Tiersen"])));
        let mt = d.join("tolerant.mp3");
        mp3_with_tag(&mt, 4, &body);
        let (t, issue) = read(&mt).unwrap();
        let got = named(&t);
        assert!(has(&got, "TCOM", "composer", "Yann Tiersen") && has(&got, "TPE3", "conductor", "Loïc Brenn") && has(&got, "TEXT", "lyricist", "Kerys Lae"), "{got:?}");
        assert!(has(&got, "TMCL:piano", "performer", "Yann Tiersen"), "the readable TMCL beside the broken TIPL: {got:?}");
        assert!(issue.is_some_and(|(i, why)| i == Issue::DamagedTag && why.contains("TIPL")), "the broken frame named in the file's issue, as today");
        let mut body = id3_frame(3, "TIPL", &broken);
        body.extend(id3_frame(3, "TCOM", &latin1(&["Yann Tiersen"])));
        body.extend(id3_frame(3, "IPLS", &latin1(&["piano", "Yann Tiersen", "producer", "Gwen Arzel"])));
        let got = super::super::id3::parse(3, 0, &body).items;
        assert!(got.contains(&("TIPL:piano".to_string(), Some("performer"), "Yann Tiersen".to_string())), "the tolerant reader reads IPLS: {got:?}");
        assert!(got.contains(&("TIPL:producer".to_string(), Some("producer"), "Gwen Arzel".to_string())), "{got:?}");
        assert!(got.contains(&("TCOM".to_string(), Some("composer"), "Yann Tiersen".to_string())), "{got:?}");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn gain_tags_get_their_names_however_the_format_keys_them() {
        assert_eq!(normalised(ItemKey::ReplayGainTrackGain), Some("replaygain_track_gain"));
        assert_eq!(by_raw_key("R128_TRACK_GAIN"), Some("r128_track_gain"));
        assert_eq!(by_raw_key("----:com.apple.iTunes:replaygain_album_peak"), Some("replaygain_album_peak"));
        assert_eq!(by_raw_key("TXXX:REPLAYGAIN_TRACK_GAIN"), Some("replaygain_track_gain"));
        assert_eq!(by_raw_key("COMMENT"), None);
    }

    #[test]
    fn field_problems_are_cut_to_the_frame_and_reason() {
        let e = "cannot be read as audio: failed to parse Mpeg file: failed to parse Id3v2 tag: failed to parse frame 'TIPL': failed to decode UTF-16 sequence: UTF-16 string has an invalid byte order mark";
        assert_eq!(field_problem(e), "TIPL: failed to decode UTF-16 sequence: UTF-16 string has an invalid byte order mark");
        assert_eq!(field_problem("cannot be read as audio: failed to parse Mpeg file: bad header"), "Mpeg file: bad header");
    }

    #[test]
    fn a_file_that_is_not_audio_is_an_error_not_a_panic() {
        let p = std::env::temp_dir().join(format!("cratarr-notaudio-{}.mp3", std::process::id()));
        std::fs::write(&p, b"this is not an mp3").unwrap();
        assert!(read(&p).is_err());
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn a_seek_table_at_the_first_byte_is_told_and_made_padding() {
        // The owner's DARKSIDE "Nothing": every point at byte 0, so a far seek ended the track.
        let dir = std::env::temp_dir().join(format!("cratarr-seektable-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = |name: &str, offsets: &[u64]| {
            let mut b = b"fLaC".to_vec();
            let mut info = vec![0u8; 34];
            // 44.1 kHz, 2 channels, 16 bits, 441000 samples.
            let x: u64 = (44100u64 << 44) | (1 << 41) | (15 << 36) | 441_000;
            info[10..18].copy_from_slice(&x.to_be_bytes());
            b.extend([0u8, 0, 0, 34]);
            b.extend(&info);
            b.extend([0x83u8, 0, 0, (offsets.len() * 18) as u8]);
            for (i, o) in offsets.iter().enumerate() {
                b.extend((i as u64 * 44_100).to_be_bytes());
                b.extend(o.to_be_bytes());
                b.extend(4096u16.to_be_bytes());
            }
            b.extend(vec![0xffu8; 10_000]);
            let path = dir.join(name);
            std::fs::write(&path, &b).unwrap();
            path
        };
        let good = file("good.flac", &[0, 1000, 2000]);
        assert_eq!(flac_seektable(&good).unwrap().wrong(), None);
        let bad = file("bad.flac", &[0, 0, 0]);
        assert!(flac_seektable(&bad).unwrap().wrong().is_some());
        assert!(flac_seektable(&file("past.flac", &[0, 1000, 20_000])).unwrap().wrong().is_some(), "past its audio");
        let size = std::fs::metadata(&bad).unwrap().len();
        flac_drop_seektable(&bad).unwrap();
        assert_eq!(flac_seektable(&bad), None, "padding now");
        assert_eq!(std::fs::metadata(&bad).unwrap().len(), size, "no byte moved");
        std::fs::remove_dir_all(&dir).ok();
    }
}
