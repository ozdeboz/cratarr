//! The OpenSubsonic library (decision 0030): songs, albums and artists as the API shows them, derived from the
//! catalogue into their own SQLite file (deleting it costs a rebuild). Patterns from Navidrome
//! (https://github.com/navidrome/navidrome, GPL-3.0-or-later, Copyright the Navidrome authors), translated:
//!
//! - an album's aggregates live on its row (songs, length, size, years, discs, genres, added = its oldest song);
//! - search through contentless FTS5 tables (`unicode61 remove_diacritics 2`), each word as `(word OR word*)`, ranked
//!   by bm25 with column weights (`persistence/sql_search_fts.go`);
//! - paging in two phases, the page's row ids first and their details after, over covering indexes
//!   (`persistence/sql_search.go`);
//! - WAL, `synchronous=NORMAL`, a busy timeout, and `ANALYZE` after a rebuild (`db/optimize.go`).
//!
//! Changed from Navidrome: the library is rebuilt whole when the catalogue's files change (24,000 songs take seconds),
//! where Navidrome refreshes the albums a scan touched; one library; no tag tables (the catalogue keeps the tags).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use super::ids;
use crate::catalogue::{Rules, ServedRow};

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA temp_store = MEMORY;
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS songs (
  n INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, pid TEXT NOT NULL, folder INTEGER NOT NULL, rel TEXT NOT NULL,
  album_id TEXT NOT NULL, title TEXT NOT NULL, artist TEXT, album TEXT NOT NULL, album_artist TEXT,
  track_no INTEGER, disc_no INTEGER, year INTEGER, genre TEXT, bpm INTEGER, duration_ms INTEGER, size INTEGER NOT NULL,
  suffix TEXT, kbps INTEGER, created INTEGER, mbid TEXT, order_title TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS songs_by_album ON songs (album_id, disc_no, track_no, rel);
CREATE INDEX IF NOT EXISTS songs_by_title ON songs (order_title, n);
CREATE INDEX IF NOT EXISTS songs_by_pid ON songs (pid);
CREATE TABLE IF NOT EXISTS song_artists (song_id TEXT NOT NULL, artist_id TEXT NOT NULL, name TEXT NOT NULL,
  position INTEGER NOT NULL, PRIMARY KEY (song_id, position)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS albums (
  n INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, name TEXT NOT NULL, artist TEXT NOT NULL, artist_id TEXT NOT NULL,
  year INTEGER, original_year INTEGER, genre TEXT, genres TEXT NOT NULL, song_count INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL, size INTEGER NOT NULL, discs INTEGER NOT NULL, created INTEGER, mbid TEXT,
  folder INTEGER NOT NULL, dir TEXT NOT NULL, compilation INTEGER NOT NULL, order_name TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS albums_by_name ON albums (order_name, n);
CREATE INDEX IF NOT EXISTS albums_by_created ON albums (created DESC, n);
CREATE INDEX IF NOT EXISTS albums_by_year ON albums (year, n);
CREATE INDEX IF NOT EXISTS albums_by_artist ON albums (artist_id, year, n);
CREATE TABLE IF NOT EXISTS album_artists (album_id TEXT NOT NULL, artist_id TEXT NOT NULL, name TEXT NOT NULL,
  position INTEGER NOT NULL, PRIMARY KEY (album_id, position)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS album_artists_by_artist ON album_artists (artist_id, album_id);
CREATE TABLE IF NOT EXISTS artists (
  n INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, name TEXT NOT NULL, mbid TEXT, album_count INTEGER NOT NULL,
  song_count INTEGER NOT NULL, order_name TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS artists_by_name ON artists (order_name, n);
CREATE TABLE IF NOT EXISTS genres (name TEXT PRIMARY KEY, song_count INTEGER NOT NULL, album_count INTEGER NOT NULL);
CREATE VIRTUAL TABLE IF NOT EXISTS songs_fts USING fts5(title, artist, album, content='', contentless_delete=1, tokenize='unicode61 remove_diacritics 2');
CREATE VIRTUAL TABLE IF NOT EXISTS albums_fts USING fts5(name, artist, content='', contentless_delete=1, tokenize='unicode61 remove_diacritics 2');
CREATE VIRTUAL TABLE IF NOT EXISTS artists_fts USING fts5(name, content='', contentless_delete=1, tokenize='unicode61 remove_diacritics 2');
";

/// The meta key of the catalogue state the library was built from.
const BUILT_FROM: &str = "built_from";

/// One folder of the catalogue as the library reads it: its id there and its files.
pub struct FolderRows {
    pub folder: i64,
    pub rows: Vec<ServedRow>,
}

pub struct Library {
    db: Mutex<Connection>,
}

/// A number at the start of a tag ("3/12" is 3).
fn leading_number(v: &str) -> Option<u32> {
    let digits: String = v.trim().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|n| *n > 0)
}

/// The year in a date tag ("1997-05-12", "1997").
pub fn year_of(v: &str) -> Option<u32> {
    let y: String = v.trim().chars().take(4).collect();
    (y.len() == 4).then(|| y.parse().ok()).flatten().filter(|y| *y > 0)
}

/// How names sort: lower case, typographic quotes plain, a leading article ("The", "A", "An") moved off, as
/// Navidrome's `OrderName` and nghe's index do.
pub fn order_name(name: &str) -> String {
    let lower = ids::clear(&name.trim().to_lowercase());
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = lower.strip_prefix(article) {
            return rest.trim_start().to_string();
        }
    }
    lower
}

/// What a song shows and is grouped by.
struct Song<'a> {
    row: &'a ServedRow,
    folder: i64,
    title: String,
    artists: Vec<String>,
    artist_credit: Option<String>,
    album: String,
    album_artist: String,
    album_artists: Vec<String>,
    album_id: String,
    genres: Vec<String>,
}

fn song<'a>(row: &'a ServedRow, folder: i64, rules: &Rules) -> Song<'a> {
    let file_name = row.rel.rsplit('/').next().unwrap_or(&row.rel);
    let stem = file_name.rsplit_once('.').map_or(file_name, |(s, _)| s);
    let title = row.first("title").unwrap_or(stem).to_string();
    let list = |list: &str, text: &str| {
        let from = if row.all(list).next().is_some() { list } else { text };
        rules.values(from, &row.all(from).map(str::to_string).collect::<Vec<_>>())
    };
    let artists = list("artists", "artist");
    let artist_credit = row.first("artist").map(str::to_string).or_else(|| (!artists.is_empty()).then(|| artists.join(", ")));
    // The album artist as the file credits it, else the song's artist.
    let album_artist = row.first("albumartist").map(str::to_string).or_else(|| artist_credit.clone()).unwrap_or_else(|| "[Unknown Artist]".into());
    let mut album_artists = list("albumartists", "albumartist");
    if album_artists.is_empty() {
        album_artists = if artists.is_empty() { vec![album_artist.clone()] } else { artists.clone() };
    }
    let album = row.first("album").map(str::to_string).unwrap_or_else(|| "[Unknown Album]".into());
    let album_id = ids::album(row, &album_artist, folder);
    let genres = rules.values("genre", &row.all("genre").map(str::to_string).collect::<Vec<_>>());
    Song { row, folder, title, artists, artist_credit, album, album_artist, album_artists, album_id, genres }
}

