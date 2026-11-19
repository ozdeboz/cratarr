//! A tolerant ID3v2 reader for the text fields, used only when the main reader (lofty) gives up on a whole MP3
//! because of one malformed frame (seen: 175 files with a TIPL frame whose UTF-16 byte-order mark is invalid,
//! rejected by lofty 0.25 in every parsing mode while ffprobe reads them fine). It reads text frames (T***, TXXX),
//! skips any frame it cannot decode and names it, and never reads pictures.

use std::io::Read;
use std::path::Path;

/// The largest tag read; bigger tags (huge embedded art) are cut off there, which only loses frames past it.
const MAX_TAG: usize = 4 * 1024 * 1024;

#[derive(Debug, Default, PartialEq)]
pub struct Tolerant {
    /// (raw key, normalised name, value): "TPE1"/"artist", "TXXX:MusicBrainz Release Group Id"/…
    pub items: Vec<(String, Option<&'static str>, String)>,
    /// Frames that could not be decoded: "TIPL: invalid UTF-16 byte-order mark".
    pub skipped: Vec<String>,
}

fn name_of(frame: &str) -> Option<&'static str> {
    Some(match frame {
        "TIT2" => "title",
        "TPE1" => "artist",
        "TPE2" => "albumartist",
        "TALB" => "album",
        "TRCK" => "tracknumber",
        "TPOS" => "discnumber",
        "TDRC" | "TYER" => "date",
        "TDOR" | "TORY" => "originaldate",
        "TDRL" => "releasedate",
        "TCON" => "genre",
        "TCOM" => "composer",
        "TPUB" => "label",
        "TBPM" => "bpm",
        _ => return None,
    })
}

fn txxx_name(description: &str) -> Option<&'static str> {
    Some(match description.to_ascii_lowercase().as_str() {
        "artists" => "artists",
        "albumartists" => "albumartists",
        "musicbrainz release group id" => "musicbrainz_releasegroupid",
        "musicbrainz album id" => "musicbrainz_albumid",
        "musicbrainz artist id" => "musicbrainz_artistid",
        "musicbrainz album artist id" => "musicbrainz_albumartistid",
        "musicbrainz release track id" => "musicbrainz_releasetrackid",
        "originalyear" | "original year" => "originaldate",
        "catalognumber" => "catalognumber",
        _ => return None,
    })
}

fn syncsafe(b: &[u8]) -> usize {
    b.iter().fold(0usize, |acc, x| (acc << 7) | usize::from(x & 0x7f))
}

/// Decode an ID3v2 text payload (first byte = encoding) into its NUL-separated values.
fn decode(data: &[u8]) -> Result<Vec<String>, String> {
    let (&enc, body) = data.split_first().ok_or("empty frame")?;
    let text = match enc {
        0 => body.iter().map(|&b| b as char).collect::<String>(),
        3 => String::from_utf8(body.to_vec()).map_err(|_| "invalid UTF-8")?,
        1 | 2 => {
            if body.len() % 2 != 0 && body.last() != Some(&0) {
                return Err("odd-length UTF-16".into());
            }
            let mut big_endian = enc == 2;
            let mut units: Vec<u16> = Vec::with_capacity(body.len() / 2);
            let mut chunks = body.chunks_exact(2).peekable();
            // Encoding 1 starts with a byte-order mark (and again after each NUL in multi-value frames).
            let mut expect_bom = enc == 1;
            while let Some(c) = chunks.next() {
                if expect_bom {
                    expect_bom = false;
                    match c {
                        [0xff, 0xfe] => continue,
                        [0xfe, 0xff] => {
                            big_endian = true;
                            continue;
                        }
                        [0, 0] => {
                            units.push(0);
                            expect_bom = true;
                            continue;
                        }
                        _ => return Err("invalid UTF-16 byte-order mark".into()),
                    }
                }
                let u = if big_endian { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) };
                units.push(u);
                if u == 0 && enc == 1 && chunks.peek().is_some() {
                    expect_bom = true;
                }
            }
            String::from_utf16(&units).map_err(|_| "invalid UTF-16")?
        }
        _ => return Err(format!("unknown text encoding {enc}")),
    };
    Ok(text.split('\0').map(str::trim).filter(|v| !v.is_empty()).map(str::to_string).collect())
}

/// Read the ID3v2 tag at the start of `path`; `None` when there is none.
pub fn read(path: &Path) -> std::io::Result<Option<Tolerant>> {
    let mut f = std::fs::File::open(path)?;
    let mut head = [0u8; 10];
    if f.read_exact(&mut head).is_err() || &head[..3] != b"ID3" {
        return Ok(None);
    }
    let size = syncsafe(&head[6..10]).min(MAX_TAG);
    let mut tag = vec![0u8; size];
    let got = f.read(&mut tag)?;
    tag.truncate(got);
    Ok(Some(parse(head[3], head[5], &tag)))
}

