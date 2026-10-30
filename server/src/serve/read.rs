//! Reading the library as the API shows it (decision 0030). Lists are read in two phases, Navidrome's pattern
//! (`persistence/sql_search.go`, translated): the page's row numbers first, over an index and without joins, then the
//! details of just those rows. User data (stars, plays, ratings) is added by the caller (`annotate`).

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{Connection, params_from_iter};

use super::api::{Album, Artist, ArtistRef, GenreRef, ReplayGain, Song, content_type, iso};
use super::library::{Library, fts_query};

const SONG_COLS: &str = "s.n, s.id, s.folder, s.path, s.album_id, s.title, s.artist, s.album, s.album_artist, s.track_no, s.disc_no,
  s.year, s.genre, s.bpm, s.duration_ms, s.size, s.suffix, s.kbps, s.created, s.mbid, s.cover,
  s.rg_track_gain, s.rg_track_peak, s.rg_album_gain, s.rg_album_peak, s.genres";
const ALBUM_COLS: &str = "a.n, a.id, a.name, a.artist, a.artist_id, a.year, a.genre, a.genres, a.song_count, a.duration_ms,
  a.created, a.mbid, a.compilation, a.cover, a.release_type, a.type_from, a.secondary";

/// An album's length in milliseconds, for a query over `albums a`: its songs' lengths summed, NULL when any song's is
/// unknown or nothing (`albums.duration_ms` counts those as nothing), as `source::total_secs`. Over the songs' album
/// index, in the query that reads the album's format and bitrate.
const ALBUM_LENGTH: &str = "(SELECT CASE WHEN count(*) = count(nullif(s.duration_ms, 0)) THEN sum(s.duration_ms) END FROM songs s WHERE s.album_id = a.id)";

/// An album as Classical and Other versions read it (0.10, `Library::work_albums`): its id and Music-form id (what its
/// tags and marks are kept under), name, artist, year, MusicBrainz release, its files' genres, its album artists.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkAlbum {
    pub id: String,
    pub base: String,
    pub name: String,
    pub artist: String,
    pub year: Option<u32>,
    pub release: Option<String>,
    pub genres: Vec<String>,
    pub artists: Vec<WorkArtist>,
}

/// One of an album's album artists: their library id, name, and MusicBrainz id where the files carry one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkArtist {
    pub id: String,
    pub name: String,
    pub mbid: Option<String>,
}

/// A song as Other versions reads it (0.10, `Library::work_songs`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkSong {
    pub id: String,
    pub album: String,
    /// Its MusicBrainz recording (the files' `musicbrainz_trackid`).
    pub recording: Option<String>,
    pub disc: Option<u32>,
    pub track: Option<u32>,
    pub title: String,
    pub artist: String,
    pub year: Option<u32>,
}

/// A song in Cratarr's ids, with its album's name and album artist (see `Library::song_ids`).
#[allow(dead_code)] // for the pushed player state (decision 0031), which comes next
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SongIds {
    pub song_id: String,
    pub album_id: String,
    pub album: String,
    pub album_artist: String,
}

/// How album lists are ordered (`getAlbumList2`'s types that the library alone decides).
#[derive(Debug, Clone, PartialEq)]
pub enum AlbumOrder {
    Newest,
    ByName,
    ByArtist,
    Random,
    ByYear { from: u32, to: u32 },
    ByGenre(String),
}

/// A song of the genre in parameter `n`: any of its genres, whole, regardless of capitals (as albums are found).
fn any_genre(n: usize) -> String {
    format!("('\u{1f}' || genres || '\u{1f}') LIKE ('%\u{1f}' || ?{n} || '\u{1f}%')")
}

fn song_of(r: &rusqlite::Row) -> rusqlite::Result<Song> {
    let suffix: Option<String> = r.get(16)?;
    let suffix = suffix.unwrap_or_default();
    let created: Option<i64> = r.get(18)?;
    let genre: Option<String> = r.get(12)?;
    let album_id: String = r.get(4)?;
    Ok(Song {
        id: r.get(1)?,
        parent: album_id.clone(),
        is_dir: false,
        title: r.get(5)?,
        album: r.get(7)?,
        artist: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
        track: r.get(9)?,
        year: r.get(11)?,
        // Every genre (OpenSubsonic's list); the one-genre field has the first.
        genres: r.get::<_, String>(25)?.split('\u{1f}').filter(|g| !g.is_empty()).map(|g| GenreRef { name: g.into() }).collect(),
        genre,
        cover_art: r.get::<_, Option<String>>(20)?.unwrap_or_else(|| album_id.clone()),
        size: r.get::<_, i64>(15)?.max(0) as u64,
        content_type: content_type(&suffix).to_string(),
        duration: r.get::<_, Option<i64>>(14)?.map_or(0, |d| (d.max(0) as u64 + 500) / 1000),
        bit_rate: r.get(17)?,
        path: r.get(3)?,
        disc_number: r.get(10)?,
        created: created.map(iso),
        album_id,
        bpm: r.get(13)?,
        music_brainz_id: r.get(19)?,
        display_album_artist: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
        replay_gain: ReplayGain::of(r.get(21)?, r.get(22)?, r.get(23)?, r.get(24)?),
        suffix,
        kind: "music",
        media_type: "song",
        ..Song::default()
    })
}

fn album_of(r: &rusqlite::Row) -> rusqlite::Result<Album> {
    let created: Option<i64> = r.get(10)?;
    let genres: String = r.get(7)?;
    let artist: String = r.get(3)?;
    let id: String = r.get(1)?;
    let release = release_of(r.get(14)?, r.get(15)?, r.get::<_, Option<String>>(16)?.as_deref());
    Ok(Album {
        cover_art: r.get::<_, Option<String>>(13)?.unwrap_or_else(|| id.clone()),
        id,
        name: r.get(2)?,
        display_artist: artist.clone(),
        artist,
        artist_id: r.get(4)?,
        year: r.get(5)?,
        genre: r.get(6)?,
        genres: genres.split('\u{1f}').filter(|g| !g.is_empty()).map(|g| GenreRef { name: g.into() }).collect(),
        song_count: r.get::<_, i64>(8)?.max(0) as u32,
        duration: (r.get::<_, i64>(9)?.max(0) as u64 + 500) / 1000,
        created: created.map(iso),
        music_brainz_id: r.get(11)?,
        is_compilation: r.get(12)?,
        release_types: release.kind.iter().chain(&release.secondary).map(|w| super::library::type_label(w)).collect(),
        release,
        ..Album::default()
    })
}

/// An album's release type as kept (0.9.6).
fn release_of(kind: Option<String>, from: Option<String>, secondary: Option<&str>) -> crate::source::ReleaseType {
    crate::source::ReleaseType {
        kind,
        type_from: from,
        secondary: secondary.unwrap_or_default().split('\u{1f}').filter(|s| !s.is_empty()).map(str::to_string).collect(),
        ..Default::default()
    }
}

