//! The fixture library's parts that the areas' tests below the top reach (moved from `fixtures`, which keeps their names): a
//! generated tone as a WAV, and FX01 with its files tagged in production's shapes.

use std::path::Path;

use anyhow::Result;

const RATE: u32 = 44_100;

/// A tone (`seconds` long, its pitch by `n`) as a 16-bit stereo WAV.
pub(crate) fn wav(path: &Path, n: u32, seconds: u32) -> Result<()> {
    let frames = (RATE * seconds) as usize;
    let hz = 220.0 * (1.0 + f64::from(n) * 0.125);
    let mut data = Vec::with_capacity(frames * 4);
    for i in 0..frames {
        let s = ((i as f64 * hz * std::f64::consts::TAU / f64::from(RATE)).sin() * 9000.0) as i16;
        data.extend_from_slice(&s.to_le_bytes());
        data.extend_from_slice(&s.to_le_bytes());
    }
    let mut b = Vec::with_capacity(44 + data.len());
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(data.len() as u32).to_le_bytes());
    b.extend_from_slice(&data);
    std::fs::write(path, b)?;
    Ok(())
}

/// The folder of bob's own music in the tests of FX01 tagged (`tag_0_16_shapes`).
#[cfg(any(test, feature = "test-support"))]
pub const BOB_OWN: &str = "/own/bob";

/// Tests elsewhere (P2.5, P3): FX01's had records' files tagged in production's shapes, in its catalogue `cat`, opened with
/// `/music` and `BOB_OWN` in use; FX01's files name their album and album artist already. Tide Tables: tagged (MusicBrainz's id
/// for its album artist, FX01's), a label, and a track crediting a guest, Ferro Lane, credited nowhere else; Two Rooms: two album
/// artists in tag order, a label; Night Walks EP and Partial Shore: tagged by names; Undated: untagged (no id, no album artist:
/// its album and artist alone); Late Train OST: Various Artists, three tracks of three artists, its file gone come back and two
/// more; Signal: a copy in bob's own music, the same track. Blocking.
#[cfg(any(test, feature = "test-support"))]
pub fn tag_0_16_shapes(cat: &crate::catalogue::Catalogue) {
    let own = cat.folders().id_of(Path::new(BOB_OWN)).expect("bob's own music in use");
    let file = |folder: i64, rel: &str, track: i64| {
        format!(
            "INSERT INTO files (folder, rel, size, mtime, state, format, bitrate, duration_ms, track, bits, rate, came) VALUES ({folder}, '{rel}', 24000000, 1760000000, 1, 'FLAC', 900, 200000, {track}, 16, 44100, 1760000000);"
        )
    };
    let tag = |rels: &str, n: i64, name: &str, value: &str| {
        format!("INSERT INTO tags (folder, rel, n, raw, name, value) SELECT folder, rel, {n}, upper('{name}'), '{name}', '{value}' FROM files WHERE folder = 0 AND rel LIKE '{rels}';")
    };
    let late = |rel: &str| format!("Various Artists/Late Train OST (2005)/{rel}.flac");
    let mut sql = vec![
        format!("UPDATE files SET gone_at = NULL WHERE rel = '{}';", late("01 Platform")),
        file(0, &late("02 Ticket Hall"), 21),
        file(0, &late("03 Signal Box"), 22),
        "DELETE FROM tags WHERE rel = 'Kite Theory/Undated (no year)/01 Morning.flac';".to_string(),
    ];
    let albums: [(&str, &[(&str, &str)]); 7] = [
        ("Glass Harbour/Tide Tables (2012)/%", &[("artist", "Glass Harbour"), ("label", "Quiet Pier"), ("catalognumber", "QP 001")]),
        ("Noor Field/Two Rooms (2019)/%", &[("albumartist", "Orbit Lane"), ("artist", "Noor Field"), ("label", "Long Room"), ("catalognumber", "LR-2")]),
        ("Kite Theory/Night Walks EP (2020)/%", &[("artist", "Kite Theory")]),
        ("Kite Theory/Undated (no year)/%", &[("album", "Undated"), ("artist", "Kite Theory")]),
        ("Various Artists/Late Train OST (2005)/%", &[("compilation", "1")]),
        ("Noor Field/Signal (2021)/%", &[("artist", "Noor Field")]),
        ("Glass Harbour/Partial Shore (2011)/%", &[("artist", "Glass Harbour")]),
    ];
    for (rels, tags) in albums {
        sql.extend(tags.iter().zip(20..).map(|((name, value), n)| tag(rels, n, name, value)));
    }
    for rel in ["02 Ticket Hall", "03 Signal Box"] {
        sql.push(tag(&late(rel), 10, "album", "Late Train OST"));
        sql.push(tag(&late(rel), 11, "albumartist", "Various Artists"));
    }
    for (rel, artist) in [("01 Platform", "Glass Harbour"), ("02 Ticket Hall", "Orbit Lane"), ("03 Signal Box", "Tape Club")] {
        sql.push(tag(&late(rel), 30, "artist", artist));
    }
    for (n, artist) in [(30, "Glass Harbour"), (31, "Ferro Lane")] {
        sql.push(tag("Glass Harbour/Tide Tables (2012)/03 Spring Tide.flac", n, "artists", artist));
    }
    let signal = "Noor Field/Signal (2021)/01 Signal.flac";
    sql.push(file(own, signal, 28));
    sql.push(format!("INSERT INTO tags (folder, rel, n, raw, name, value) SELECT {own}, rel, n, raw, name, value FROM tags WHERE folder = 0 AND rel = '{signal}';"));
    cat.execute_for_tests(&sql.concat());
}
