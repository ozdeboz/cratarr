//! What a device's index reads of the served library (0101's point 16): every record with its copies, every artist, and each
//! song's own fields, in small rows, as the index packs them one at a time. `Library::index_rows` holds the library's lock for
//! its reads alone: a stream waits on that lock, and packing 34,000 tracks takes long enough to be heard.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use cratarr_rules::index::{Record, RecordCopy};

use super::copies::{Copy, Viewer, in_order};
use super::ids;
use super::library::Library;

/// A served song's own fields, in numbers where the index has them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Song {
    pub track: i64,
    pub record: i64,
    pub disc: Option<u32>,
    pub number: Option<u32>,
    pub title: String,
    pub artist: Option<String>,
    /// As Radio takes them: the song's in their order, then the album artist unless it is a compilation's.
    pub artists: Vec<i64>,
}

/// An artist as the served library names them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Named {
    pub id: i64,
    pub name: String,
    pub sort: String,
}

/// The served library, as the index reads it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rows {
    pub artists: Vec<Named>,
    pub records: Vec<Record>,
    pub songs: Vec<Song>,
}

fn number(id: &str, prefix: &str) -> Option<i64> {
    id.strip_prefix(prefix)?.parse().ok()
}

fn small(v: Option<i64>) -> Option<u32> {
    v.and_then(|v| u32::try_from(v).ok())
}

/// A list as the library keeps it in one column.
fn words(list: &str) -> Vec<String> {
    list.split('\u{1f}').filter(|w| !w.is_empty()).map(str::to_string).collect()
}

impl Library {
    /// Every record, artist and song served. A row that names what is not served leaves that name out, so one odd row does not
    /// cost a device its whole index. A record's copies come in the order they would play for anyone signed in: no account's
    /// order differs from another's since CR17, and a copy off keeps its place. Blocking.
    pub fn index_rows(&self) -> Result<Rows> {
        let db = self.db();
        let artists: Vec<Named> = {
            let mut st = db.prepare("SELECT id, name, order_name FROM artists")?;
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
            rows.filter_map(|r| r.map(|(id, name, sort)| Some(Named { id: number(&id, ids::ARTIST)?, name, sort })).transpose()).collect::<rusqlite::Result<_>>()?
        };
        let known: HashSet<i64> = artists.iter().map(|a| a.id).collect();
        let artist_of = |id: &str| number(id, ids::ARTIST).filter(|a| known.contains(a));
        let mut of_record: HashMap<String, Vec<i64>> = HashMap::new();
        {
            let mut st = db.prepare("SELECT album_id, artist_id FROM album_artists ORDER BY album_id, position")?;
            for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
                let (album, artist) = row?;
                of_record.entry(album).or_default().extend(artist_of(&artist));
            }
        }
        let mut copies: HashMap<String, Vec<(Copy, RecordCopy)>> = HashMap::new();
        {
            let mut st = db.prepare("SELECT album, folder, dir, format, min_kbps, size, added, tracks, bits, rate, short FROM copies ORDER BY album, folder, dir")?;
            let rows = st.query_map([], |r| {
                let (folder, dir, format): (i64, String, Option<String>) = (r.get(1)?, r.get(2)?, r.get(3)?);
                let (kbps, bits, rate): (Option<i64>, Option<i64>, Option<i64>) = (r.get(4)?, r.get(8)?, r.get(9)?);
                let copy = Copy { folder, dir: dir.clone(), kind: super::copies::Kind::Main, order: 0, format: format.clone(), min_kbps: kbps, bits, rate, short: r.get(10)?, added: r.get(6)? };
                let row = RecordCopy {
                    library: folder,
                    dir,
                    format,
                    min_kbps: small(kbps),
                    bits: small(bits),
                    rate: small(rate),
                    size: r.get::<_, Option<i64>>(5)?.and_then(|s| u64::try_from(s).ok()),
                    added: r.get(6)?,
                    tracks: small(r.get(7)?).unwrap_or(0),
                };
                Ok((r.get::<_, String>(0)?, copy, row))
            })?;
            for row in rows {
                let (album, copy, row) = row?;
                copies.entry(album).or_default().push((copy, row));
            }
        }
        let mut records: Vec<Record> = Vec::new();
        {
            let mut st = db.prepare("SELECT id, name, order_name, artist, year, original_year, created, release_type, secondary, genres, compilation, whole_tracks FROM albums")?;
            let rows = st.query_map([], |r| {
                let id: String = r.get(0)?;
                let record = Record {
                    id: number(&id, ids::ALBUM).unwrap_or(0),
                    title: r.get(1)?,
                    sort: r.get(2)?,
                    artist: r.get(3)?,
                    year: small(r.get(4)?),
                    original_year: small(r.get(5)?),
                    added: r.get(6)?,
                    kind: r.get(7)?,
                    secondary: words(&r.get::<_, String>(8)?),
                    genres: words(&r.get::<_, String>(9)?),
                    compilation: r.get(10)?,
                    whole_tracks: small(r.get(11)?),
                    ..Record::default()
                };
                Ok((id, record))
            })?;
            for row in rows {
                let (id, mut record) = row?;
                if record.id != 0 {
                    record.artist_ids = of_record.remove(&id).unwrap_or_default();
                    records.push(record);
                }
            }
        }
        let held: HashSet<i64> = records.iter().map(|r| r.id).collect();
        let mut of_song: HashMap<String, Vec<i64>> = HashMap::new();
        {
            let mut st = db.prepare("SELECT song_id, artist_id FROM song_artists ORDER BY song_id, position")?;
            for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
                let (song, artist) = row?;
                of_song.entry(song).or_default().extend(artist_of(&artist));
            }
        }
        let mut songs: Vec<Song> = Vec::new();
        {
            let query = format!(
                "SELECT s.id, s.album_id, s.disc_no, s.track_no, s.title, s.artist, {} FROM songs s LEFT JOIN albums a ON a.id = s.album_id",
                crate::app::radio::ALBUM_ARTIST_HEARD
            );
            let mut st = db.prepare(&query)?;
            let rows = st.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    (small(r.get(2)?), small(r.get(3)?)),
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            })?;
            for row in rows {
                let (id, album, (disc, number_of), title, artist, album_artist) = row?;
                let (Some(track), Some(record)) = (ids::track_of(&id), number(&album, ids::ALBUM).filter(|r| held.contains(r))) else { continue };
                let mut artists = of_song.remove(&id).unwrap_or_default();
                artists.extend(album_artist.as_deref().and_then(artist_of));
                songs.push(Song { track, record, disc, number: number_of, title, artist, artists });
            }
        }
        drop(db);
        for record in &mut records {
            let mut read = copies.remove(&ids::record(record.id)).unwrap_or_default();
            read.iter_mut().for_each(|(c, _)| (c.kind, c.order) = self.kind_of(c.folder));
            let of: Vec<Copy> = read.iter().map(|(c, _)| c.clone()).collect();
            record.copies = in_order(&of, Some(&Viewer::default())).iter().filter_map(|c| read.iter().find(|(held, _)| held == c).map(|(_, row)| row.clone())).collect();
        }
        Ok(Rows { artists, records, songs })
    }
}