/// Rows by number, in the order given (the second phase).
fn by_numbers<T>(db: &Connection, table: &str, cols: &str, alias: &str, ns: &[i64], f: fn(&rusqlite::Row) -> rusqlite::Result<T>) -> Result<Vec<T>> {
    if ns.is_empty() {
        return Ok(Vec::new());
    }
    let marks = vec!["?"; ns.len()].join(",");
    let mut st = db.prepare(&format!("SELECT {cols} FROM {table} {alias} WHERE {alias}.n IN ({marks})"))?;
    let mut found: HashMap<i64, T> = HashMap::new();
    let rows = st.query_map(params_from_iter(ns.iter()), |r| Ok((r.get::<_, i64>(0)?, f(r)?)))?;
    for row in rows {
        let (n, v) = row?;
        found.insert(n, v);
    }
    Ok(ns.iter().filter_map(|n| found.remove(n)).collect())
}

/// The row numbers of a query's page (the first phase).
fn numbers(db: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<Vec<i64>> {
    let mut st = db.prepare_cached(sql)?;
    let rows = st.query_map(params, |r| r.get::<_, i64>(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// What `Library::find` found (decision 0072): each with why it was found, where it was not by its own name.
#[derive(Debug, Default)]
pub struct Found {
    pub albums: Vec<(Album, Hit)>,
    pub artists: Vec<(Artist, Hit)>,
    pub tracks: Vec<Song>,
    /// Genres and how many tracks they have.
    pub genres: Vec<(String, u32)>,
    /// How many of each match in all, beyond the `limit` shown.
    pub totals: Totals,
}

/// One track of the Library page's Tracks list (0.9.4): what its row shows. `plays` and `favorite` are the asking
/// account's, filled in by `app::tracks`; `folder` and `order_title` only order and filter it.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct TrackRow {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_no: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_no: Option<i64>,
    /// Seconds.
    pub duration: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i64>,
    /// When it came into the library (epoch seconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<i64>,
    pub plays: u32,
    pub favorite: bool,
    /// Its file's kind (`flac`, `mp3`…) and bitrate (kbps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i64>,
    /// Its tempo (BPM), its album's artist, and when the asking account last played it (epoch seconds) (0.9.7: the
    /// Tracks table's columns).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bpm: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_played: Option<i64>,
    #[serde(skip)]
    pub folder: i64,
    #[serde(skip)]
    pub order_title: String,
}

fn track_row(r: &rusqlite::Row) -> rusqlite::Result<TrackRow> {
    Ok(TrackRow {
        id: r.get(0)?,
        title: r.get(1)?,
        artist: r.get(2)?,
        album: r.get(3)?,
        album_id: r.get(4)?,
        track_no: r.get(5)?,
        disc_no: r.get(6)?,
        duration: r.get::<_, Option<i64>>(7)?.map_or(0, |d| (d.max(0) as u64 + 500) / 1000),
        year: r.get(8)?,
        added: r.get(9)?,
        plays: 0,
        favorite: false,
        format: r.get::<_, Option<String>>(10)?.filter(|s| !s.is_empty()),
        bitrate: r.get(11)?,
        folder: r.get(12)?,
        order_title: r.get(13)?,
        bpm: r.get::<_, Option<i64>>(14)?.filter(|b| *b > 0),
        album_artist: r.get::<_, Option<String>>(15)?.filter(|a| !a.is_empty()),
        last_played: None,
    })
}

const TRACK_ROW_COLS: &str = "s.id, s.title, coalesce(s.artist, ''), s.album, s.album_id, s.track_no, s.disc_no, s.duration_ms, s.year, s.created,
  s.suffix, s.kbps, s.folder, s.order_title, s.bpm, s.album_artist";

/// How many albums, artists, tracks and genres a search matches in all (decision 0072: a group's header counts them).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Totals {
    pub albums: usize,
    pub artists: usize,
    pub tracks: usize,
    pub genres: usize,
}

/// Why something was found: on its `name`, its `artist`, one of its `track`s or `album`s (`text`, and how many).
#[derive(Debug, Clone, Default)]
pub struct Hit {
    pub on: &'static str,
    pub text: String,
    pub count: Option<u32>,
}

/// The FTS5 query for the words, each with the index terms it meets with typos (0.9.7, `search::typo_terms`): the
/// terms of the songs', albums' and artists' indexes that begin with its first letter.
fn typo_fts_query(db: &Connection, words: &[String]) -> Result<Option<String>> {
    let mut typo = Vec::with_capacity(words.len());
    for w in words {
        let Some(first) = w.chars().next().filter(|_| crate::search::typo_budget(w) > 0) else {
            typo.push(Vec::new());
            continue;
        };
        let from = first.to_string();
        let to = char::from_u32(first as u32 + 1).map_or_else(|| format!("{first}\u{10ffff}"), |c| c.to_string());
        let mut terms: Vec<String> = Vec::new();
        for table in ["songs_vocab", "albums_vocab", "artists_vocab"] {
            let mut st = db.prepare_cached(&format!("SELECT term FROM {table} WHERE term >= ?1 AND term < ?2"))?;
            terms.extend(st.query_map([&from, &to], |r| r.get::<_, String>(0))?.filter_map(Result::ok));
        }
        terms.sort_unstable();
        terms.dedup();
        typo.push(crate::search::typo_terms(w, terms.iter().map(String::as_str)));
    }
    Ok(crate::search::fts_with(words, &typo))
}

/// Search asks the index for this many times what it shows (0.9.7), at most `WIDEST`, and ranks them before the cut.
const WIDER: usize = 4;
const WIDEST: usize = 200;

impl Library {
    /// Fills songs' artists and album artists (OpenSubsonic's lists).
    fn song_artists(&self, db: &Connection, songs: &mut [Song]) -> Result<()> {
        let mut st = db.prepare_cached("SELECT artist_id, name FROM song_artists WHERE song_id = ?1 ORDER BY position")?;
        let mut aa = db.prepare_cached("SELECT artist_id, name FROM album_artists WHERE album_id = ?1 ORDER BY position")?;
        let mut album_cache: HashMap<String, Vec<ArtistRef>> = HashMap::new();
        for s in songs.iter_mut() {
            s.artists = st.query_map([&s.id], |r| Ok(ArtistRef { id: r.get(0)?, name: r.get(1)? }))?.collect::<rusqlite::Result<_>>()?;
            s.display_artist = s.artist.clone();
            if let Some(first) = s.artists.first() {
                s.artist_id = first.id.clone();
            }
            if !album_cache.contains_key(&s.album_id) {
                let refs: Vec<ArtistRef> = aa.query_map([&s.album_id], |r| Ok(ArtistRef { id: r.get(0)?, name: r.get(1)? }))?.collect::<rusqlite::Result<_>>()?;
                album_cache.insert(s.album_id.clone(), refs);
            }
            s.album_artists = album_cache[&s.album_id].clone();
        }
        Ok(())
    }

    fn album_artists(&self, db: &Connection, albums: &mut [Album]) -> Result<()> {
        let mut aa = db.prepare_cached("SELECT artist_id, name FROM album_artists WHERE album_id = ?1 ORDER BY position")?;
        for a in albums.iter_mut() {
            a.artists = aa.query_map([&a.id], |r| Ok(ArtistRef { id: r.get(0)?, name: r.get(1)? }))?.collect::<rusqlite::Result<_>>()?;
        }
        Ok(())
    }

    /// A page of albums in one of the library's own orders; `folder` limits it to one music folder.
    pub fn album_list(&self, order: &AlbumOrder, size: usize, offset: usize, folder: Option<i64>) -> Result<Vec<Album>> {
        let db = self.db();
        let f = folder.map_or(String::new(), |f| format!(" AND folder = {f}"));
        let (filter, by): (String, &str) = match order {
            AlbumOrder::Newest => (String::new(), "created DESC, n"),
            AlbumOrder::ByName => (String::new(), "order_name, n"),
            AlbumOrder::ByArtist => (String::new(), "(SELECT order_name FROM artists WHERE artists.id = albums.artist_id), year, n"),
            AlbumOrder::Random => (String::new(), "random()"),
            AlbumOrder::ByYear { from, to } if from <= to => (format!(" AND year BETWEEN {from} AND {to}"), "year, order_name, n"),
            AlbumOrder::ByYear { from, to } => (format!(" AND year BETWEEN {to} AND {from}"), "year DESC, order_name, n"),
            AlbumOrder::ByGenre(_) => (" AND ('\u{1f}' || genres || '\u{1f}') LIKE ('%\u{1f}' || ?3 || '\u{1f}%')".into(), "order_name, n"),
        };
        let sql = format!("SELECT n FROM albums WHERE 1=1{f}{filter} ORDER BY {by} LIMIT ?1 OFFSET ?2");
        let ns = match order {
            AlbumOrder::ByGenre(g) => numbers(&db, &sql, &[&(size as i64), &(offset as i64), g])?,
            _ => numbers(&db, &sql, &[&(size as i64), &(offset as i64)])?,
        };
        let mut albums = by_numbers(&db, "albums", ALBUM_COLS, "a", &ns, album_of)?;
        self.album_artists(&db, &mut albums)?;
        Ok(albums)
    }

    /// Albums by id, in the order given; unknown ids are left out.
    pub fn albums_by_id(&self, ids: &[String]) -> Result<Vec<Album>> {
        let db = self.db();
        let mut ns = Vec::new();
        {
            let mut st = db.prepare_cached("SELECT n FROM albums WHERE id = ?1")?;
            for id in ids {
                if let Ok(n) = st.query_row([id], |r| r.get::<_, i64>(0)) {
                    ns.push(n);
                }
            }
        }
        let mut albums = by_numbers(&db, "albums", ALBUM_COLS, "a", &ns, album_of)?;
        self.album_artists(&db, &mut albums)?;
        Ok(albums)
    }

    /// Every song's id, in the library's own order (a play of all of it, app::play).
    pub fn all_song_ids(&self) -> Result<Vec<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT id FROM songs ORDER BY n")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Songs by id, in the order given; unknown ids are left out.
    pub fn songs_by_id(&self, ids: &[String]) -> Result<Vec<Song>> {
        let db = self.db();
        let mut ns = Vec::new();
        {
            let mut st = db.prepare_cached("SELECT n FROM songs WHERE id = ?1")?;
            for id in ids {
                if let Ok(n) = st.query_row([id], |r| r.get::<_, i64>(0)) {
                    ns.push(n);
                }
            }
        }
        let mut songs = by_numbers(&db, "songs", SONG_COLS, "s", &ns, song_of)?;
        self.song_artists(&db, &mut songs)?;
        Ok(songs)
    }

    /// An album with its songs in disc and track order.
    pub fn album(&self, id: &str) -> Result<Option<Album>> {
        let Some(mut album) = self.albums_by_id(&[id.to_string()])?.pop() else { return Ok(None) };
        let db = self.db();
        let ns = numbers(&db, "SELECT n FROM songs WHERE album_id = ?1 ORDER BY coalesce(disc_no, 1), track_no, rel", &[&id])?;
        let mut songs = by_numbers(&db, "songs", SONG_COLS, "s", &ns, song_of)?;
        self.song_artists(&db, &mut songs)?;
        album.song = Some(songs);
        Ok(Some(album))
    }

    /// Every artist credited as an album artist, by name order (`getArtists`).
    pub fn album_artists_index(&self) -> Result<Vec<Artist>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT id, name, mbid, album_count, order_name FROM artists WHERE album_count > 0 ORDER BY order_name, n",
        )?;
        let rows = st.query_map([], artist_of)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// An artist with their albums, newest release last.
    pub fn artist(&self, id: &str) -> Result<Option<Artist>> {
        let db = self.db();
        let artist = {
            let mut st = db.prepare_cached("SELECT id, name, mbid, album_count, order_name FROM artists WHERE id = ?1")?;
            let found = st.query_map([id], artist_of)?.next().transpose()?;
            found
        };
        let Some(mut artist) = artist else { return Ok(None) };
        let ns = numbers(
            &db,
            "SELECT a.n FROM album_artists x JOIN albums a ON a.id = x.album_id WHERE x.artist_id = ?1 ORDER BY a.year, a.order_name",
            &[&id],
        )?;
        let mut albums = by_numbers(&db, "albums", ALBUM_COLS, "a", &ns, album_of)?;
        self.album_artists(&db, &mut albums)?;
        artist.album = Some(albums);
        Ok(Some(artist))
    }

    /// An artist's work (the artist panel): the albums credited to them and the albums they only appear on (credited
    /// on a track), both newest first, and the songs credited to them or on their albums, album by album in disc and
    /// track order.
    pub fn artist_works(&self, artist_id: &str) -> Result<(Vec<Album>, Vec<Album>, Vec<Song>)> {
        let db = self.db();
        let own = numbers(
            &db,
            "SELECT a.n FROM album_artists x JOIN albums a ON a.id = x.album_id WHERE x.artist_id = ?1
               ORDER BY a.year DESC, a.order_name, a.n",
            &[&artist_id],
        )?;
        let on = numbers(
            &db,
            "SELECT DISTINCT a.n, a.year, a.order_name FROM song_artists x JOIN songs s ON s.id = x.song_id JOIN albums a ON a.id = s.album_id
               WHERE x.artist_id = ?1 AND NOT EXISTS (SELECT 1 FROM album_artists y WHERE y.album_id = a.id AND y.artist_id = ?1)
               ORDER BY a.year DESC, a.order_name, a.n",
            &[&artist_id],
        )?;
        let songs = numbers(
            &db,
            "SELECT s.n FROM songs s JOIN albums a ON a.id = s.album_id
               WHERE s.album_id IN (SELECT album_id FROM album_artists WHERE artist_id = ?1)
                  OR s.id IN (SELECT song_id FROM song_artists WHERE artist_id = ?1)
               ORDER BY a.year DESC, a.order_name, a.n, coalesce(s.disc_no, 1), s.track_no, s.rel",
            &[&artist_id],
        )?;
        let mut own = by_numbers(&db, "albums", ALBUM_COLS, "a", &own, album_of)?;
        let mut on = by_numbers(&db, "albums", ALBUM_COLS, "a", &on, album_of)?;
        self.album_artists(&db, &mut own)?;
        self.album_artists(&db, &mut on)?;
        let songs = by_numbers(&db, "songs", SONG_COLS, "s", &songs, song_of)?;
        Ok((own, on, songs))
    }

    /// Genres with their song and album counts.
    pub fn genres(&self) -> Result<Vec<(String, u32, u32)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT name, song_count, album_count FROM genres ORDER BY name COLLATE NOCASE")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Random songs, optionally of a genre, in a year range, in a folder.
    pub fn random_songs(&self, size: usize, genre: Option<&str>, years: (Option<u32>, Option<u32>), folder: Option<i64>) -> Result<Vec<Song>> {
        let db = self.db();
        let mut sql = String::from("SELECT n FROM songs WHERE 1=1");
        let mut p: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(g) = genre {
            p.push(Box::new(g.to_string()));
            sql.push_str(&format!(" AND {}", any_genre(p.len())));
        }
        if let Some(y) = years.0 {
            sql.push_str(&format!(" AND year >= {y}"));
        }
        if let Some(y) = years.1 {
            sql.push_str(&format!(" AND year <= {y}"));
        }
        if let Some(f) = folder {
            sql.push_str(&format!(" AND folder = {f}"));
        }
        sql.push_str(&format!(" ORDER BY random() LIMIT {size}"));
        let refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
        let ns = numbers(&db, &sql, &refs)?;
        let mut songs = by_numbers(&db, "songs", SONG_COLS, "s", &ns, song_of)?;
        self.song_artists(&db, &mut songs)?;
        Ok(songs)
    }

    /// Songs of a genre, by title.
    pub fn songs_by_genre(&self, genre: &str, size: usize, offset: usize, folder: Option<i64>) -> Result<Vec<Song>> {
        let db = self.db();
        let f = folder.map_or(String::new(), |f| format!(" AND folder = {f}"));
        let ns = numbers(
            &db,
            &format!("SELECT n FROM songs WHERE {}{f} ORDER BY order_title, n LIMIT ?2 OFFSET ?3", any_genre(1)),
            &[&genre, &(size as i64), &(offset as i64)],
        )?;
        let mut songs = by_numbers(&db, "songs", SONG_COLS, "s", &ns, song_of)?;
        self.song_artists(&db, &mut songs)?;
        Ok(songs)
    }

    /// `search3`: artists, albums and songs matching what was typed, best first; an empty query lists everything in
    /// name order (clients sync whole libraries that way).
    #[allow(clippy::too_many_arguments)]
    pub fn search(&self, query: &str, artists: (usize, usize), albums: (usize, usize), songs: (usize, usize), folder: Option<i64>) -> Result<(Vec<Artist>, Vec<Album>, Vec<Song>)> {
        let db = self.db();
        let q = query.trim().trim_matches('"');
        let f = folder.map_or(String::new(), |f| format!(" AND folder = {f}"));
        let (artist_ns, album_ns, song_ns) = if q.is_empty() {
            (
                numbers(&db, "SELECT n FROM artists WHERE album_count > 0 ORDER BY order_name, n LIMIT ?1 OFFSET ?2", &[&(artists.0 as i64), &(artists.1 as i64)])?,
                numbers(&db, &format!("SELECT n FROM albums WHERE 1=1{f} ORDER BY order_name, n LIMIT ?1 OFFSET ?2"), &[&(albums.0 as i64), &(albums.1 as i64)])?,
                numbers(&db, &format!("SELECT n FROM songs WHERE 1=1{f} ORDER BY order_title, n LIMIT ?1 OFFSET ?2"), &[&(songs.0 as i64), &(songs.1 as i64)])?,
            )
        } else if let Some(fq) = fts_query(q) {
            // bm25 column weights as Navidrome's: the name counts most.
            (
                numbers(&db, "SELECT rowid FROM artists_fts WHERE artists_fts MATCH ?1 ORDER BY bm25(artists_fts) LIMIT ?2 OFFSET ?3", &[&fq, &(artists.0 as i64), &(artists.1 as i64)])?,
                numbers(
                    &db,
                    &format!("SELECT f.rowid FROM albums_fts f JOIN albums a ON a.n = f.rowid WHERE albums_fts MATCH ?1{} ORDER BY bm25(albums_fts, 10.0, 2.0) LIMIT ?2 OFFSET ?3", f.replace("folder", "a.folder")),
                    &[&fq, &(albums.0 as i64), &(albums.1 as i64)],
                )?,
                numbers(
                    &db,
                    &format!("SELECT f.rowid FROM songs_fts f JOIN songs s ON s.n = f.rowid WHERE songs_fts MATCH ?1{} ORDER BY bm25(songs_fts, 10.0, 2.0, 1.0) LIMIT ?2 OFFSET ?3", f.replace("folder", "s.folder")),
                    &[&fq, &(songs.0 as i64), &(songs.1 as i64)],
                )?,
            )
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        };
        let found_artists = {
            let mut out = Vec::new();
            let mut st = db.prepare_cached("SELECT id, name, mbid, album_count, order_name FROM artists WHERE n = ?1")?;
            for n in artist_ns {
                if let Some(a) = st.query_map([n], artist_of)?.next() {
                    out.push(a?);
                }
            }
            out
        };
        let mut found_albums = by_numbers(&db, "albums", ALBUM_COLS, "a", &album_ns, album_of)?;
        self.album_artists(&db, &mut found_albums)?;
        let mut found_songs = by_numbers(&db, "songs", SONG_COLS, "s", &song_ns, song_of)?;
        self.song_artists(&db, &mut found_songs)?;
        Ok((found_artists, found_albums, found_songs))
    }

    /// A structured search of the library (decision 0072): albums by their own name or artist and by their tracks,
    /// artists by their name and by their albums, tracks, and genres; `limit` of each, the best first. `words` as
    /// `crate::search::words` reads a query.
    pub fn find(&self, words: &[String], limit: usize) -> Result<Found> {
        let db = self.db();
        let Some(fq) = typo_fts_query(&db, words)? else { return Ok(Found::default()) };
        let lim = limit as i64;
        // More asked of the index than are shown (0.9.7): ranked here (`search::rank`, a direct hit first) before the
        // cut, so one found by its artist or album cannot push one named by the words out of the first `limit`.
        let wide = (limit * WIDER).clamp(limit, WIDEST) as i64;
        let ranked = |names: Vec<&str>| -> Vec<usize> {
            let n = names.len();
            let mut order: Vec<(usize, f32)> = names.iter().enumerate().map(|(i, name)| (i, crate::search::rank(name, words, i, n))).collect();
            order.sort_by(|a, b| b.1.total_cmp(&a.1));
            order.into_iter().take(limit).map(|(i, _)| i).collect()
        };
        fn pick<T: Clone>(list: &[T], order: &[usize]) -> Vec<T> {
            order.iter().map(|&i| list[i].clone()).collect()
        }
        // Albums by themselves, the name counting most; then those found only by a track's title.
        let own = numbers(&db, "SELECT rowid FROM albums_fts WHERE albums_fts MATCH ?1 ORDER BY bm25(albums_fts, 10.0, 2.0) LIMIT ?2", &[&fq, &wide])?;
        let listed = by_numbers(&db, "albums", ALBUM_COLS, "a", &own, album_of)?;
        let listed = pick(&listed, &ranked(listed.iter().map(|a| a.name.as_str()).collect()));
        let mut albums: Vec<(Album, Hit)> = listed
            .into_iter()
            .map(|a| {
                let on = if crate::search::matches(&a.name, words) { "name" } else { "artist" };
                let text = if on == "name" { a.name.clone() } else { a.artist.clone() };
                (a, Hit { on, text, count: None })
            })
            .collect();
        let title_q = format!("title : ({fq})");
        let count = |sql: &str, params: &[&dyn rusqlite::ToSql]| -> Result<usize> { Ok(db.prepare_cached(sql)?.query_row(params, |r| r.get::<_, i64>(0))?.max(0) as usize) };
        let mut totals = Totals {
            albums: count(
                "SELECT count(*) FROM (SELECT a.id FROM albums_fts f JOIN albums a ON a.n = f.rowid WHERE albums_fts MATCH ?1
                 UNION SELECT s.album_id FROM songs_fts f JOIN songs s ON s.n = f.rowid WHERE songs_fts MATCH ?2)",
                &[&fq, &title_q],
            )?,
            ..Totals::default()
        };
        let mut st = db.prepare_cached(
            "SELECT s.album_id, count(*), min(s.title) FROM songs_fts f JOIN songs s ON s.n = f.rowid WHERE songs_fts MATCH ?1
             GROUP BY s.album_id ORDER BY count(*) DESC LIMIT ?2",
        )?;
        let by_tracks: Vec<(String, u32, String)> =
            st.query_map(rusqlite::params![title_q, lim], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u32, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        for (id, count, title) in by_tracks {
            if albums.len() >= limit || albums.iter().any(|(a, _)| a.id == id) {
                continue;
            }
            let ns = numbers(&db, "SELECT n FROM albums WHERE id = ?1", &[&id])?;
            if let Some(a) = by_numbers(&db, "albums", ALBUM_COLS, "a", &ns, album_of)?.pop() {
                albums.push((a, Hit { on: "track", text: title, count: Some(count) }));
            }
        }
        // Their artists, as every album the API gives has them.
        let (mut list, hits): (Vec<Album>, Vec<Hit>) = albums.into_iter().unzip();
        self.album_artists(&db, &mut list)?;
        let albums: Vec<(Album, Hit)> = list.into_iter().zip(hits).collect();
        // Artists by their name; then those found only by an album's name.
        let own = numbers(&db, "SELECT rowid FROM artists_fts WHERE artists_fts MATCH ?1 ORDER BY bm25(artists_fts) LIMIT ?2", &[&fq, &wide])?;
        let mut artists: Vec<(Artist, Hit)> = Vec::new();
        {
            let mut st = db.prepare_cached("SELECT id, name, mbid, album_count, order_name FROM artists WHERE n = ?1")?;
            let mut listed: Vec<Artist> = Vec::new();
            for n in own {
                if let Some(a) = st.query_map([n], artist_of)?.next() {
                    listed.push(a?);
                }
            }
            for a in pick(&listed, &ranked(listed.iter().map(|a| a.name.as_str()).collect())) {
                let text = a.name.clone();
                artists.push((a, Hit { on: "name", text, count: None }));
            }
        }
        let name_q = format!("name : ({fq})");
        totals.artists = count(
            "SELECT count(*) FROM (SELECT a.id FROM artists_fts f JOIN artists a ON a.n = f.rowid WHERE artists_fts MATCH ?1
             UNION SELECT aa.artist_id FROM albums_fts f JOIN albums a ON a.n = f.rowid JOIN album_artists aa ON aa.album_id = a.id WHERE albums_fts MATCH ?2)",
            &[&fq, &name_q],
        )?;
        let mut st = db.prepare_cached(
            "SELECT aa.artist_id, count(*), min(a.name) FROM albums_fts f JOIN albums a ON a.n = f.rowid JOIN album_artists aa ON aa.album_id = a.id
             WHERE albums_fts MATCH ?1 GROUP BY aa.artist_id ORDER BY count(*) DESC LIMIT ?2",
        )?;
        let by_albums: Vec<(String, u32, String)> =
            st.query_map(rusqlite::params![name_q, lim], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u32, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        {
            let mut st = db.prepare_cached("SELECT id, name, mbid, album_count, order_name FROM artists WHERE id = ?1")?;
            for (id, count, name) in by_albums {
                if artists.len() >= limit || artists.iter().any(|(a, _)| a.id == id) {
                    continue;
                }
                if let Some(a) = st.query_map([&id], artist_of)?.next() {
                    artists.push((a?, Hit { on: "album", text: name, count: Some(count) }));
                }
            }
        }
        // Tracks, the title counting most.
        totals.tracks = count("SELECT count(*) FROM songs_fts WHERE songs_fts MATCH ?1", &[&fq])?;
        let ns = numbers(&db, "SELECT rowid FROM songs_fts WHERE songs_fts MATCH ?1 ORDER BY bm25(songs_fts, 10.0, 2.0, 1.0) LIMIT ?2", &[&fq, &wide])?;
        let listed = by_numbers(&db, "songs", SONG_COLS, "s", &ns, song_of)?;
        let mut tracks = pick(&listed, &ranked(listed.iter().map(|t| t.title.as_str()).collect()));
        self.song_artists(&db, &mut tracks)?;
        // Genres, by their name.
        let mut st = db.prepare_cached("SELECT name, song_count FROM genres")?;
        let mut genres: Vec<(String, u32)> = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as u32)))?
            .filter_map(Result::ok)
            .filter(|(g, _)| crate::search::matches(g, words))
            .collect();
        // Ranked as the rest (a direct hit, closeness, then how much of the name), the most played among equals.
        genres.sort_by(|a, b| crate::search::rank(&b.0, words, 0, 1).total_cmp(&crate::search::rank(&a.0, words, 0, 1)).then(b.1.cmp(&a.1)));
        totals.genres = genres.len();
        genres.truncate(limit);
        Ok(Found { albums, artists, tracks, genres, totals })
    }

    /// The songs whose title, artist and album hold the words, at most `cap`, by id with those three: what search
    /// finds playlists and crates by (decision 0072, deep).
    pub fn songs_naming(&self, words: &[String], cap: usize) -> Result<Vec<(String, String, String, String)>> {
        let db = self.db();
        let Some(fq) = typo_fts_query(&db, words)? else { return Ok(Vec::new()) };
        let mut st = db.prepare_cached(
            "SELECT s.id, s.title, coalesce(s.artist, ''), s.album FROM songs_fts f JOIN songs s ON s.n = f.rowid WHERE songs_fts MATCH ?1 LIMIT ?2",
        )?;
        let rows = st.query_map(rusqlite::params![fq, cap as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The album in a catalogue folder's folder (`dir`), by its id (0.9.5: Replace shows its cover).
    pub fn album_in(&self, folder: i64, dir: &str) -> Result<Option<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT id FROM albums WHERE folder = ?1 AND dir = ?2 ORDER BY n LIMIT 1")?;
        Ok(st.query_map(rusqlite::params![folder, dir], |r| r.get::<_, String>(0))?.next().transpose()?)
    }

    /// An album's tracks for Renumber (0.9.6): (song id, path in its catalogue folder, title, length in ms, disc, track),
    /// in the album's order.
    #[allow(clippy::type_complexity)]
    pub fn album_tracks(&self, album: &str) -> Result<Vec<(String, String, String, u64, Option<u32>, Option<u32>)>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT id, rel, title, coalesce(duration_ms, 0), disc_no, track_no FROM songs WHERE album_id = ?1 ORDER BY coalesce(disc_no, 1), track_no, rel",
        )?;
        let rows = st.query_map([album], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)?.max(0) as u64, r.get::<_, Option<i64>>(4)?.map(|d| d.max(0) as u32), r.get::<_, Option<i64>>(5)?.map(|t| t.max(0) as u32)))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every album's numbering in a catalogue folder (all without one) for Health (0.9.6): (album id, name, artist, each
    /// track's (disc, track)).
    #[allow(clippy::type_complexity)]
    pub fn numbering(&self, folder: Option<i64>) -> Result<Vec<(String, String, String, Vec<(Option<u32>, Option<u32>)>)>> {
        let db = self.db();
        let mut albums: Vec<(String, String, String, Vec<(Option<u32>, Option<u32>)>)> = Vec::new();
        let mut at: HashMap<String, usize> = HashMap::new();
        let mut st = db.prepare("SELECT id, name, artist FROM albums WHERE ?1 IS NULL OR folder = ?1")?;
        for row in st.query_map([folder], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
            let (id, name, artist) = row?;
            at.insert(id.clone(), albums.len());
            albums.push((id, name, artist, Vec::new()));
        }
        let mut st = db.prepare("SELECT album_id, disc_no, track_no FROM songs")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, Option<i64>>(2)?)))? {
            let (album, disc, track) = row?;
            if let Some(i) = at.get(&album) {
                albums[*i].3.push((disc.map(|d| d.max(0) as u32), track.map(|t| t.max(0) as u32)));
            }
        }
        Ok(albums)
    }

    /// An album's place: its catalogue folder and folder in it, its MusicBrainz release, artist and name (0.9.5: its
    /// cover changed by hand).
    #[allow(clippy::type_complexity)]
    /// A song's file and album (0.9.14, Health's fixes): its folder, its path in it, its album's id and its title.
    pub fn song_file(&self, id: &str) -> Result<Option<(i64, String, String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT folder, rel, album_id, title FROM songs WHERE id = ?1")?;
        Ok(st.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.next().transpose()?)
    }

    /// The song a file is (0.9.14, Health's items): its id, its album's, and its title.
    pub fn song_at(&self, folder: i64, rel: &str) -> Result<Option<(String, String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT id, album_id, title FROM songs WHERE folder = ?1 AND rel = ?2")?;
        Ok(st.query_map(rusqlite::params![folder, rel], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.next().transpose()?)
    }

    pub fn album_place(&self, id: &str) -> Result<Option<(i64, String, Option<String>, String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT folder, dir, mbid, artist, name FROM albums WHERE id = ?1")?;
        Ok(st.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.next().transpose()?)
    }

    /// Every album's id, Music-form id, artist id and the files' genres (0.9.7: the library's words, `app::words`).
    pub fn album_word_inputs(&self) -> Result<Vec<(String, String, String, Vec<String>)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, base_id, artist_id, genres FROM albums ORDER BY n")?;
        let rows = st.query_map([], |r| {
            let genres: String = r.get(3)?;
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, genres.split('\u{1f}').filter(|g| !g.is_empty()).map(str::to_string).collect()))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The harvest's albums (0.9.7, `app::harvest`): each Music-form id once, with its MusicBrainz release (or empty),
    /// artist and name.
    pub fn album_subjects(&self) -> Result<Vec<(String, String, String, String)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT base_id, coalesce(max(mbid), ''), min(artist), min(name) FROM albums GROUP BY base_id ORDER BY min(n)")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The harvest's artists: those with albums, by id, with their MusicBrainz id (or empty) and name.
    pub fn artist_subjects(&self) -> Result<Vec<(String, String, String)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, coalesce(mbid, ''), name FROM artists WHERE album_count > 0 ORDER BY n")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The albums these files (catalogue folder, path) are in, each once (0.9.7: the rules' fixes).
    pub fn albums_of_files(&self, files: &[(i64, String)]) -> Result<Vec<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT album_id FROM songs WHERE folder = ?1 AND rel = ?2")?;
        let mut out: Vec<String> = Vec::new();
        for (folder, rel) in files {
            if let Some(a) = st.query_map(rusqlite::params![folder, rel], |r| r.get::<_, String>(0))?.next() {
                let a = a?;
                if !out.contains(&a) {
                    out.push(a);
                }
            }
        }
        Ok(out)
    }

    /// Each album's Music-form id, by its id (0.11, quick crates: the crates' Album axis, Discogs' labels).
    pub fn album_bases(&self) -> Result<HashMap<String, String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, base_id FROM albums")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Each song's album (0.9.7: a file's words are its album's).
    pub fn song_album_map(&self) -> Result<HashMap<String, String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, album_id FROM songs")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The albums whose files carry no genre (0.9.5: asked of Discogs): their ids, MusicBrainz releases (or empty),
    /// artists and names.
    pub fn albums_without_genres(&self) -> Result<Vec<(String, String, String, String)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, coalesce(mbid, ''), artist, name FROM albums WHERE genres = '' ORDER BY n")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every album as Classical and Other versions read it (0.10, `app::classical`): its ids, name, artist, year,
    /// MusicBrainz release, its files' genres and its album artists, in the library's order.
    pub fn work_albums(&self) -> Result<Vec<WorkAlbum>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, base_id, name, artist, year, mbid, genres FROM albums ORDER BY n")?;
        let mut out: Vec<WorkAlbum> = st
            .query_map([], |r| {
                let genres: String = r.get(6)?;
                Ok(WorkAlbum {
                    id: r.get(0)?,
                    base: r.get(1)?,
                    name: r.get(2)?,
                    artist: r.get(3)?,
                    year: r.get::<_, Option<i64>>(4)?.and_then(|y| u32::try_from(y).ok()),
                    release: r.get::<_, Option<String>>(5)?.filter(|m| !m.is_empty()),
                    genres: genres.split('\u{1f}').filter(|g| !g.is_empty()).map(str::to_string).collect(),
                    artists: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let at: HashMap<String, usize> = out.iter().enumerate().map(|(i, a)| (a.id.clone(), i)).collect();
        let mut st = db.prepare("SELECT album_id, artist_id, name, mbid FROM album_artists ORDER BY album_id, position")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?)))?;
        for row in rows {
            let (album, id, name, mbid) = row?;
            if let Some(i) = at.get(&album) {
                out[*i].artists.push(WorkArtist { id, name, mbid: mbid.filter(|m| !m.is_empty()) });
            }
        }
        Ok(out)
    }

    /// Every song as Other versions reads it (0.10): its id, album, MusicBrainz recording, disc and track, title,
    /// artist and year.
    pub fn work_songs(&self) -> Result<Vec<WorkSong>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, album_id, mbid, disc_no, track_no, title, coalesce(artist, ''), year FROM songs ORDER BY n")?;
        let number = |v: Option<i64>| v.and_then(|n| u32::try_from(n).ok());
        let rows = st.query_map([], |r| {
            Ok(WorkSong {
                id: r.get(0)?,
                album: r.get(1)?,
                recording: r.get::<_, Option<String>>(2)?.filter(|m| !m.is_empty()),
                disc: number(r.get(3)?),
                track: number(r.get(4)?),
                title: r.get(5)?,
                artist: r.get(6)?,
                year: number(r.get(7)?),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every album's MusicBrainz release (0.9.6: its release group's types asked of MusicBrainz), in the library's order.
    pub fn album_releases(&self) -> Result<Vec<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT DISTINCT mbid FROM albums WHERE mbid IS NOT NULL AND mbid <> '' ORDER BY n")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).filter(|m| crate::app::discogs::is_mbid(m)).collect())
    }

    /// The albums whose type is not known, only guessed, or Discogs' (0.9.6: asked of Discogs; 0.10: Discogs' own asked
    /// again where an older rule found them): (id, MusicBrainz release, artist, name, tracks, length in ms).
    pub fn albums_to_type(&self) -> Result<Vec<(String, String, String, String, usize, u64)>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT id, coalesce(mbid, ''), artist, name, song_count, duration_ms FROM albums
             WHERE release_type IS NULL OR type_from IN ('guess', 'discogs') ORDER BY n",
        )?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, i64>(4)?.max(0) as usize, r.get::<_, i64>(5)?.max(0) as u64)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Each album's tracks and length (ms) by its MusicBrainz release (0.10: an edition compared with its release
    /// group's first release); of albums sharing a release, the one of more tracks.
    pub fn release_sizes(&self) -> Result<HashMap<String, (usize, u64)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT mbid, song_count, duration_ms FROM albums WHERE mbid IS NOT NULL AND mbid <> ''")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as usize, r.get::<_, i64>(2)?.max(0) as u64)))?;
        let mut out: HashMap<String, (usize, u64)> = HashMap::new();
        for row in rows {
            let (release, tracks, ms) = row?;
            let e = out.entry(release).or_insert((tracks, ms));
            if tracks > e.0 {
                *e = (tracks, ms);
            }
        }
        Ok(out)
    }

    /// Each song's album's release types as the axes show them ("EP", "Live"), primary first (0.9.6); songs of albums of
    /// no known type left out.
    pub fn song_release_types(&self) -> Result<HashMap<String, Vec<String>>> {
        let db = self.db();
        let mut st = db.prepare("SELECT s.id, a.release_type, a.secondary FROM songs s JOIN albums a ON a.id = s.album_id WHERE a.release_type IS NOT NULL OR a.secondary <> ''")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?)))?;
        let mut out = HashMap::new();
        for row in rows {
            let (song, primary, secondary) = row?;
            let words: Vec<String> = primary.iter().map(String::as_str).chain(secondary.split('\u{1f}').filter(|s| !s.is_empty())).map(super::library::type_label).collect();
            out.insert(song, words);
        }
        Ok(out)
    }

    /// The MusicBrainz releases of the albums without an original year (0.9.5: asked of MusicBrainz), in the library's
    /// order.
    pub fn releases_without_original(&self) -> Result<Vec<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT DISTINCT mbid FROM albums WHERE original_year IS NULL AND mbid IS NOT NULL AND mbid <> '' ORDER BY n")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The songs of these albums, the albums in the order given, each one's in its own order (disc, track, file), as
    /// `album` has them; read 500 albums at a time (0.9.4: several albums' tracks in one call).
    pub fn songs_of_albums(&self, albums: &[String]) -> Result<Vec<Song>> {
        let db = self.db();
        let mut of: HashMap<String, Vec<Song>> = HashMap::new();
        for chunk in albums.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(", ");
            let mut st = db.prepare(&format!("SELECT {SONG_COLS} FROM songs s WHERE s.album_id IN ({marks}) ORDER BY coalesce(s.disc_no, 1), s.track_no, s.rel"))?;
            let mut songs = st.query_map(params_from_iter(chunk.iter()), song_of)?.collect::<rusqlite::Result<Vec<_>>>()?;
            self.song_artists(&db, &mut songs)?;
            for s in songs {
                of.entry(s.album_id.clone()).or_default().push(s);
            }
        }
        Ok(albums.iter().filter_map(|a| of.remove(a)).flatten().collect())
    }

    /// Every track for the Library page's Tracks (0.9.4): those whose title, artist and album hold the words (every one
    /// without any), but those in the catalogue folders `off` (libraries switched off); in no order.
    pub fn track_rows(&self, words: &[String], off: &[i64]) -> Result<Vec<TrackRow>> {
        let db = self.db();
        let rows: Vec<TrackRow> = match typo_fts_query(&db, words)? {
            Some(fq) => {
                let mut st = db.prepare_cached(&format!("SELECT {TRACK_ROW_COLS} FROM songs_fts f JOIN songs s ON s.n = f.rowid WHERE songs_fts MATCH ?1"))?;
                st.query_map([fq], track_row)?.collect::<rusqlite::Result<_>>()?
            }
            None => {
                let mut st = db.prepare_cached(&format!("SELECT {TRACK_ROW_COLS} FROM songs s"))?;
                st.query_map([], track_row)?.collect::<rusqlite::Result<_>>()?
            }
        };
        Ok(rows.into_iter().filter(|r| !off.contains(&r.folder)).collect())
    }

    /// These tracks' rows, as the Tracks list has them (0.9.8: what a device keeps for offline play); unknown ids left
    /// out.
    pub fn track_rows_by_id(&self, ids: &[String]) -> Result<Vec<TrackRow>> {
        let db = self.db();
        let mut out = Vec::new();
        for chunk in ids.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(",");
            let mut st = db.prepare(&format!("SELECT {TRACK_ROW_COLS} FROM songs s WHERE s.id IN ({marks})"))?;
            out.extend(st.query_map(params_from_iter(chunk.iter()), track_row)?.collect::<rusqlite::Result<Vec<_>>>()?);
        }
        Ok(out)
    }

    /// An album's id as it is (or would be) in Music: what its tags are kept under.
    pub fn album_base(&self, album: &str) -> Result<Option<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT base_id FROM albums WHERE id = ?1")?;
        Ok(st.query_map([album], |r| r.get::<_, String>(0))?.next().transpose()?)
    }

    /// The albums kept under these Music-form ids (an album in Incoming and its kept copy share one), in the order
    /// given.
    pub fn albums_by_base(&self, bases: &[String]) -> Result<Vec<Album>> {
        let db = self.db();
        let mut ns = Vec::new();
        {
            let mut st = db.prepare_cached("SELECT n FROM albums WHERE base_id = ?1 ORDER BY folder")?;
            for b in bases {
                for n in st.query_map([b], |r| r.get::<_, i64>(0))? {
                    ns.push(n?);
                }
            }
        }
        let mut albums = by_numbers(&db, "albums", ALBUM_COLS, "a", &ns, album_of)?;
        self.album_artists(&db, &mut albums)?;
        Ok(albums)
    }

    /// The albums of one catalogue folder, newest first, at most `limit`, with what a source's album listing needs:
    /// (album, its folder relative to the catalogue folder, format, lowest bitrate, length in seconds).
    #[allow(clippy::type_complexity)]
    pub fn albums_in(&self, folder: i64, limit: usize) -> Result<Vec<(Album, String, Option<String>, Option<u32>, Option<f64>)>> {
        let db = self.db();
        let ns = numbers(&db, "SELECT n FROM albums WHERE folder = ?1 ORDER BY created DESC, n LIMIT ?2", &[&folder, &(limit as i64)])?;
        let mut albums = by_numbers(&db, "albums", ALBUM_COLS, "a", &ns, album_of)?;
        self.album_artists(&db, &mut albums)?;
        let mut st = db.prepare_cached(&format!("SELECT dir, format, min_kbps, {ALBUM_LENGTH} FROM albums a WHERE a.id = ?1"))?;
        let mut out = Vec::with_capacity(albums.len());
        for a in albums {
            let (dir, format, kbps, ms) = st.query_row([&a.id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<u32>>(2)?, r.get::<_, Option<i64>>(3)?))
            })?;
            out.push((a, dir, format, kbps, ms.map(|ms| ms as f64 / 1000.0)));
        }
        Ok(out)
    }

    /// One album's format, lowest bitrate and length: (songs, format, lowest bitrate, length in seconds).
    pub fn album_facts(&self, album: &str) -> Result<Option<(u32, Option<String>, Option<u32>, Option<f64>)>> {
        let db = self.db();
        let mut st = db.prepare_cached(&format!("SELECT song_count, format, min_kbps, {ALBUM_LENGTH} FROM albums a WHERE a.id = ?1"))?;
        let mut facts = st.query_map([album], |r| {
            Ok((r.get::<_, i64>(0)?.max(0) as u32, r.get(1)?, r.get(2)?, r.get::<_, Option<i64>>(3)?.map(|ms| ms as f64 / 1000.0)))
        })?;
        Ok(facts.next().transpose()?)
    }

    /// A page of one catalogue folder's songs: (song id, path), in a stable order.
    pub fn song_paths(&self, folder: i64, start: usize, limit: usize) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT id, path FROM songs WHERE folder = ?1 ORDER BY n LIMIT ?2 OFFSET ?3")?;
        let rows = st.query_map(params_from_iter([folder, limit as i64, start as i64]), |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The albums outside the Music folder (waiting in Incoming), newest first.
    pub fn incoming_albums(&self) -> Result<Vec<Album>> {
        let db = self.db();
        let ns = numbers(&db, "SELECT n FROM albums WHERE folder > 0 ORDER BY created DESC, n", &[])?;
        let mut albums = by_numbers(&db, "albums", ALBUM_COLS, "a", &ns, album_of)?;
        self.album_artists(&db, &mut albums)?;
        Ok(albums)
    }

    /// Answers when both its tables can be read: (songs, albums). For the health check.
    pub fn health(&self) -> Result<(u64, u64)> {
        let db = self.db();
        let songs: i64 = db.query_row("SELECT count(*) FROM songs", [], |r| r.get(0))?;
        let albums: i64 = db.query_row("SELECT count(*) FROM albums", [], |r| r.get(0))?;
        Ok((songs as u64, albums as u64))
    }

    /// The album of each of these songs, by song id; a song not in the library is left out.
    /// Every song's album, in one go: (song, album).
    pub fn song_albums(&self) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT id, album_id FROM songs")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn albums_of(&self, songs: &[String]) -> Result<HashMap<String, String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT album_id FROM songs WHERE id = ?1")?;
        let mut out = HashMap::new();
        for id in songs {
            if let Some(album) = st.query_map([id], |r| r.get::<_, String>(0))?.next().transpose()? {
                out.insert(id.clone(), album);
            }
        }
        Ok(out)
    }

    /// Where a song's file is: (catalogue folder, path in it).
    pub fn file_of(&self, song: &str) -> Result<Option<(i64, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT folder, rel FROM songs WHERE id = ?1")?;
        Ok(st.query_map([song], |r| Ok((r.get(0)?, r.get(1)?)))?.next().transpose()?)
    }

    /// What an album's art is found from: (catalogue folder, its folder in it, its cover id, its first track's path in
    /// the catalogue folder).
    /// Every album's cover stamp (0.9.7): its art's fingerprint, from its cover id (`<album>_<print>`); an album
    /// without a cover has none.
    pub fn cover_stamps(&self) -> Result<std::collections::HashMap<String, String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT id, cover FROM albums WHERE cover IS NOT NULL")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (id, cover) = row?;
            if let Some(print) = cover.strip_prefix(&format!("{id}_")).filter(|p| !p.is_empty()) {
                out.insert(id, print.to_string());
            }
        }
        Ok(out)
    }

    pub fn album_art(&self, album: &str) -> Result<Option<(i64, String, Option<String>, Option<String>)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT folder, dir, cover FROM albums WHERE id = ?1")?;
        let found = st.query_map([album], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))?.next().transpose()?;
        let Some((folder, dir, cover)) = found else { return Ok(None) };
        let mut first = db.prepare_cached("SELECT rel FROM songs WHERE album_id = ?1 ORDER BY rel LIMIT 1")?;
        let rel = first.query_map([album], |r| r.get::<_, String>(0))?.next().transpose()?;
        Ok(Some((folder, dir, cover, rel)))
    }

    /// An album's folder: (catalogue folder, path in it).
    pub fn album_dir(&self, album: &str) -> Result<Option<(i64, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT folder, dir FROM albums WHERE id = ?1")?;
        Ok(st.query_map([album], |r| Ok((r.get(0)?, r.get(1)?)))?.next().transpose()?)
    }

    /// What a player reports about a song in Cratarr's ids (the pushed player state, decision 0031): the song id, its
    /// album's id, the album's name and album artist. `None` when the track is not in the library.
    #[allow(dead_code)] // see `SongIds`
    pub fn song_ids(&self, track: i64) -> Result<Option<SongIds>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT s.id, s.album_id, a.name, a.artist FROM songs s JOIN albums a ON a.id = s.album_id WHERE s.id = ?1",
        )?;
        let id = super::ids::song(track);
        Ok(st
            .query_map([&id], |r| Ok(SongIds { song_id: r.get(0)?, album_id: r.get(1)?, album: r.get(2)?, album_artist: r.get(3)? }))?
            .next()
            .transpose()?)
    }

    /// A song's album and artists, for counting a play.
    pub fn play_targets(&self, song: &str) -> Result<Option<(String, String, Vec<String>)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT pid, album_id FROM songs WHERE id = ?1")?;
        let Some((pid, album)) = st.query_map([song], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.next().transpose()? else {
            return Ok(None);
        };
        let mut a = db.prepare_cached("SELECT artist_id FROM song_artists WHERE song_id = ?1 ORDER BY position")?;
        let artists = a.query_map([song], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(Some((pid, album, artists)))
    }

    /// Every album's folder: (id, catalogue folder, path in it), to find an album by the folder a media server names.
    pub fn album_places(&self) -> Result<Vec<(String, i64, String)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, folder, dir FROM albums")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Whether the library has this artist.
    pub fn has_artist(&self, id: &str) -> Result<bool> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT 1 FROM artists WHERE id = ?1")?;
        Ok(st.exists([id])?)
    }

    /// Every persistent id with the song that carries its user data, for that data to follow a rebuilt catalogue. A
    /// track in two folders (an album in Music and again in Incoming) is one persistent id with two songs: its data
    /// goes to the first folder's (Music's, the library's own file), not back and forth between them on every build.
    pub fn pids(&self) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT pid, id FROM songs ORDER BY folder, n")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for row in rows {
            let (pid, id) = row?;
            if seen.insert(pid.clone()) {
                out.push((pid, id));
            }
        }
        Ok(out)
    }
}

fn artist_of(r: &rusqlite::Row) -> rusqlite::Result<Artist> {
    let id: String = r.get(0)?;
    Ok(Artist {
        cover_art: id.clone(),
        id,
        name: r.get(1)?,
        music_brainz_id: r.get(2)?,
        album_count: r.get::<_, i64>(3)?.max(0) as u32,
        sort_name: r.get(4)?,
        ..Artist::default()
    })
}