/// The frames of a tag body. `version` is the major version (3 or 4); `flags` the header flags.
pub fn parse(version: u8, flags: u8, body: &[u8]) -> Tolerant {
    let mut out = Tolerant::default();
    if !(3..=4).contains(&version) {
        out.skipped.push(format!("ID3v2.{version} is not supported"));
        return out;
    }
    // Whole-tag unsynchronisation (v2.3): undo FF 00 → FF.
    let owned;
    let mut body = body;
    if flags & 0x80 != 0 && version == 3 {
        let mut v = Vec::with_capacity(body.len());
        let mut i = 0;
        while i < body.len() {
            v.push(body[i]);
            if body[i] == 0xff && body.get(i + 1) == Some(&0) {
                i += 1;
            }
            i += 1;
        }
        owned = v;
        body = &owned;
    }
    let mut pos = 0;
    // Skip an extended header.
    if flags & 0x40 != 0 && body.len() >= 4 {
        let n = if version == 4 { syncsafe(&body[..4]) } else { u32::from_be_bytes([body[0], body[1], body[2], body[3]]) as usize + 4 };
        pos = n.min(body.len());
    }
    while pos + 10 <= body.len() {
        let id = &body[pos..pos + 4];
        if id[0] == 0 {
            break; // padding
        }
        let size = if version == 4 {
            syncsafe(&body[pos + 4..pos + 8])
        } else {
            u32::from_be_bytes([body[pos + 4], body[pos + 5], body[pos + 6], body[pos + 7]]) as usize
        };
        let frame_flags = body[pos + 9];
        let start = pos + 10;
        let end = start.saturating_add(size).min(body.len());
        pos = start.saturating_add(size);
        let Ok(id) = std::str::from_utf8(id) else { break };
        if !id.starts_with('T') {
            continue; // pictures, comments, binary frames: not needed
        }
        // Compressed, encrypted or per-frame unsynchronised (v2.4) frames are skipped rather than guessed at.
        let odd = if version == 4 { frame_flags & 0x0f != 0 } else { frame_flags & 0xc0 != 0 };
        if odd {
            out.skipped.push(format!("{id}: compressed or encoded frame"));
            continue;
        }
        match decode(&body[start..end]) {
            Err(why) => out.skipped.push(format!("{id}: {why}")),
            Ok(values) if id == "TXXX" => {
                let mut it = values.into_iter();
                let Some(desc) = it.next() else { continue };
                let name = txxx_name(&desc);
                for v in it {
                    out.items.push((format!("TXXX:{desc}"), name, v));
                }
            }
            Ok(values) => {
                for v in values {
                    out.items.push((id.to_string(), name_of(id), v));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(id: &str, payload: &[u8]) -> Vec<u8> {
        let mut f = id.as_bytes().to_vec();
        f.extend_from_slice(&(payload.len() as u32).to_be_bytes()); // v2.3 sizes are plain
        f.extend_from_slice(&[0, 0]);
        f.extend_from_slice(payload);
        f
    }

    fn utf16le(enc_bom: bool, s: &str) -> Vec<u8> {
        let mut v = vec![1u8];
        if enc_bom {
            v.extend_from_slice(&[0xff, 0xfe]);
        }
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }

    #[test]
    fn a_bad_frame_is_skipped_and_named_the_rest_is_read() {
        let mut body = frame("TIT2", &[&[0u8][..], b"HS"].concat());
        // A TIPL whose UTF-16 lacks a valid byte-order mark, as in the reference library.
        body.extend(frame("TIPL", &[&[1u8, 0x41, 0x00][..], &[0x42, 0x00]].concat()));
        body.extend(frame("TPE1", &utf16le(true, "André Bratten")));
        body.extend(frame("TXXX", &[&[3u8][..], b"MusicBrainz Release Group Id\0abc-123"].concat()));
        body.extend([0u8; 16]); // padding
        let t = parse(3, 0, &body);
        assert_eq!(
            t.items,
            vec![
                ("TIT2".to_string(), Some("title"), "HS".to_string()),
                ("TPE1".to_string(), Some("artist"), "André Bratten".to_string()),
                ("TXXX:MusicBrainz Release Group Id".to_string(), Some("musicbrainz_releasegroupid"), "abc-123".to_string()),
            ]
        );
        assert_eq!(t.skipped, vec!["TIPL: invalid UTF-16 byte-order mark".to_string()]);
    }

    #[test]
    fn multiple_values_and_big_endian_are_read() {
        let mut v = vec![1u8, 0xfe, 0xff];
        for u in "A".encode_utf16() {
            v.extend_from_slice(&u.to_be_bytes());
        }
        v.extend_from_slice(&[0, 0, 0xfe, 0xff]);
        for u in "B".encode_utf16() {
            v.extend_from_slice(&u.to_be_bytes());
        }
        assert_eq!(decode(&v).unwrap(), vec!["A", "B"]);
        assert_eq!(decode(&[&[0u8][..], b"Rock\0Pop"].concat()).unwrap(), vec!["Rock", "Pop"]);
    }

    #[test]
    fn version_4_sizes_are_syncsafe_and_odd_frames_skipped() {
        let mut f = b"TALB".to_vec();
        f.extend_from_slice(&[0, 0, 0, 5]);
        f.extend_from_slice(&[0, 0]);
        f.extend_from_slice(&[3, b'A', b'l', b'b', b'.']);
        let mut c = b"TCON".to_vec();
        c.extend_from_slice(&[0, 0, 0, 2, 0, 0x08, 3, b'x']); // compressed flag
        f.extend(c);
        let t = parse(4, 0, &f);
        assert_eq!(t.items, vec![("TALB".to_string(), Some("album"), "Alb.".to_string())]);
        assert_eq!(t.skipped, vec!["TCON: compressed or encoded frame".to_string()]);
        assert_eq!(syncsafe(&[0, 0, 0x02, 0x01]), 257);
    }

    #[test]
    fn garbage_never_panics() {
        for len in 0..64 {
            let junk: Vec<u8> = (0..len).map(|i| (i * 37 % 251) as u8).collect();
            let _ = parse(3, 0xc0, &junk);
            let _ = parse(4, 0x40, &junk);
        }
    }
}
