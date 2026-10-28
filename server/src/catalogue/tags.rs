//! Reading a file's tags and audio properties. Two layers are kept (decision 0008): each item's key as the format
//! stores it (ID3v2 "TPE1", Vorbis "ARTIST", …) and a normalised name in Picard's vocabulary ("artist"), with every
//! value as stored: nothing is split or merged here (see `normalise`). Cover art is not read.

/// Bumped when reading improves, so files that failed before are tried again (see `Catalogue::open`).
pub const READER_VERSION: u32 = 6;

/// What is wrong with a file that could still be read (a file that cannot be read at all is an error instead).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// A tag field is malformed and was left out; the rest was read.
    DamagedTag,
    /// The file has no tags at all.
    NoTags,
    /// No audio length could be read.
    NoLength,
}

impl Issue {
    pub fn code(self) -> &'static str {
        match self {
            Issue::DamagedTag => "damaged_tag",
            Issue::NoTags => "no_tags",
            Issue::NoLength => "no_length",
        }
    }
}

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
        _ => return None,
    })
}

/// Names for keys lofty has no item for, by the raw key's last part: Vorbis `ALBUMARTISTS`, MP4
/// `----:com.apple.iTunes:ALBUMARTISTS` (Picard writes the album's artists apart there, decision 0016); the gain tags
/// where lofty does not map them (Opus's `R128_*`, a lower-case MP4 freeform), decision 0035.
fn by_raw_key(raw: &str) -> Option<&'static str> {
    let last = raw.rsplit(':').next().unwrap_or(raw);
    const NAMED: [&str; 8] = [
        "albumartists",
        // A style (0.9.7), as Vorbis `STYLE`, ID3v2 `TXXX:STYLE` or an MP4 freeform keeps it.
        "style",
        "replaygain_track_gain",
        "replaygain_track_peak",
        "replaygain_album_gain",
        "replaygain_album_peak",
        "r128_track_gain",
        "r128_album_gain",
    ];
    NAMED.into_iter().find(|n| last.eq_ignore_ascii_case(n))
}

/// Read a file and say what, if anything, is wrong with it. The normal mode is tried first; if it fails, the relaxed
/// mode tells a damaged field (seen: a TIPL frame with a broken UTF-16 byte-order mark, which the normal mode gives
/// up the whole file for) from a file that cannot be read at all.
pub fn read(path: &Path) -> Result<(FileTags, Option<(Issue, String)>)> {
    read_tags(path).map(|(mut t, issue)| {
        // An MP3's length from its own info frame where it has one (0.9.14): lofty missed some, reading them as a
        // constant bitrate of their info frame's (32 kb/s: ten times their length).
        if t.format.as_deref() == Some("MP3")
            && let Some(ms) = crate::stream::mp3_info(path).and_then(|i| i.length_ms())
        {
            t.duration_ms = Some(ms);
        }
        (t, issue)
    })
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
            let raw = key.map_key(tag_type).map(str::to_string).unwrap_or_else(|| format!("{key:?}"));
            let name = normalised(key).or_else(|| by_raw_key(&raw));
            // ID3v2 keeps several values of one frame in one string, separated by NUL.
            for v in value.split('\0').map(str::trim).filter(|v| !v.is_empty()) {
                out.items.push((raw.clone(), name, v.to_string()));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