/// What the catalogue's files say, cheaply: when it differs from what the library was built from, it is built again.
pub fn fingerprint(folders: &[FolderRows]) -> String {
    let mut h = md5::Md5::default();
    use md5::Digest;
    for f in folders {
        h.update(f.folder.to_le_bytes());
        for r in &f.rows {
            h.update(r.rel.as_bytes());
            h.update(r.track.unwrap_or(0).to_le_bytes());
            h.update(r.mtime.to_le_bytes());
            h.update(r.size.to_le_bytes());
        }
    }
    format!("{:x}", h.finalize())
}

impl Library {
    /// `file`: the database; `None` keeps it in memory.
    pub fn open(file: Option<&Path>) -> Result<Self> {
        let conn = match file {
            Some(f) => Connection::open(f).with_context(|| format!("opening {}", f.display()))?,
            None => Connection::open_in_memory()?,
        };
        conn.busy_timeout(Duration::from_secs(15))?;
        conn.execute_batch(SCHEMA).context("creating the library tables")?;
        Ok(Self { db: Mutex::new(conn) })
    }

    pub(crate) fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Builds the library again from the catalogue's files, unless it was built from these already. Returns whether it
    /// was built. Blocking.
    pub fn refresh(&self, folders: &[FolderRows], rules: &Rules) -> Result<bool> {
        let print = fingerprint(folders);
        let built: Option<String> =
            self.db().query_row("SELECT value FROM meta WHERE key = ?1", [BUILT_FROM], |r| r.get(0)).optional()?;
        if built.as_deref() == Some(print.as_str()) {
            return Ok(false);
        }
        self.build(folders, rules)?;
        self.db().execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![BUILT_FROM, print])?;
        Ok(true)
    }

    fn build(&self, folders: &[FolderRows], rules: &Rules) -> Result<()> {
        let songs: Vec<Song> =
            folders.iter().flat_map(|f| f.rows.iter().filter(|r| r.track.is_some()).map(move |r| (f.folder, r))).map(|(folder, r)| song(r, folder, rules)).collect();
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute_batch(
            "DELETE FROM songs; DELETE FROM song_artists; DELETE FROM albums; DELETE FROM album_artists; DELETE FROM artists;
             DELETE FROM genres; DELETE FROM songs_fts; DELETE FROM albums_fts; DELETE FROM artists_fts;",
        )?;
        // Artists: every name credited on an album or a song, the album artists counted.
        let mut artist_names: BTreeMap<String, String> = BTreeMap::new();
        let mut artist_mbid: HashMap<String, String> = HashMap::new();
        let mut artist_albums: HashMap<String, BTreeSet<String>> = HashMap::new();
        let mut artist_songs: HashMap<String, u32> = HashMap::new();
        let mut albums: BTreeMap<String, Vec<&Song>> = BTreeMap::new();
        {
            let mut ins_song = tx.prepare(
                "INSERT INTO songs (id, pid, folder, rel, album_id, title, artist, album, album_artist, track_no, disc_no, year,
                   genre, bpm, duration_ms, size, suffix, kbps, created, mbid, order_title)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            )?;
            let mut ins_fts = tx.prepare("INSERT INTO songs_fts (rowid, title, artist, album) VALUES (?1, ?2, ?3, ?4)")?;
            let mut ins_artist = tx.prepare("INSERT INTO song_artists (song_id, artist_id, name, position) VALUES (?1, ?2, ?3, ?4)")?;
            for s in &songs {
                let r = s.row;
                let id = ids::song(r.track.unwrap_or_default());
                let suffix = r.rel.rsplit_once('.').map(|(_, e)| e.to_lowercase());
                ins_song.execute(params![
                    id,
                    ids::song_pid(r, &s.album_artist),
                    s.folder,
                    r.rel,
                    s.album_id,
                    s.title,
                    s.artist_credit,
                    s.album,
                    s.album_artist,
                    r.first("tracknumber").and_then(leading_number),
                    r.first("discnumber").and_then(leading_number),
                    r.first("date").and_then(year_of),
                    s.genres.first(),
                    r.first("bpm").and_then(leading_number),
                    r.duration_ms.map(|d| d as i64),
                    r.size as i64,
                    suffix,
                    r.kbps,
                    r.born,
                    r.first("musicbrainz_trackid"),
                    order_name(&s.title),
                ])?;
                let n = tx.last_insert_rowid();
                ins_fts.execute(params![n, s.title, s.artist_credit.as_deref().unwrap_or(""), s.album])?;
                for (i, name) in s.artists.iter().enumerate() {
                    let aid = ids::artist(name);
                    ins_artist.execute(params![id, aid, name, i as i64])?;
                    artist_names.entry(aid.clone()).or_insert_with(|| name.clone());
                    *artist_songs.entry(aid).or_default() += 1;
                }
                albums.entry(s.album_id.clone()).or_default().push(s);
            }
        }
        {
            let mut ins_album = tx.prepare(
                "INSERT INTO albums (id, name, artist, artist_id, year, original_year, genre, genres, song_count, duration_ms,
                   size, discs, created, mbid, folder, dir, compilation, order_name)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            )?;
            let mut ins_fts = tx.prepare("INSERT INTO albums_fts (rowid, name, artist) VALUES (?1, ?2, ?3)")?;
            let mut ins_aa = tx.prepare("INSERT INTO album_artists (album_id, artist_id, name, position) VALUES (?1, ?2, ?3, ?4)")?;
            let mut genre_songs: BTreeMap<String, (String, u32, BTreeSet<String>)> = BTreeMap::new();
            for (album_id, songs) in &albums {
                let first = songs[0];
                let years: Vec<u32> = songs.iter().filter_map(|s| s.row.first("date").and_then(year_of)).collect();
                let original: Vec<u32> = songs.iter().filter_map(|s| s.row.first("originaldate").and_then(year_of)).collect();
                let mut genres: Vec<String> = Vec::new();
                for s in songs.iter() {
                    for g in &s.genres {
                        if !genres.iter().any(|x| x.eq_ignore_ascii_case(g)) {
                            genres.push(g.clone());
                        }
                        let e = genre_songs.entry(g.to_lowercase()).or_insert_with(|| (g.clone(), 0, BTreeSet::new()));
                        e.1 += 1;
                        e.2.insert(album_id.clone());
                    }
                }
                // The album's folder: the one its files share.
                let dirs: Vec<&str> = songs.iter().map(|s| s.row.rel.rsplit_once('/').map_or("", |(d, _)| d)).collect();
                let dir = common_dir(&dirs);
                let discs = songs.iter().filter_map(|s| s.row.first("discnumber").and_then(leading_number)).max().unwrap_or(1);
                let compilation = songs.iter().any(|s| s.row.first("compilation").is_some_and(|c| c == "1"));
                let artist_id = ids::artist(&first.album_artists[0]);
                ins_album.execute(params![
                    album_id,
                    first.album,
                    first.album_artist,
                    artist_id,
                    years.iter().max(),
                    original.iter().min(),
                    genres.first(),
                    genres.join("\u{1f}"),
                    songs.len() as i64,
                    songs.iter().filter_map(|s| s.row.duration_ms).sum::<u64>() as i64,
                    songs.iter().map(|s| s.row.size).sum::<u64>() as i64,
                    discs,
                    songs.iter().filter_map(|s| s.row.born).min(),
                    first.row.first("musicbrainz_albumid"),
                    first.folder,
                    dir,
                    compilation,
                    order_name(&first.album),
                ])?;
                ins_fts.execute(params![tx.last_insert_rowid(), first.album, first.album_artist])?;
                for (i, name) in first.album_artists.iter().enumerate() {
                    let aid = ids::artist(name);
                    ins_aa.execute(params![album_id, aid, name, i as i64])?;
                    artist_names.entry(aid.clone()).or_insert_with(|| name.clone());
                    artist_albums.entry(aid.clone()).or_default().insert(album_id.clone());
                    if let (0, Some(mb)) = (i, first.row.first("musicbrainz_albumartistid")) {
                        artist_mbid.entry(aid).or_insert_with(|| mb.to_string());
                    }
                }
            }
            let mut ins_genre = tx.prepare("INSERT OR IGNORE INTO genres (name, song_count, album_count) VALUES (?1, ?2, ?3)")?;
            for (_, (name, songs, albums)) in genre_songs {
                ins_genre.execute(params![name, songs, albums.len() as i64])?;
            }
        }
        {
            let mut ins_artist =
                tx.prepare("INSERT INTO artists (id, name, mbid, album_count, song_count, order_name) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
            let mut ins_fts = tx.prepare("INSERT INTO artists_fts (rowid, name) VALUES (?1, ?2)")?;
            for (aid, name) in &artist_names {
                ins_artist.execute(params![
                    aid,
                    name,
                    artist_mbid.get(aid),
                    artist_albums.get(aid).map_or(0, |a| a.len() as i64),
                    artist_songs.get(aid).copied().unwrap_or(0),
                    order_name(name),
                ])?;
                ins_fts.execute(params![tx.last_insert_rowid(), name])?;
            }
        }
        tx.commit()?;
        db.execute_batch("ANALYZE;")?;
        Ok(())
    }

    /// Counts, for the log and tests: (songs, albums, artists).
    pub fn counts(&self) -> Result<(u64, u64, u64)> {
        let db = self.db();
        let n = |t: &str| -> Result<u64> { Ok(db.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get::<_, i64>(0))? as u64) };
        Ok((n("songs")?, n("albums")?, n("artists")?))
    }
}

