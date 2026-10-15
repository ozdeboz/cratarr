//! Reading the library as the API shows it (decision 0030). Lists are read in two phases, Navidrome's pattern
//! (`persistence/sql_search.go`, translated): the page's row numbers first, over an index and without joins, then the
//! details of just those rows. User data (stars, plays, ratings) is added by the caller (`annotate`).

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{Connection, params_from_iter};

use super::api::{Album, Artist, ArtistRef, GenreRef, Song, content_type, iso};
use super::library::{Library, fts_query};

const SONG_COLS: &str = "s.n, s.id, s.folder, s.path, s.album_id, s.title, s.artist, s.album, s.album_artist, s.track_no, s.disc_no,
  s.year, s.genre, s.bpm, s.duration_ms, s.size, s.suffix, s.kbps, s.created, s.mbid, s.cover";
const ALBUM_COLS: &str = "a.n, a.id, a.name, a.artist, a.artist_id, a.year, a.genre, a.genres, a.song_count, a.duration_ms,
  a.created, a.mbid, a.compilation, a.cover";

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
        genres: genre.iter().map(|g| GenreRef { name: g.clone() }).collect(),
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
        ..Album::default()
    })
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
            sql.push_str(&format!(" AND genre = ?{} COLLATE NOCASE", p.len()));
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
            &format!("SELECT n FROM songs WHERE genre = ?1 COLLATE NOCASE{f} ORDER BY order_title, n LIMIT ?2 OFFSET ?3"),
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
    /// (album, its folder relative to the catalogue folder, format, lowest bitrate).
    pub fn albums_in(&self, folder: i64, limit: usize) -> Result<Vec<(Album, String, Option<String>, Option<u32>)>> {
        let db = self.db();
        let ns = numbers(&db, "SELECT n FROM albums WHERE folder = ?1 ORDER BY created DESC, n LIMIT ?2", &[&folder, &(limit as i64)])?;
        let mut albums = by_numbers(&db, "albums", ALBUM_COLS, "a", &ns, album_of)?;
        self.album_artists(&db, &mut albums)?;
        let mut st = db.prepare_cached("SELECT dir, format, min_kbps FROM albums WHERE id = ?1")?;
        let mut out = Vec::with_capacity(albums.len());
        for a in albums {
            let (dir, format, kbps) = st.query_row([&a.id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<u32>>(2)?)))?;
            out.push((a, dir, format, kbps));
        }
        Ok(out)
    }

    /// One album's format and lowest bitrate: (songs, format, lowest bitrate).
    pub fn album_facts(&self, album: &str) -> Result<Option<(u32, Option<String>, Option<u32>)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT song_count, format, min_kbps FROM albums WHERE id = ?1")?;
        Ok(st.query_map([album], |r| Ok((r.get::<_, i64>(0)?.max(0) as u32, r.get(1)?, r.get(2)?)))?.next().transpose()?)
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

    /// Where a song's file is: (catalogue folder, path in it).
    pub fn file_of(&self, song: &str) -> Result<Option<(i64, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT folder, rel FROM songs WHERE id = ?1")?;
        Ok(st.query_map([song], |r| Ok((r.get(0)?, r.get(1)?)))?.next().transpose()?)
    }

    /// What an album's art is found from: (catalogue folder, its folder in it, its cover id, its first track's path in
    /// the catalogue folder).
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

    /// Every song's (persistent id, id), for user data to follow a rebuilt catalogue.
    pub fn pids(&self) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT pid, id FROM songs")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
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