/// The folder several paths share ("" when none).
fn common_dir(dirs: &[&str]) -> String {
    let mut common: Option<Vec<&str>> = None;
    for d in dirs {
        let parts: Vec<&str> = d.split('/').filter(|p| !p.is_empty()).collect();
        common = Some(match common {
            None => parts,
            Some(c) => c.iter().zip(&parts).take_while(|(a, b)| a == b).map(|(a, _)| *a).collect(),
        });
    }
    common.unwrap_or_default().join("/")
}

/// An FTS5 query from what a user typed: each word as `(word OR word*)`, as Navidrome's; quotes and FTS operators
/// cannot leak in. `None` when nothing is left to search for.
pub fn fts_query(typed: &str) -> Option<String> {
    let words: Vec<String> = typed
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("(\"{w}\" OR \"{w}\"*)"))
        .collect();
    (!words.is_empty()).then(|| words.join(" AND "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(track: i64, rel: &str, tags: &[(&str, &str)]) -> ServedRow {
        ServedRow {
            rel: rel.into(),
            track: Some(track),
            born: Some(1_700_000_000 + track),
            format: Some("FLAC".into()),
            kbps: Some(900),
            duration_ms: Some(200_000),
            size: 20_000_000,
            mtime: 1,
            tags: tags.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect(),
        }
    }

    fn sample() -> Vec<FolderRows> {
        let homogenic = |t: i64, n: &str, title: &str| {
            row(t, &format!("Björk/Homogenic (1997)/{n} {title}.flac"), &[
                ("title", title),
                ("artist", "Björk"),
                ("albumartist", "Björk"),
                ("album", "Homogenic"),
                ("tracknumber", n),
                ("date", "1997-09-22"),
                ("genre", "Electronic; Art Pop"),
            ])
        };
        vec![FolderRows {
            folder: 0,
            rows: vec![
                homogenic(1, "01", "Hunter"),
                homogenic(2, "02", "Jóga"),
                row(3, "The Orb/Adventures (1991)/01 Little Fluffy Clouds.mp3", &[
                    ("title", "Little Fluffy Clouds"),
                    ("artist", "The Orb"),
                    ("album", "Adventures Beyond the Ultraworld"),
                    ("tracknumber", "1/10"),
                    ("discnumber", "2/2"),
                ]),
            ],
        }]
    }

    #[test]
    fn songs_albums_and_artists_come_from_the_tags() {
        let lib = Library::open(None).unwrap();
        assert!(lib.refresh(&sample(), &Rules::default()).unwrap());
        assert_eq!(lib.counts().unwrap(), (3, 2, 2));
        let db = lib.db();
        let (count, genres, dir, year, created): (i64, String, String, i64, i64) = db
            .query_row("SELECT song_count, genres, dir, year, created FROM albums WHERE name = 'Homogenic'", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap();
        assert_eq!((count, genres.as_str(), dir.as_str(), year, created), (2, "Electronic\u{1f}Art Pop", "Björk/Homogenic (1997)", 1997, 1_700_000_001));
        let (track, disc): (i64, i64) =
            db.query_row("SELECT track_no, disc_no FROM songs WHERE title = 'Little Fluffy Clouds'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((track, disc), (1, 2));
        let order: String = db.query_row("SELECT order_name FROM artists WHERE name = 'The Orb'", [], |r| r.get(0)).unwrap();
        assert_eq!(order, "orb");
    }

    #[test]
    fn built_again_only_when_the_files_change() {
        let lib = Library::open(None).unwrap();
        let mut folders = sample();
        assert!(lib.refresh(&folders, &Rules::default()).unwrap());
        assert!(!lib.refresh(&folders, &Rules::default()).unwrap());
        folders[0].rows[0].mtime = 2;
        assert!(lib.refresh(&folders, &Rules::default()).unwrap());
    }

    #[test]
    fn search_finds_words_and_their_beginnings_without_accents() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        let db = lib.db();
        let found = |table: &str, q: &str| -> i64 {
            let q = fts_query(q).unwrap();
            db.query_row(&format!("SELECT count(*) FROM {table} WHERE {table} MATCH ?1"), [q], |r| r.get(0)).unwrap()
        };
        assert_eq!(found("songs_fts", "joga"), 1, "diacritics are ignored");
        assert_eq!(found("songs_fts", "fluff"), 1, "a word's beginning finds it");
        assert_eq!(found("albums_fts", "bjork homo"), 1);
        assert_eq!(found("artists_fts", "orb"), 1);
        assert_eq!(fts_query("\" *"), None, "nothing left to search for");
        assert_eq!(fts_query("a OR b").as_deref(), Some("(\"a\" OR \"a\"*) AND (\"OR\" OR \"OR\"*) AND (\"b\" OR \"b\"*)"), "quoted: a word, not an operator");
    }

    #[test]
    fn articles_do_not_decide_the_order() {
        assert_eq!(order_name("The Beatles"), "beatles");
        assert_eq!(order_name("A Tribe Called Quest"), "tribe called quest");
        assert_eq!(order_name("Theatre"), "theatre");
        assert_eq!(year_of("1997-09-22"), Some(1997));
        assert_eq!(leading_number("03/12"), Some(3));
    }
}
