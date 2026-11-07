//! The OpenSubsonic library (decision 0030): songs, albums and artists as the API shows them, derived from the
//! catalogue into their own SQLite file (deleting it costs a rebuild). Patterns from Navidrome
//! (https://github.com/navidrome/navidrome, GPL-3.0-or-later, Copyright the Navidrome authors), translated:
//!
//! - an album's aggregates live on its row (songs, length, size, years, discs, genres, added = its oldest song);
//! - search through contentless FTS5 tables (`unicode61 remove_diacritics 2`), each word as `(word OR word*)`, ranked
//!   by bm25 with column weights (`persistence/sql_search_fts.go`); beside them (0.12: one matcher, search in general)
//!   trigram tables over each name's letters run together (`TRIGRAMS`), for the candidates the matcher judges;
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

use crate::userdata::overrides::Overrides;

use super::copies::{Copy, Kind, copy_for, in_order, keeping};
use super::ids;
use crate::catalogue::{Rules, Say, ServedRow, vote_of};

// journal_size_limit: as the catalogue's (the log's file cut back after a checkpoint).
const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA journal_size_limit = 8388608;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS songs (
  n INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, pid TEXT NOT NULL, folder INTEGER NOT NULL, rel TEXT NOT NULL, path TEXT NOT NULL,
  album_id TEXT NOT NULL, title TEXT NOT NULL, artist TEXT, album TEXT NOT NULL, album_artist TEXT,
  track_no INTEGER, disc_no INTEGER, year INTEGER, genre TEXT, bpm INTEGER, duration_ms INTEGER, size INTEGER NOT NULL,
  suffix TEXT, kbps INTEGER, created INTEGER, mbid TEXT, order_title TEXT NOT NULL, cover TEXT,
  rg_track_gain REAL, rg_track_peak REAL, rg_album_gain REAL, rg_album_peak REAL, genres TEXT NOT NULL DEFAULT '',
  dir TEXT NOT NULL DEFAULT '', src TEXT NOT NULL DEFAULT '', track_total INTEGER);
CREATE INDEX IF NOT EXISTS songs_by_album ON songs (album_id, disc_no, track_no, rel);
CREATE INDEX IF NOT EXISTS songs_by_title ON songs (order_title, n);
CREATE INDEX IF NOT EXISTS songs_by_pid ON songs (pid);
CREATE TABLE IF NOT EXISTS song_artists (song_id TEXT NOT NULL, artist_id TEXT NOT NULL, name TEXT NOT NULL,
  position INTEGER NOT NULL, PRIMARY KEY (song_id, position)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS albums (
  n INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, name TEXT NOT NULL, artist TEXT NOT NULL, artist_id TEXT NOT NULL,
  year INTEGER, original_year INTEGER, genre TEXT, genres TEXT NOT NULL, song_count INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL, size INTEGER NOT NULL, discs INTEGER NOT NULL, created INTEGER, mbid TEXT,
  compilation INTEGER NOT NULL, order_name TEXT NOT NULL, record INTEGER NOT NULL,
  format TEXT, min_kbps INTEGER, cover TEXT, release_type TEXT, type_from TEXT, secondary TEXT NOT NULL DEFAULT '', src TEXT NOT NULL DEFAULT '', type_why TEXT,
  set_fields TEXT NOT NULL DEFAULT '', whole_tracks INTEGER);
CREATE INDEX IF NOT EXISTS albums_by_name ON albums (order_name, n);
CREATE INDEX IF NOT EXISTS albums_by_created ON albums (created DESC, n);
CREATE INDEX IF NOT EXISTS albums_by_year ON albums (year, n);
CREATE INDEX IF NOT EXISTS albums_by_artist ON albums (artist_id, year, n);
CREATE TABLE IF NOT EXISTS album_artists (album_id TEXT NOT NULL, artist_id TEXT NOT NULL, name TEXT NOT NULL,
  position INTEGER NOT NULL, mbid TEXT, PRIMARY KEY (album_id, position)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS album_artists_by_artist ON album_artists (artist_id, album_id);
-- A copy (0100): what one folder holds of an album. `album` is `albums.id`: a record's id once ids are the records'.
CREATE TABLE IF NOT EXISTS copies (
  album TEXT NOT NULL, folder INTEGER NOT NULL, dir TEXT NOT NULL, format TEXT, min_kbps INTEGER, size INTEGER NOT NULL,
  added INTEGER, tracks INTEGER NOT NULL, bits INTEGER, rate INTEGER, short INTEGER NOT NULL, PRIMARY KEY (album, folder, dir)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS copies_by_place ON copies (folder, dir);
-- Every served file, with its own numbers. `dir` is the file's own folder, a copy's is what its files share: a copy over
-- CD1 and CD2 is one row above and has two `dir` here. `src` is the print of the row it was built from.
CREATE TABLE IF NOT EXISTS copy_files (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, dir TEXT NOT NULL, song TEXT NOT NULL, album TEXT NOT NULL, src TEXT NOT NULL,
  size INTEGER NOT NULL, suffix TEXT, kbps INTEGER, duration_ms INTEGER, bpm INTEGER,
  rg_track_gain REAL, rg_track_peak REAL, rg_album_gain REAL, rg_album_peak REAL, PRIMARY KEY (folder, rel)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS copy_files_by_dir ON copy_files (folder, dir);
CREATE INDEX IF NOT EXISTS copy_files_by_album ON copy_files (album);
CREATE INDEX IF NOT EXISTS copy_files_by_song ON copy_files (song);
CREATE INDEX IF NOT EXISTS song_artists_by_artist ON song_artists (artist_id, song_id);
CREATE TABLE IF NOT EXISTS artists (
  n INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, name TEXT NOT NULL, mbid TEXT, album_count INTEGER NOT NULL,
  song_count INTEGER NOT NULL, order_name TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS artists_by_name ON artists (order_name, n);
CREATE TABLE IF NOT EXISTS genres (name TEXT PRIMARY KEY, song_count INTEGER NOT NULL, album_count INTEGER NOT NULL);
CREATE VIRTUAL TABLE IF NOT EXISTS songs_fts USING fts5(title, artist, album, content='', contentless_delete=1, tokenize='unicode61 remove_diacritics 2');
CREATE VIRTUAL TABLE IF NOT EXISTS albums_fts USING fts5(name, artist, content='', contentless_delete=1, tokenize='unicode61 remove_diacritics 2');
CREATE VIRTUAL TABLE IF NOT EXISTS artists_fts USING fts5(name, content='', contentless_delete=1, tokenize='unicode61 remove_diacritics 2');
-- The indexes' terms (0.9.7): a query's word with typos stands for those it meets (`search::typo_terms`).
CREATE VIRTUAL TABLE IF NOT EXISTS songs_vocab USING fts5vocab(songs_fts, 'row');
CREATE VIRTUAL TABLE IF NOT EXISTS albums_vocab USING fts5vocab(albums_fts, 'row');
CREATE VIRTUAL TABLE IF NOT EXISTS artists_vocab USING fts5vocab(artists_fts, 'row');
";

/// The trigram tables (0.12: one matcher, search in general), beside the FTS tables: (table, the table it indexes, the
/// name it holds). Each holds a name's letters run together as the matcher reads them (`cratarr_letters`, the rules
/// crate's `matcher::letters`: separators gone, folded), `tokenize='trigram'` (SQLite 3.34 on; case folded, its
/// default), contentless as the FTS tables, the same row numbers. Search asks them for candidates however a name is
/// spelled or spaced ("hiphop" for Hip Hop Orchestra, "radiohed" for Radiohead) and the matcher judges them
/// (`read::trigram_extras`). Kept in step where the FTS tables' rows are written and deleted, in the same transaction
/// (`build_with`, `rebuild_dirs`, `write_albums`, `artists_again`); made and filled on open where missing
/// (`add_trigrams`). Named `*_tri_fts` so a library of another layout drops them as the FTS tables (their shadow
/// tables, `*_fts_*`, go with them).
const TRIGRAMS: [(&str, &str, &str); 3] = [("songs_tri_fts", "songs", "title"), ("albums_tri_fts", "albums", "name"), ("artists_tri_fts", "artists", "name")];

/// `cratarr_letters(text)` (0.12): a name's letters run together as the matcher reads them (`matcher::letters`: NFKD,
/// marks dropped, case folded, everything but letters and digits gone), for SQL: what the trigram tables are filled
/// with, on open as in every write. Deterministic; NULL stays NULL.
pub(crate) fn register_letters(conn: &Connection) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    conn.create_scalar_function(
        "cratarr_letters",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_INNOCUOUS,
        |ctx| Ok(ctx.get::<Option<String>>(0)?.map(|t| cratarr_rules::matcher::letters(&t))),
    )?;
    Ok(())
}

/// Libraries from before the trigram tables (0.12) get them, filled from the names they hold: one INSERT … SELECT each,
/// in one transaction (the songs' a few seconds on a large library at most). Safe to repeat.
fn add_trigrams(conn: &Connection) -> Result<()> {
    for (tri, table, column) in TRIGRAMS {
        let has: bool = conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?.exists([tri])?;
        if !has {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(&format!(
                "CREATE VIRTUAL TABLE {tri} USING fts5(letters, content='', contentless_delete=1, tokenize='trigram');
                 INSERT INTO {tri} (rowid, letters) SELECT n, cratarr_letters({column}) FROM {table};"
            ))
            .with_context(|| format!("making {tri}"))?;
            tx.commit()?;
        }
    }
    Ok(())
}

/// The secondary types kept (MusicBrainz's names, lower case; 0.13: the ones a record can be set to).
pub const SECONDARY: [&str; 12] =
    ["compilation", "soundtrack", "spokenword", "interview", "audiobook", "audio drama", "live", "remix", "dj-mix", "mixtape/street", "demo", "field recording"];

/// A release's types as kept (0.9.6): its primary type ("album", "ep", "single", "other") and its secondary ones
/// ("compilation", "live", "remix", "soundtrack"…), lower case, from MusicBrainz's names ("Album", "EP",
/// "Broadcast" as other) or Discogs' format descriptions ("LP", "EP", "Single", "Compilation"). Words of neither kind
/// are left out.
pub fn release_words<'a>(words: impl IntoIterator<Item = &'a str>) -> (Option<String>, Vec<String>) {
    let (mut primary, mut secondary): (Option<String>, Vec<String>) = (None, Vec::new());
    for w in words {
        let w = w.trim().to_lowercase();
        let main = match w.as_str() {
            "album" | "lp" => Some("album"),
            "ep" => Some("ep"),
            "single" => Some("single"),
            "broadcast" => Some("broadcast"),
            "other" => Some("other"),
            _ => None,
        };
        match main {
            Some(m) => {
                primary.get_or_insert_with(|| m.to_string());
            }
            None if SECONDARY.contains(&w.as_str()) && !secondary.contains(&w) => secondary.push(w),
            None => {}
        }
    }
    (primary, secondary)
}

/// A release type as the axes and OpenSubsonic show it ("EP", "DJ-mix", "Compilation").
pub fn type_label(word: &str) -> String {
    match word {
        "ep" => "EP".into(),
        "dj-mix" => "DJ-mix".into(),
        w => {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
    }
}

/// A release's types as kept (user data `release_types`): its primary type and its secondary ones.
pub type Kept = (Option<String>, Vec<String>);

/// An album as the type rule reads it: its tracks, their length and its longest (ms), its name, whether its album
/// artist is Various Artists. Its tracks are the library's, or the whole release's where tracks were found missing
/// (`Whole`).
struct Shape<'a> {
    tracks: usize,
    duration_ms: u64,
    longest_ms: u64,
    name: &'a str,
    various: bool,
}

/// The whole release of an album found missing tracks (`app::incomplete`, user data `album_complete`): how many tracks
/// it has, and their length and the longest (ms) where every one's is known. The type rule reads an incomplete album
/// as this (0.13, the owner, 2026-10-31: incomplete is a fact, not a type).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Whole {
    pub tracks: usize,
    pub duration_ms: Option<u64>,
    pub longest_ms: Option<u64>,
}

impl Shape<'_> {
    /// The album as its whole release has it, where tracks were found missing: never fewer tracks or shorter than the
    /// library's.
    fn whole(self, whole: Option<&Whole>) -> Self {
        let Some(w) = whole else { return self };
        Shape {
            tracks: self.tracks.max(w.tracks),
            duration_ms: self.duration_ms.max(w.duration_ms.unwrap_or(0)),
            longest_ms: self.longest_ms.max(w.longest_ms.unwrap_or(0)),
            ..self
        }
    }
}

const MINUTE_MS: u64 = 60_000;

/// An album's release type (0.13, the owner's rule, 2026-10-31; decision 0081's second amendment), on top of what
/// MusicBrainz's release group, else Discogs' format, says: MusicBrainz's Single is one song in versions, not a short
/// release, and an EP of either may be an album of twelve tracks. In order:
/// 1. The name wins: "… EP" an EP, "… LP" an album.
/// 2. Seven tracks or more, or over 45 minutes: an album (the owner's EPs reach 44 minutes, their albums start at 47).
/// 3. Four to six tracks: an EP.
/// 4. One to three: an EP where over 15 minutes or a track is 10 minutes or longer, else a single.
/// 5. A Single said (one song in versions) is an EP at most, however long.
///
/// An album found missing tracks is read as its whole release (`Whole`; 0.13, the owner: incomplete is no type). The
/// same for an album neither says anything of. A Broadcast or Other stays as said. A Various Artists album is a
/// compilation besides. (primary, where from, which rule where the rule decided, secondary): where from is the source
/// that said a type, none where none did; the rule ("name", "tracks", "length", "one song") only where the type is not
/// the one said.
fn release_type_of(
    musicbrainz: Option<&Kept>,
    discogs: Option<&Kept>,
    album: &Shape,
) -> (Option<String>, Option<&'static str>, Option<&'static str>, Vec<String>) {
    let (said, from, mut secondary) = match (musicbrainz, discogs) {
        (Some((Some(p), s)), _) => (Some(p.as_str()), Some("musicbrainz"), s.clone()),
        (_, Some((Some(p), s))) => (Some(p.as_str()), Some("discogs"), s.clone()),
        _ => (None, None, musicbrainz.or(discogs).map(|t| t.1.clone()).unwrap_or_default()),
    };
    if album.various && !secondary.iter().any(|s| s == "compilation") {
        secondary.push("compilation".into());
    }
    if matches!(said, Some("other" | "broadcast")) || album.tracks == 0 {
        return (said.map(str::to_string), from, None, secondary);
    }
    let (kind, why) = if let Some(kind) = named(album.name) {
        (kind, "name")
    } else {
        let (kind, why) = if album.tracks >= 7 {
            ("album", "tracks")
        } else if album.duration_ms > 45 * MINUTE_MS {
            ("album", "length")
        } else if album.tracks >= 4 {
            ("ep", "tracks")
        } else if album.duration_ms > 15 * MINUTE_MS || album.longest_ms >= 10 * MINUTE_MS {
            ("ep", "length")
        } else {
            ("single", "tracks")
        };
        if kind == "album" && said == Some("single") { ("ep", "one song") } else { (kind, why) }
    };
    (Some(kind.into()), from, (said != Some(kind)).then_some(why), secondary)
}

/// Another server's album typed by the owner's rule (0.13, the owner: a record's type is always known): its track
/// count, length and name as its shape (its longest track not known there), nothing said of it.
pub fn type_by_rule(album: &mut crate::source::Album) {
    if album.release.kind.is_some() {
        return;
    }
    let shape = Shape {
        tracks: album.tracks as usize,
        duration_ms: album.duration_secs.map_or(0, |s| (s.max(0.0) * 1000.0) as u64),
        longest_ms: 0,
        name: &album.title,
        various: album.artist.as_deref().is_some_and(|a| a.eq_ignore_ascii_case("Various Artists")),
    };
    let (kind, _, why, secondary) = release_type_of(None, None, &shape);
    album.release.kind = kind;
    album.release.type_why = why.map(str::to_string);
    album.release.secondary = secondary;
}

/// The type an album's name gives: its last word "EP" an EP, "LP" an album (any case).
fn named(name: &str) -> Option<&'static str> {
    name.split(|c: char| !c.is_alphanumeric()).rev().find_map(|w| match w.to_ascii_lowercase().as_str() {
        "ep" => Some("ep"),
        "lp" => Some("album"),
        _ => None,
    })
}

#[cfg(test)]
mod release_type_tests {
    use super::*;

    fn shape(name: &str, tracks: usize, minutes: f64, longest: f64) -> Shape<'_> {
        let ms = |m: f64| (m * MINUTE_MS as f64) as u64;
        Shape { tracks, duration_ms: ms(minutes), longest_ms: ms(longest), name, various: false }
    }

    fn said(kind: &str) -> Kept {
        (Some(kind.into()), Vec::new())
    }

    /// (type, rule) for an album as `kept` says.
    fn typed(kept: Option<&Kept>, album: Shape) -> (String, Option<&'static str>) {
        let (kind, _, why, _) = release_type_of(kept, None, &album);
        (kind.unwrap_or_default(), why)
    }

    #[test]
    fn release_words_read_musicbrainz_s_and_discogs_names() {
        assert_eq!(release_words(["EP", "Live", "Remastered"]), (Some("ep".into()), vec!["live".into()]));
        assert_eq!(release_words(["LP", "Album", "Compilation"]), (Some("album".into()), vec!["compilation".into()]));
        assert_eq!(release_words(["Broadcast"]).0.as_deref(), Some("broadcast"));
        assert_eq!(release_words(["Other"]).0.as_deref(), Some("other"));
        assert_eq!(type_label("ep"), "EP");
        assert_eq!(type_label("compilation"), "Compilation");
    }

    /// The owner's verdicts on the Library's Singles and EPs (2026-10-31).
    #[test]
    fn the_owner_s_rule_types_by_tracks_and_length_on_top_of_what_is_said() {
        let (single, ep) = (said("single"), said("ep"));
        // MusicBrainz's Singles that are EPs: long, four tracks, or named so.
        assert_eq!(typed(Some(&single), shape("Passiflora", 1, 21.7, 21.7)), ("ep".into(), Some("length")));
        assert_eq!(typed(Some(&single), shape("Take California", 4, 18.0, 6.0)), ("ep".into(), Some("tracks")));
        assert_eq!(typed(Some(&single), shape("Let's Get Down EP", 2, 9.0, 5.0)), ("ep".into(), Some("name")));
        assert_eq!(typed(Some(&single), shape("Melbourne Bolero", 2, 14.0, 10.0)), ("ep".into(), Some("length")), "a track of 10 minutes");
        // One song in versions stays an EP, however long.
        assert_eq!(typed(Some(&single), shape("Kime Ne", 6, 56.6, 12.0)), ("ep".into(), Some("one song")));
        assert_eq!(typed(Some(&single), shape("Kime Ne", 8, 56.6, 12.0)), ("ep".into(), Some("one song")));
        // Short ones stay singles, as said.
        assert_eq!(typed(Some(&single), shape("Nautiluss", 2, 11.0, 6.0)), ("single".into(), None));
        // EPs that are albums: seven tracks or more, however short, or over 45 minutes.
        assert_eq!(typed(Some(&ep), shape("Plantation", 10, 63.0, 8.0)), ("album".into(), Some("tracks")));
        assert_eq!(typed(Some(&ep), shape("Just Piano", 8, 20.0, 3.0)), ("album".into(), Some("tracks")));
        assert_eq!(typed(Some(&ep), shape("Flowery Rave Tales", 6, 47.1, 9.0)), ("album".into(), Some("length")));
        // The owner's longest EPs stay EPs.
        assert_eq!(typed(Some(&ep), shape("Afro Damba", 6, 43.6, 9.0)), ("ep".into(), None));
        assert_eq!(typed(Some(&ep), shape("Awake Into Life", 4, 42.4, 12.0)), ("ep".into(), None));
        // An EP of a few short tracks is a single.
        assert_eq!(typed(Some(&ep), shape("Islets In Pink Polypropylene", 2, 8.0, 4.5)), ("single".into(), Some("tracks")));
        // The name wins over the length both ways.
        assert_eq!(typed(Some(&ep), shape("Long Player LP", 3, 12.0, 4.0)), ("album".into(), Some("name")));
        assert_eq!(named("Epoch"), None);
        assert_eq!(named("Night Walks (EP)"), Some("ep"));
    }

    #[test]
    fn an_album_found_missing_tracks_is_typed_as_its_whole_release() {
        let whole = |tracks, minutes: f64, longest: f64| Whole { tracks, duration_ms: Some((minutes * MINUTE_MS as f64) as u64), longest_ms: Some((longest * MINUTE_MS as f64) as u64) };
        // Two of an eleven-track album: an album, not a single.
        assert_eq!(typed(Some(&said("album")), shape("Västertorp", 2, 9.0, 5.0).whole(Some(&whole(11, 50.0, 6.0)))), ("album".into(), None));
        assert_eq!(typed(None, shape("Escape Reality", 2, 9.0, 5.0).whole(Some(&whole(5, 24.0, 6.0)))), ("ep".into(), Some("tracks")));
        // The name still wins.
        assert_eq!(typed(Some(&said("ep")), shape("Outsider Music EP", 1, 7.0, 7.0).whole(Some(&whole(9, 40.0, 7.0)))), ("ep".into(), None));
        // Lengths not known: its tracks count, the library's length at least.
        let counted = Whole { tracks: 8, duration_ms: None, longest_ms: None };
        assert_eq!(typed(None, shape("Odd", 3, 12.0, 4.0).whole(Some(&counted))), ("album".into(), Some("tracks")));
        assert_eq!(typed(Some(&said("ep")), shape("Speed Dembow Vol.II", 1, 4.0, 4.0)).0, "single", "not found missing: as it is");
    }

    #[test]
    fn another_server_s_album_is_typed_by_the_rule() {
        let album = |title: &str, tracks: u32, secs: f64, artist: &str| crate::source::Album {
            id: "x".into(),
            title: title.into(),
            artist: Some(artist.into()),
            year: None,
            added: None,
            path: None,
            tracks,
            format: None,
            bitrate_kbps: None,
            duration_secs: Some(secs),
            tags: Vec::new(),
            library: None,
            copies: Vec::new(),
            artists: Vec::new(),
            release: Default::default(),
        };
        let typed = |mut a: crate::source::Album| {
            type_by_rule(&mut a);
            (a.release.kind, a.release.secondary)
        };
        assert_eq!(typed(album("Odd", 10, 2700.0, "Ott")), (Some("album".into()), Vec::<String>::new()));
        assert_eq!(typed(album("Odd", 5, 1300.0, "Ott")), (Some("ep".into()), Vec::new()));
        assert_eq!(typed(album("Odd", 2, 420.0, "Ott")), (Some("single".into()), Vec::new()));
        assert_eq!(typed(album("Hits", 14, 3600.0, "Various Artists")), (Some("album".into()), vec!["compilation".to_string()]));
        let mut said = album("Odd", 2, 420.0, "Ott");
        said.release.kind = Some("album".into());
        assert_eq!(typed(said).0.as_deref(), Some("album"), "what is known stays");
    }

    #[test]
    fn broadcast_and_other_stay_as_said() {
        assert_eq!(typed(Some(&said("broadcast")), shape("Essential Mix", 1, 120.0, 120.0)), ("broadcast".into(), None));
        assert_eq!(typed(Some(&said("other")), shape("Odd", 12, 60.0, 6.0)), ("other".into(), None));
    }

    #[test]
    fn where_nothing_is_said_the_same_rule_and_other_stays() {
        assert_eq!(release_type_of(None, None, &shape("Odd", 5, 22.0, 6.0)), (Some("ep".into()), None, Some("tracks"), Vec::new()));
        assert_eq!(release_type_of(None, None, &shape("Odd", 2, 7.0, 4.0)), (Some("single".into()), None, Some("tracks"), Vec::new()));
        let dc = said("ep");
        assert_eq!(release_type_of(None, Some(&dc), &shape("Odd", 4, 20.0, 6.0)), (Some("ep".into()), Some("discogs"), None, Vec::new()));
        let mb = (Some("album".to_string()), vec!["live".to_string()]);
        assert_eq!(release_type_of(Some(&mb), Some(&dc), &shape("Odd", 12, 50.0, 6.0)), (Some("album".into()), Some("musicbrainz"), None, vec!["live".into()]));
        let broadcast = said("other");
        assert_eq!(release_type_of(Some(&broadcast), None, &shape("Radio Show", 1, 120.0, 120.0)).0.as_deref(), Some("other"));
    }

    #[test]
    fn a_various_artists_album_is_a_compilation_besides() {
        let album = Shape { various: true, ..shape("COD3 QR 015", 9, 59.0, 8.0) };
        assert_eq!(release_type_of(None, None, &album), (Some("album".into()), None, Some("tracks"), vec!["compilation".into()]));
        let soundtrack = (Some("album".to_string()), vec!["soundtrack".to_string(), "compilation".to_string()]);
        assert_eq!(release_type_of(Some(&soundtrack), None, &album).3, vec!["soundtrack".to_string(), "compilation".to_string()], "once");
    }
}

/// The tables' layout; a library of another layout is built again.
const LAYOUT: i64 = 22;

/// The meta key of the catalogue state the library was built from.
const BUILT_FROM: &str = "built_from";

/// What changed since the library was built: nothing, only the analyses (their gains and tempi are the songs'), or the
/// songs themselves (files, tags, the rules, the added dates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    None,
    Analyses,
    Songs,
}

/// The change from what the library was built from (`was`) to now, and the print to note: the songs' part and the
/// analyses' part, apart. A print from before they were apart reads as the songs changed: built once more.
fn change_between(was: Option<&str>, songs: &str, analyses: &str) -> (Change, String) {
    let print = format!("{songs} | analysis {analyses}");
    let change = match was {
        Some(w) if w == print => Change::None,
        Some(w) if w.split_once(" | analysis ").is_some_and(|(s, _)| s == songs) => Change::Analyses,
        _ => Change::Songs,
    };
    (change, print)
}

/// One folder of the catalogue as the library reads it: its id there and its files.
pub struct FolderRows {
    pub folder: i64,
    /// The folder as Cratarr sees it; a song's `path` is this and its place in it.
    pub root: String,
    pub rows: Vec<ServedRow>,
}

pub struct Library {
    db: Mutex<Connection>,
    /// When albums were added, where known better than the catalogue's first sight of their files (imported from a
    /// media server): an album's `created` is the earlier of the two.
    added: Mutex<HashMap<String, i64>>,
    /// Albums' original years from MusicBrainz (0.9.5), by their MusicBrainz release: for an album whose files carry no
    /// original date.
    originals: Mutex<HashMap<String, u32>>,
    /// The release types found (0.9.6), by `mb:<release>` and `dc:<album id>` (user data `release_types`).
    release_types: Mutex<HashMap<String, Kept>>,
    /// The albums found missing tracks, by album id, with their whole release (user data `album_complete`; 0.13: typed
    /// as it).
    incomplete: Mutex<HashMap<String, Whole>>,
    /// A record's facts set by hand (0.13), by its id (user data `record_overrides`).
    overrides: Mutex<HashMap<String, Overrides>>,
    /// Every artist's id for life, by `ids::artist_key`: the catalogue's, as `credit` last read them.
    artists: Mutex<HashMap<String, i64>>,
    /// What each catalogue folder's library is, in the libraries' order (`set_kinds`).
    kinds: Mutex<Vec<(i64, Kind)>>,
    /// Discogs' genres and styles for albums whose files carry none (0.9.5), by album id.
    extra_genres: Mutex<HashMap<String, Vec<String>>>,
    /// One build at a time (0.9.8): a check's build reads every row first, and must not write over the albums Cratarr's
    /// own write has just built again from newer rows.
    building: Mutex<()>,
}

/// A number at the start of a tag ("3/12" is 3).
/// A song's ReplayGain (decision 0035): from its analysis once it has one (every track levelled the same way), else
/// from the file's own tags (`REPLAYGAIN_*`; Opus's `R128_*`, which count from -23 LUFS in 1/256 dB, so 5 dB more for
/// ReplayGain's -18). The album's from its tracks' analyses once all have one, else the same way from the tags.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Gains {
    pub track_gain: Option<f64>,
    pub track_peak: Option<f64>,
    pub album_gain: Option<f64>,
    pub album_peak: Option<f64>,
}

/// "-6.53 dB", "+1.2 dB", "0.988": the number at the start.
fn decibels(v: &str) -> Option<f64> {
    let t = v.trim();
    let end = t.find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.'))).unwrap_or(t.len());
    t[..end].parse::<f64>().ok().filter(|x| x.is_finite())
}

fn r128(v: &str) -> Option<f64> {
    v.trim().parse::<i32>().ok().map(|q| f64::from(q) / 256.0 + 5.0)
}

pub(super) fn gains(r: &ServedRow) -> Gains {
    let tag_track = || r.first("replaygain_track_gain").and_then(decibels).or_else(|| r.first("r128_track_gain").and_then(r128));
    let tag_album = || r.first("replaygain_album_gain").and_then(decibels).or_else(|| r.first("r128_album_gain").and_then(r128));
    let peak = |name: &str| r.first(name).and_then(decibels).filter(|p| *p > 0.0);
    let (track_gain, track_peak) = match &r.analysis {
        Some(a) => (Some(f64::from(a.track_gain_db)), Some(f64::from(a.track_peak))),
        None => (tag_track(), peak("replaygain_track_peak")),
    };
    let (album_gain, album_peak) = match r.analysis.as_ref().and_then(|a| a.album) {
        Some((g, p)) => (Some(f64::from(g)), Some(f64::from(p))),
        None => (tag_album(), peak("replaygain_album_peak")),
    };
    Gains { track_gain, track_peak, album_gain, album_peak }
}

/// A song's tempo (decision 0035): the file's own BPM tag, else its analysis' when that is sure enough.
pub(super) fn bpm_of(r: &ServedRow) -> Option<u32> {
    r.first("bpm")
        .and_then(leading_number)
        .or_else(|| r.analysis.as_ref().and_then(|a| a.bpm).map(|b| b.round()).filter(|b| *b > 0.0).map(|b| b as u32))
}

/// The record's track count a song's tags say (0.14): TRACKNUMBER's "05/06", else its total (Vorbis TRACKTOTAL or
/// TOTALTRACKS; lofty splits ID3v2's TRCK and MP4's trkn into the number and the total).
fn total_of(r: &ServedRow) -> Option<u32> {
    r.first("tracknumber").and_then(|v| v.split_once('/')).and_then(|(_, t)| leading_number(t)).or_else(|| r.first("totaltracks").and_then(leading_number))
}

pub(crate) fn leading_number(v: &str) -> Option<u32> {
    let digits: String = v.trim().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|n| *n > 0)
}

/// The year in a date tag ("1997-05-12", "1997").
/// A song as a file of a folder knows it ([`Library::songs_by_rel`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RelSong {
    pub song: String,
    pub album: String,
    pub duration_ms: Option<i64>,
    /// When it came (epoch seconds).
    pub created: Option<i64>,
    /// Its genres, as its tags have them: what a playlist's top genres count.
    pub genres: Vec<String>,
}

pub fn year_of(v: &str) -> Option<u32> {
    let y: String = v.trim().chars().take(4).collect();
    // `0001-01-01` is a tagger's placeholder, not a year (12 albums on the real library): as a real date it would beat the
    // original year, and a copy with it would give its record the year 1.
    (y.len() == 4).then(|| y.parse().ok()).flatten().filter(|y| *y > 1)
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
    /// The record its album is (`songs_of`) and that record's id: what most of its group's tracks hold, so a stray shows
    /// with the album its file lies in.
    record: i64,
    album_id: String,
    /// What gathers one folder's files of an album: the hash of its tags, the folder in front, its id until 0.16.
    group: String,
    /// The same as it is (or would be) in Music, folder 0: what the first start finds what was kept by it through (N5).
    album_base: String,
    genres: Vec<String>,
}

/// The album artist of a file that credits none, not even an artist of its own.
pub const UNKNOWN_ARTIST: &str = "[Unknown Artist]";

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
    let album_artist = row.first("albumartist").map(str::to_string).or_else(|| artist_credit.clone()).unwrap_or_else(|| UNKNOWN_ARTIST.into());
    let mut album_artists = list("albumartists", "albumartist");
    if album_artists.is_empty() {
        album_artists = if artists.is_empty() { vec![album_artist.clone()] } else { artists.clone() };
    }
    let album = row.first("album").map(str::to_string).unwrap_or_else(|| "[Unknown Album]".into());
    let group = ids::album(row, &album_artist, folder);
    let album_base = ids::album(row, &album_artist, 0);
    let genres = rules.values("genre", &row.all("genre").map(str::to_string).collect::<Vec<_>>());
    Song { row, folder, title, artists, artist_credit, album, album_artist, album_artists, record: 0, album_id: String::new(), group, album_base, genres }
}

/// What these files say, cheaply: when it differs from what the library was built from, it is built again (tests;
/// the server checks the catalogue itself, `change` and `build_from`).
#[cfg(test)]
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
        let conn = crate::db::open(file)?;
        conn.busy_timeout(Duration::from_secs(15))?;
        register_letters(&conn)?;
        // Derived data: a library from an older layout is simply built again.
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != LAYOUT {
            let tables: Vec<String> = conn
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '%_fts_%'")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for t in tables {
                conn.execute_batch(&format!("DROP TABLE IF EXISTS \"{t}\""))?;
            }
        }
        conn.execute_batch(SCHEMA).context("creating the library tables")?;
        add_trigrams(&conn)?;
        conn.execute_batch(&format!("PRAGMA user_version = {LAYOUT}"))?;
        Ok(Self {
            db: Mutex::new(conn),
            added: Mutex::default(),
            originals: Mutex::default(),
            extra_genres: Mutex::default(),
            release_types: Mutex::default(),
            incomplete: Mutex::default(),
            overrides: Mutex::default(),
            artists: Mutex::default(),
            kinds: Mutex::default(),
            building: Mutex::new(()),
        })
    }

    /// The added dates known better than the catalogue's (by album id); a change builds the library again.
    pub fn set_added(&self, added: HashMap<String, i64>) {
        *self.added.lock().unwrap_or_else(|p| p.into_inner()) = added;
    }

    /// The artists' ids for life, by `ids::artist_key`.
    pub fn set_artists(&self, artists: HashMap<String, i64>) {
        *self.artists.lock().unwrap_or_else(|p| p.into_inner()) = artists;
    }

    /// Every name these rows credit has its id, a new one given by the catalogue. What is kept is good for life, so the
    /// catalogue is asked only for a name not among it, and a restart, which keeps nothing, asks once.
    fn credit<'r>(&self, cat: &crate::catalogue::Catalogue, rows: impl Iterator<Item = (i64, &'r ServedRow)>, rules: &Rules) -> Result<()> {
        let names = credited_artists(rows, rules);
        let known = |artists: &HashMap<String, i64>| names.iter().all(|n| artists.contains_key(&ids::artist_key(n)));
        if !known(&self.artists.lock().unwrap_or_else(|p| p.into_inner())) {
            self.set_artists(cat.artists_for(&names).context("giving the artists their ids")?);
        }
        Ok(())
    }

    /// The original years known from MusicBrainz (by MusicBrainz release); a change builds the library again.
    pub fn set_originals(&self, originals: HashMap<String, u32>) {
        *self.originals.lock().unwrap_or_else(|p| p.into_inner()) = originals;
    }

    /// The release types found on MusicBrainz and Discogs (0.9.6); a change builds the library again.
    pub fn set_release_types(&self, types: HashMap<String, Kept>) {
        *self.release_types.lock().unwrap_or_else(|p| p.into_inner()) = types;
    }

    /// The records' facts set by hand (by record, 0.13); a change builds the library again.
    pub fn set_overrides(&self, overrides: HashMap<String, Overrides>) {
        *self.overrides.lock().unwrap_or_else(|p| p.into_inner()) = overrides;
    }

    /// The albums found missing tracks, with their whole release (by album id, 0.13); a change builds the library again.
    pub fn set_incomplete(&self, albums: HashMap<String, Whole>) {
        *self.incomplete.lock().unwrap_or_else(|p| p.into_inner()) = albums;
    }

    /// Discogs' genres for albums whose files carry none (by album id); a change builds the library again.
    pub fn set_extra_genres(&self, genres: HashMap<String, Vec<String>>) {
        *self.extra_genres.lock().unwrap_or_else(|p| p.into_inner()) = genres;
    }

    /// What each catalogue folder's library is, in the libraries' order: what a copy is judged by, so it is set before
    /// the first build (`main`) and again at each check. A change builds the whole library.
    pub fn set_kinds(&self, kinds: Vec<(i64, Kind)>) {
        *self.kinds.lock().unwrap_or_else(|p| p.into_inner()) = kinds;
    }

    pub fn kinds(&self) -> Vec<(i64, Kind)> {
        self.kinds.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// What a folder's library is, and its place in the libraries' order. With no word from the app (a test without one;
    /// `main` sets them before anything is served): the first folder is the Music folder, the others wait in Incoming.
    pub fn kind_of(&self, folder: i64) -> (Kind, usize) {
        kind_in(&self.kinds.lock().unwrap_or_else(|p| p.into_inner()), folder)
    }

    /// A summary of the original years, part of what the library was built from.
    fn originals_print(&self) -> String {
        let o = self.originals.lock().unwrap_or_else(|p| p.into_inner());
        let g = self.extra_genres.lock().unwrap_or_else(|p| p.into_inner());
        // The release types too (0.9.6): without them here, types that came after a build never built the library
        // again (seen on the sim, 2026-10-26: 21 kept, none shown).
        let t = self.release_types.lock().unwrap_or_else(|p| p.into_inner());
        // The albums found missing tracks too (0.13): they are typed as their whole release.
        use md5::Digest;
        let mut i: Vec<String> = self.incomplete.lock().unwrap_or_else(|p| p.into_inner()).iter().map(|(a, w)| format!("{a} {w:?}")).collect();
        i.sort_unstable();
        // And the facts set by hand (0.13).
        let set: BTreeMap<String, Overrides> =
            self.overrides.lock().unwrap_or_else(|p| p.into_inner()).iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        format!(
            "{}/{} genres {}/{} types {}/{} incomplete {:x} set {:x}",
            o.len(),
            o.values().fold(0u64, |a, v| a.wrapping_add(u64::from(*v))),
            g.len(),
            g.values().map(Vec::len).sum::<usize>(),
            t.len(),
            t.values().map(|v| usize::from(v.0.is_some()) + v.1.len()).sum::<usize>(),
            md5::Md5::digest(i.join("\n").as_bytes()),
            md5::Md5::digest(serde_json::to_string(&set).unwrap_or_default().as_bytes())
        )
    }

    /// A summary of the added dates, part of what the library was built from.
    fn added_print(&self) -> String {
        let added = self.added.lock().unwrap_or_else(|p| p.into_inner());
        format!("{}/{}", added.len(), added.values().fold(0i64, |a, v| a.wrapping_add(*v)))
    }

    pub(crate) fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Builds the library again from these files, unless it was built from them already. Returns whether it was
    /// built. Blocking.
    #[cfg(test)]
    pub fn refresh(&self, folders: &[FolderRows], rules: &Rules) -> Result<bool> {
        // The print of the files as given: a test's rows have no records until `build` gives them theirs.
        let print = fingerprint(folders);
        if self.built_from().as_deref() == Some(print.as_str()) {
            return Ok(false);
        }
        self.build(folders, rules)?;
        self.set_built_from(&print)
    }

    /// Builds the library again from the catalogue when its files changed (a cheap check first), reading through a
    /// connection of its own so the catalogue is not held up meanwhile (a full read takes seconds). Returns whether it
    /// was built. Blocking. The server does it in its two steps (`change`, then `build_from`), to tell new analyses from
    /// changed songs; this is the timing test's.
    #[cfg(test)]
    pub fn refresh_from(&self, cat: &crate::catalogue::Catalogue, rules: &Rules) -> Result<bool> {
        let (change, print) = self.change(cat, rules)?;
        if change == Change::None {
            return Ok(false);
        }
        self.build_from(cat, rules, &print)?;
        Ok(true)
    }

    /// What changed since the library was built (a cheap check: the catalogue's counts and sums), and the print to
    /// build it with. Blocking.
    pub fn change(&self, cat: &crate::catalogue::Catalogue, rules: &Rules) -> Result<(Change, String)> {
        let served = match cat.reader() {
            Some(c) => crate::catalogue::served_fingerprint(&c)?,
            None => cat.served_fingerprint()?,
        };
        // The analyses' part is the fingerprint's tail (`… analysis <count>/<sum of when>`).
        let (files, analyses) = served.split_once(" analysis ").unwrap_or((served.as_str(), ""));
        // The folders by their paths, not how many: a library renamed changes no file, and what was built lists it by the path it had.
        let songs = format!(
            "catalogue {} {:?} {} kinds {:?} rules {:x} files {}",
            cat.generation(),
            cat.folders(),
            format!("{} originals {}", self.added_print(), self.originals_print()),
            self.kinds(),
            {
                use md5::Digest;
                md5::Md5::digest(serde_json::to_string(rules)?.as_bytes())
            },
            files
        );
        Ok(change_between(self.built_from().as_deref(), &songs, analyses))
    }

    /// Brings the library up to the catalogue, `print` (from `change`) noted as what it was built from (0.9.8): only the
    /// albums of the files that changed are built again (`update_rows`), unless what it is built from besides its files
    /// changed (`context`: the rules, the folders, a new catalogue), or that fails; then the whole of it. Whether it was
    /// built whole, and how many folders were built again. Blocking.
    pub fn build_from(&self, cat: &crate::catalogue::Catalogue, rules: &Rules, print: &str) -> Result<(bool, usize)> {
        let _one = self.building.lock().unwrap_or_else(|p| p.into_inner());
        let reader = cat.reader();
        let read = || {
            cat.folders()
                .iter()
                .map(|(folder, root)| {
                    let rows = match &reader {
                        Some(c) => crate::catalogue::served_rows(c, folder)?,
                        None => cat.served_rows(folder)?,
                    };
                    Ok(FolderRows { folder, root: root.display().to_string(), rows })
                })
                .collect::<Result<Vec<_>>>()
        };
        cat.reading_albums();
        let mut folders = read()?;
        // New tracks get their records first, and strays their albums': what the library lists is what a record is.
        let mut copies = album_copies(&folders, rules);
        let kinds = self.kinds();
        copies.iter_mut().for_each(|a| a.rank = keeping(kind_in(&kinds, a.folder).0));
        if cat.records_unsettled() || copies.iter().any(|a| a.split) {
            // A row with no record is not served, so a build going on without them would take those albums off the lists,
            // and note itself as up to date. The library stays as it is until the next pass can give them.
            let given = cat.assign_records(&copies).context("giving the albums their records")?;
            // A row says its record, and `row_src` prints it: built from rows read before, a track given one now would
            // build without it, and build again at the next pass.
            if given > 0 {
                folders = read()?;
            }
        }
        drop(copies);
        // As for records: a new name has its id before anything written names it.
        self.credit(cat, folders.iter().flat_map(|f| f.rows.iter().map(move |r| (f.folder, r))), rules)?;
        let covers = match &reader {
            Some(c) => crate::catalogue::cover_prints(c)?,
            None => cat.cover_prints()?,
        };
        let roots: Vec<(i64, String)> = folders.iter().map(|f| (f.folder, f.root.clone())).collect();
        let context = context(&cat.generation(), &roots, &self.kinds(), rules)?;
        let (whole, dirs) = if self.built_context().as_deref() != Some(context.as_str()) {
            self.build_with(&folders, rules, &covers)?;
            (true, 0)
        } else {
            match self.update_rows(&folders, rules, &covers) {
                Ok(n) => (false, n),
                Err(e) => {
                    tracing::warn!(target: "library", reason = %format!("{e:#}"), "the changed albums could not be built alone: built whole");
                    self.build_with(&folders, rules, &covers)?;
                    (true, 0)
                }
            }
        };
        self.set_built_context(&context)?;
        self.set_built_from(print)?;
        Ok((whole, dirs))
    }

    fn built_from(&self) -> Option<String> {
        self.db().query_row("SELECT value FROM meta WHERE key = ?1", [BUILT_FROM], |r| r.get(0)).optional().ok().flatten()
    }

    fn set_built_from(&self, print: &str) -> Result<bool> {
        self.db().execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![BUILT_FROM, print])?;
        Ok(true)
    }

    /// The kept facts albums are built with besides their files, as they are now.
    fn facts(&self) -> Facts {
        Facts {
            added: self.added.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            originals: self.originals.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            extra_genres: self.extra_genres.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            release_types: self.release_types.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            incomplete: self.incomplete.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            overrides: self.overrides.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            artists: self.artists.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            kinds: self.kinds(),
        }
    }

    #[cfg(test)]
    fn build(&self, folders: &[FolderRows], rules: &Rules) -> Result<()> {
        self.credit_in_tests(folders, rules);
        self.build_with(&with_records(folders, rules), rules, &Covers::new())
    }

    /// Stands in for `build_from` giving these rows' names their ids, in tests that write with no catalogue.
    #[cfg(test)]
    fn credit_in_tests(&self, folders: &[FolderRows], rules: &Rules) {
        for name in credited_artists(folders.iter().flat_map(|f| f.rows.iter().map(move |r| (f.folder, r))), rules) {
            self.artists.lock().unwrap_or_else(|p| p.into_inner()).entry(ids::artist_key(&name)).or_insert(artist_number_in_tests(&name));
        }
    }

    /// Builds the whole library from these files; `covers`, each folder's cover files (`catalogue::cover_prints`).
    fn build_with(&self, folders: &[FolderRows], rules: &Rules, covers: &Covers) -> Result<()> {
        let facts = self.facts();
        let roots: HashMap<i64, &str> = folders.iter().map(|f| (f.folder, f.root.as_str())).collect();
        let rows: Vec<(i64, &ServedRow)> = folders.iter().flat_map(|f| f.rows.iter().map(move |r| (f.folder, r))).collect();
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute_batch(
            "DELETE FROM songs; DELETE FROM song_artists; DELETE FROM albums; DELETE FROM album_artists; DELETE FROM artists;
             DELETE FROM genres; DELETE FROM songs_fts; DELETE FROM albums_fts; DELETE FROM artists_fts;
             DELETE FROM songs_tri_fts; DELETE FROM albums_tri_fts; DELETE FROM artists_tri_fts;
             DELETE FROM copies; DELETE FROM copy_files;",
        )?;
        write_albums(&tx, &rows, &roots, rules, &facts, covers)?;
        artists_again(&tx, None)?;
        genres_again(&tx)?;
        tx.execute_batch("UPDATE songs SET cover = (SELECT a.cover FROM albums a WHERE a.id = songs.album_id);")?;
        // Where each catalogue folder is, so the library can be read before the catalogue is open (decision 0032).
        let roots: Vec<(i64, &str)> = folders.iter().map(|f| (f.folder, f.root.as_str())).collect();
        tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('roots', ?1)", [serde_json::to_string(&roots)?])?;
        tx.commit()?;
        db.execute_batch("ANALYZE;")?;
        Ok(())
    }

    /// Builds again only the albums of the files that changed since they were built (0.9.8, the owner: a fix, a cover,
    /// a Renumber showed only after a whole build, half a minute on the NAS): each file keeps the print of the row it
    /// was built from, each album its kept facts' and cover files'; the folders where any differs, or a file came or
    /// went, are built again, with every folder their albums reach. The number of folders built again. Blocking.
    fn update_rows(&self, folders: &[FolderRows], rules: &Rules, covers: &Covers) -> Result<usize> {
        let facts = self.facts();
        let now: HashMap<(i64, &str), &ServedRow> =
            folders.iter().flat_map(|f| f.rows.iter().filter(|r| r.track.is_some() && r.record.is_some()).map(move |r| ((f.folder, r.rel.as_str()), r))).collect();
        let built = self.built_files()?;
        let mut dirs: BTreeSet<(i64, String)> = BTreeSet::new();
        for ((folder, rel), r) in &now {
            if built.get(&(*folder, rel.to_string())).is_none_or(|b| b.src != row_src(r)) {
                dirs.insert((*folder, crate::catalogue::dir_of(rel).to_string()));
            }
        }
        for ((folder, rel), b) in &built {
            if !now.contains_key(&(*folder, rel.as_str())) {
                dirs.insert((*folder, b.dir.clone()));
            }
        }
        // Albums whose kept facts or cover files changed. Only the copy that plays shows in `albums`, so only it is compared:
        // a cover changed in another copy changes nothing built.
        for a in self.built_albums()? {
            if album_src(&a.id, a.release.as_deref(), a.folder, &a.dir, &facts, covers) != a.src {
                dirs.extend(built.values().filter(|b| b.album == a.id).map(|b| (b.folder, b.dir.clone())));
            }
        }
        if dirs.is_empty() {
            return Ok(0);
        }
        let by_dir: HashMap<(i64, String), Vec<ServedRow>> = folders.iter().fold(HashMap::new(), |mut m, f| {
            for r in &f.rows {
                m.entry((f.folder, crate::catalogue::dir_of(&r.rel).to_string())).or_insert_with(Vec::new).push(r.clone());
            }
            m
        });
        let roots: HashMap<i64, &str> = folders.iter().map(|f| (f.folder, f.root.as_str())).collect();
        self.rebuild_dirs(dirs, &mut |folder, dir| Ok(by_dir.get(&(folder, dir.to_string())).cloned().unwrap_or_default()), &roots, rules, covers)
    }

    /// Builds again the albums of the folders `dirs` holds (of library folders), and of every folder those albums and their
    /// tracks reach, from their files now (`rows_of`). An album goes whole, as which copy plays can change with any of them.
    /// A track's other files are followed too, whichever album they lie in: a track has one row in `songs`, so a record left
    /// out would keep that row and the write would break `songs.id`, and what is built must be what a whole build gives.
    /// The number of folders built again. Blocking.
    fn rebuild_dirs(
        &self,
        mut dirs: BTreeSet<(i64, String)>,
        rows_of: &mut dyn FnMut(i64, &str) -> Result<Vec<ServedRow>>,
        roots: &HashMap<i64, &str>,
        rules: &Rules,
        covers: &Covers,
    ) -> Result<usize> {
        // The folders' rows, and every folder their albums (as built, and as they would be now) reach.
        let mut rows: Vec<(i64, ServedRow)> = Vec::new();
        let mut read: BTreeSet<(i64, String)> = BTreeSet::new();
        for _ in 0..8 {
            for (folder, dir) in dirs.difference(&read).cloned().collect::<Vec<_>>() {
                rows.extend(rows_of(folder, &dir)?.into_iter().map(|r| (folder, r)));
                read.insert((folder, dir));
            }
            let refs: Vec<(i64, &ServedRow)> = rows.iter().map(|(f, r)| (*f, r)).collect();
            let mut albums: BTreeSet<String> = album_ids(&refs, rules);
            albums.extend(self.built_albums_in(&dirs)?);
            let tracks: BTreeSet<String> = rows.iter().filter_map(|(_, r)| r.track).map(ids::song).collect();
            let reach = self.dirs_of(&albums, &tracks)?;
            if reach.is_subset(&dirs) {
                break;
            }
            dirs.extend(reach);
        }
        if !dirs.is_subset(&read) {
            anyhow::bail!("the albums reach further than they were followed");
        }
        // Here, not before the rows: `update_files` gives their new names ids as it reads them.
        let facts = self.facts();
        let refs: Vec<(i64, &ServedRow)> = rows.iter().map(|(f, r)| (*f, r)).collect();
        let mut db = self.db();
        let tx = db.transaction()?;
        // What was built of them: their albums, and the artists they credit, go. An album goes whole, as the folders followed
        // above are every one it reaches, and is found by its files, as a file need not be a song.
        let mut touched: BTreeSet<String> = BTreeSet::new();
        let mut gone_albums: BTreeSet<String> = BTreeSet::new();
        {
            let mut in_dir = tx.prepare("SELECT DISTINCT album FROM copy_files WHERE folder = ?1 AND dir = ?2")?;
            for (folder, dir) in &dirs {
                for album in in_dir.query_map(params![folder, dir], |r| r.get::<_, String>(0))? {
                    gone_albums.insert(album?);
                }
            }
            let mut songs = tx.prepare("SELECT n, id FROM songs WHERE album_id = ?1")?;
            let mut credits = tx.prepare("SELECT artist_id FROM song_artists WHERE song_id = ?1")?;
            let mut del_fts = tx.prepare("DELETE FROM songs_fts WHERE rowid = ?1")?;
            let mut del_tri = tx.prepare("DELETE FROM songs_tri_fts WHERE rowid = ?1")?;
            let mut del_credits = tx.prepare("DELETE FROM song_artists WHERE song_id = ?1")?;
            let mut del_song = tx.prepare("DELETE FROM songs WHERE n = ?1")?;
            let mut album_n = tx.prepare("SELECT n FROM albums WHERE id = ?1")?;
            let mut album_credits = tx.prepare("SELECT artist_id FROM album_artists WHERE album_id = ?1")?;
            let mut del_album_fts = tx.prepare("DELETE FROM albums_fts WHERE rowid = ?1")?;
            let mut del_album_tri = tx.prepare("DELETE FROM albums_tri_fts WHERE rowid = ?1")?;
            let mut del_album_credits = tx.prepare("DELETE FROM album_artists WHERE album_id = ?1")?;
            let mut del_album = tx.prepare("DELETE FROM albums WHERE id = ?1")?;
            let mut del_copies = tx.prepare("DELETE FROM copies WHERE album = ?1")?;
            let mut del_files = tx.prepare("DELETE FROM copy_files WHERE album = ?1")?;
            for album in &gone_albums {
                let found: Vec<(i64, String)> = songs.query_map([album], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
                for (n, id) in found {
                    for a in credits.query_map([&id], |r| r.get::<_, String>(0))? {
                        touched.insert(a?);
                    }
                    del_fts.execute([n])?;
                    del_tri.execute([n])?;
                    del_credits.execute([&id])?;
                    del_song.execute([n])?;
                }
                for a in album_credits.query_map([album], |r| r.get::<_, String>(0))? {
                    touched.insert(a?);
                }
                if let Some(n) = album_n.query_row([album], |r| r.get::<_, i64>(0)).optional()? {
                    del_album_fts.execute([n])?;
                    del_album_tri.execute([n])?;
                }
                del_album_credits.execute([album])?;
                del_album.execute([album])?;
                del_copies.execute([album])?;
                del_files.execute([album])?;
            }
        }
        let (written, credited) = write_albums(&tx, &refs, roots, rules, &facts, covers)?;
        touched.extend(credited);
        artists_again(&tx, Some(&touched))?;
        genres_again(&tx)?;
        {
            let mut cover = tx.prepare("UPDATE songs SET cover = (SELECT a.cover FROM albums a WHERE a.id = songs.album_id) WHERE album_id = ?1")?;
            for album in &written {
                cover.execute([album])?;
            }
        }
        tx.commit()?;
        Ok(dirs.len())
    }

    /// Each file as built: (folder, rel) → its folder, album and the print of its row.
    fn built_files(&self) -> Result<HashMap<(i64, String), BuiltFile>> {
        let db = self.db();
        let mut st = db.prepare("SELECT folder, rel, dir, album, src FROM copy_files")?;
        let rows = st.query_map([], |r| Ok(((r.get(0)?, r.get(1)?), BuiltFile { folder: r.get(0)?, dir: r.get(2)?, album: r.get(3)?, src: r.get(4)? })))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Each album as built, one row a record: what its print was made from, and the copy that plays by the rule that chose it.
    fn built_albums(&self) -> Result<Vec<BuiltAlbum>> {
        let kinds = self.kinds();
        let db = self.db();
        let mut copies: HashMap<String, Vec<Copy>> = HashMap::new();
        let mut st = db.prepare("SELECT album, folder, dir, format, min_kbps, bits, rate, short, added FROM copies")?;
        let rows = st.query_map([], |r| {
            let folder: i64 = r.get(1)?;
            let (kind, order) = kind_in(&kinds, folder);
            Ok((r.get::<_, String>(0)?, Copy { folder, dir: r.get(2)?, kind, order, format: r.get(3)?, min_kbps: r.get(4)?, bits: r.get(5)?, rate: r.get(6)?, short: r.get(7)?, added: r.get(8)? }))
        })?;
        for row in rows {
            let (album, copy) = row?;
            copies.entry(album).or_default().push(copy);
        }
        let mut st = db.prepare("SELECT id, mbid, src FROM albums")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, release, src) = row?;
            if let Some(c) = copies.get(&id).and_then(|c| copy_for(c, None, &[])) {
                out.push(BuiltAlbum { folder: c.folder, dir: c.dir.clone(), id, release, src });
            }
        }
        Ok(out)
    }

    /// The albums with files in these folders, as built.
    fn built_albums_in(&self, dirs: &BTreeSet<(i64, String)>) -> Result<BTreeSet<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT DISTINCT album FROM copy_files WHERE folder = ?1 AND dir = ?2")?;
        let mut out = BTreeSet::new();
        for (folder, dir) in dirs {
            for a in st.query_map(params![folder, dir], |r| r.get::<_, String>(0))? {
                out.insert(a?);
            }
        }
        Ok(out)
    }

    /// The folders the files of these albums are in, as built, with those of every file of these `tracks` and of the
    /// albums' tracks: a file gone from one album still has its track in another.
    fn dirs_of(&self, albums: &BTreeSet<String>, tracks: &BTreeSet<String>) -> Result<BTreeSet<(i64, String)>> {
        let db = self.db();
        let mut by_album = db.prepare_cached(
            "SELECT DISTINCT folder, dir FROM copy_files WHERE album = ?1 OR song IN (SELECT song FROM copy_files WHERE album = ?1)",
        )?;
        let mut by_track = db.prepare_cached("SELECT DISTINCT folder, dir FROM copy_files WHERE song = ?1")?;
        let mut out = BTreeSet::new();
        for a in albums {
            for d in by_album.query_map([a], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
                out.insert(d?);
            }
        }
        for t in tracks {
            for d in by_track.query_map([t], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
                out.insert(d?);
            }
        }
        Ok(out)
    }

    /// What the library was built from besides its files (the catalogue's generation, its folders, the rules): when it
    /// changes, the whole library is built again.
    fn built_context(&self) -> Option<String> {
        self.db().query_row("SELECT value FROM meta WHERE key = 'built_context'", [], |r| r.get(0)).optional().ok().flatten()
    }

    fn set_built_context(&self, context: &str) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('built_context', ?1)", [context])?;
        Ok(())
    }

    /// Builds again just the albums of files Cratarr wrote itself (0.9.8: a fix, a cover, a Renumber, a tempo, a track
    /// added): read from the catalogue for their folders alone, so they show at once. Nothing where the library is not
    /// built from the catalogue as it is now in all else (its next check builds it). The number of folders built
    /// again. Blocking.
    pub fn update_files(&self, cat: &crate::catalogue::Catalogue, rules: &Rules, paths: &[std::path::PathBuf]) -> Result<usize> {
        let _one = self.building.lock().unwrap_or_else(|p| p.into_inner());
        let roots_now: Vec<(i64, String)> = cat.folders().iter().map(|(i, p)| (i, p.display().to_string())).collect();
        if self.built_context().as_deref() != Some(context(&cat.generation(), &roots_now, &self.kinds(), rules)?.as_str()) {
            return Ok(0);
        }
        let mut dirs = BTreeSet::new();
        for path in paths {
            if let Some((folder, root)) = roots_now.iter().find(|(_, root)| path.starts_with(root))
                && let Ok(rel) = path.strip_prefix(root)
            {
                let rel = rel.components().filter_map(|c| c.as_os_str().to_str()).collect::<Vec<_>>().join("/");
                dirs.insert((*folder, crate::catalogue::dir_of(&rel).to_string()));
            }
        }
        if dirs.is_empty() {
            return Ok(0);
        }
        // A track added here takes its album's record, so it shows as one with the rest. By vote alone: these are a few
        // folders, and a whole record's birth is `build_from`'s, which sees every album.
        let here = dirs.iter().map(|(folder, dir)| Ok(FolderRows { folder: *folder, root: String::new(), rows: cat.served_rows_in(*folder, dir)? })).collect::<Result<Vec<_>>>()?;
        if here.iter().flat_map(|f| &f.rows).any(|r| r.track.is_some() && r.record.is_none())
            && let Err(e) = cat.join_records(&album_copies(&here, rules))
        {
            tracing::warn!(target: "library", reason = %format!("{e:#}"), "records could not be joined");
        }
        let covers = cat.cover_prints()?;
        let roots: HashMap<i64, &str> = roots_now.iter().map(|(f, r)| (*f, r.as_str())).collect();
        // No `build_from` came first: a retag can bring a name no build has met, and after a restart no id is kept yet.
        let mut rows_of = |folder: i64, dir: &str| -> Result<Vec<ServedRow>> {
            let rows = cat.served_rows_in(folder, dir)?;
            self.credit(cat, rows.iter().map(|r| (folder, r)), rules)?;
            Ok(rows)
        };
        self.rebuild_dirs(dirs, &mut rows_of, &roots, rules, &covers)
    }

    /// Where each catalogue folder was at the last build: (folder, path).
    /// The songs of one folder by their file: their id, album, length and when they came (the crates' cards). When a
    /// song came is its album's when that is earlier: an album's date can be imported from a media server.
    pub fn songs_by_rel(&self, folder: i64) -> Result<HashMap<String, RelSong>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT c.rel, c.song, c.album, c.duration_ms, MIN(COALESCE(s.created, a.created), COALESCE(a.created, s.created)), s.genres
             FROM copy_files c JOIN songs s ON s.id = c.song LEFT JOIN albums a ON a.id = c.album WHERE c.folder = ?1",
        )?;
        let rows = st.query_map(rusqlite::params![folder], |r| {
            let genres: Option<String> = r.get(5)?;
            Ok((
                r.get::<_, String>(0)?,
                RelSong {
                    song: r.get(1)?,
                    album: r.get(2)?,
                    duration_ms: r.get(3)?,
                    created: r.get(4)?,
                    genres: genres.map(|g| g.split('\u{1f}').filter(|x| !x.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn roots(&self) -> Vec<(i64, String)> {
        let v: Option<String> = self.db().query_row("SELECT value FROM meta WHERE key = 'roots'", [], |r| r.get(0)).optional().ok().flatten();
        v.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
    }

    /// Counts, for the log and tests: (songs, albums, artists).
    /// Every copy's album, catalogue folder and when the album came (`created`), and every file's song, album, catalogue
    /// folder and place in it: Insights' overview and its period.
    #[allow(clippy::type_complexity)]
    pub fn album_arrivals(&self) -> Result<(Vec<(String, i64, Option<i64>)>, Vec<(String, String, i64, String)>)> {
        let db = self.db();
        let mut st = db.prepare("SELECT c.album, c.folder, a.created FROM copies c JOIN albums a ON a.id = c.album")?;
        let albums = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut st = db.prepare("SELECT song, album, folder, rel FROM copy_files")?;
        let songs = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok((albums, songs))
    }

    /// Every artist with an album, the most albums first, with their MusicBrainz id where known: the artist store's
    /// builder works through them in this order.
    pub fn artist_names(&self) -> Result<Vec<(String, Option<String>)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT name, mbid FROM artists WHERE album_count > 0 ORDER BY album_count DESC, order_name")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Each of these songs' catalogue folder and place in it (its file), those the library has.
    pub fn song_places(&self, ids: &[String]) -> Result<std::collections::HashMap<String, (i64, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT folder, rel FROM songs WHERE id = ?1")?;
        let mut out = std::collections::HashMap::new();
        for id in ids {
            if let Some(p) = st.query_row([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).optional()? {
                out.insert(id.clone(), p);
            }
        }
        Ok(out)
    }

    /// An artist's name by their id (a Subsonic app's star or rating, logged by name: decision 0087).
    pub fn artist_name(&self, id: &str) -> Result<Option<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT name FROM artists WHERE id = ?1 LIMIT 1")?;
        Ok(st.query_row([id], |r| r.get(0)).optional()?)
    }

    /// The names of library ids (0.13, decision 0088: what a crate's conditions name), by id: an artist's (`ar-`) name,
    /// an album's (`al-`) or a track's (`tr-`) title. Ids it does not have, or of no such kind, are left out.
    pub fn names_of(&self, wanted: &[String]) -> Result<HashMap<String, String>> {
        let db = self.db();
        let mut artist = db.prepare_cached("SELECT name FROM artists WHERE id = ?1 LIMIT 1")?;
        let mut album = db.prepare_cached("SELECT name FROM albums WHERE id = ?1 LIMIT 1")?;
        let mut song = db.prepare_cached("SELECT title FROM songs WHERE id = ?1 LIMIT 1")?;
        let mut out = HashMap::new();
        for id in wanted {
            let st = if id.starts_with(ids::ARTIST) {
                &mut artist
            } else if id.starts_with(ids::ALBUM) {
                &mut album
            } else if id.starts_with(ids::SONG) {
                &mut song
            } else {
                continue;
            };
            if let Some(name) = st.query_row([id], |r| r.get::<_, String>(0)).optional()? {
                out.insert(id.clone(), name);
            }
        }
        Ok(out)
    }

    /// An artist by name, as the library writes it (any case): their id and MusicBrainz id.
    pub fn artist_by_name(&self, name: &str) -> Result<Option<(String, Option<String>)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT id, mbid FROM artists WHERE name = ?1 COLLATE NOCASE ORDER BY album_count DESC LIMIT 1")?;
        Ok(st.query_row([name], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    /// Every album artist's albums: (artist id, name, MusicBrainz id, album id, its copies' catalogue folders, its
    /// tracks). Home's Artists to explore.
    #[allow(clippy::type_complexity)]
    pub fn artist_albums(&self) -> Result<Vec<(String, String, Option<String>, String, Vec<i64>, i64)>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT ar.id, ar.name, ar.mbid, al.id, (SELECT group_concat(c.folder) FROM copies c WHERE c.album = al.id), al.song_count FROM album_artists aa
             JOIN albums al ON al.id = aa.album_id JOIN artists ar ON ar.id = aa.artist_id",
        )?;
        let rows = st
            .query_map([], |r| {
                let folders = r.get::<_, Option<String>>(4)?.unwrap_or_default().split(',').filter_map(|f| f.parse().ok()).collect();
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, folders, r.get(5)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Every album lacking something the library requires (the owner, 2026-10-24): its cover, year, genre, album
    /// artist, a track number, its MusicBrainz id.
    pub fn lacking(&self) -> Result<Vec<Lacks>> {
        self.lacks_where("", None)
    }

    /// One album's lacks (0.9.8: a fix applied looks at its own album, not the whole library); none where it lacks
    /// nothing, or is not there.
    pub fn lacks_of(&self, album: &str) -> Result<Option<Lacks>> {
        Ok(self.lacks_where(" WHERE al.id = ?1", Some(album))?.pop())
    }

    fn lacks_where(&self, filter: &str, album: Option<&str>) -> Result<Vec<Lacks>> {
        let db = self.db();
        let mut st = db.prepare(&format!(
            "SELECT al.id, al.name, al.artist,
                    al.cover IS NULL,
                    al.year IS NULL OR al.year = 0,
                    al.genres = '',
                    EXISTS (SELECT 1 FROM songs s WHERE s.album_id = al.id AND (s.album_artist IS NULL OR s.album_artist = '')),
                    EXISTS (SELECT 1 FROM songs s WHERE s.album_id = al.id AND (s.track_no IS NULL OR s.track_no = 0)),
                    al.mbid IS NULL OR al.mbid = ''
             FROM albums al{filter}"
        ))?;
        let params: Vec<&str> = album.into_iter().collect();
        let rows = st
            .query_map(rusqlite::params_from_iter(params), |r| {
                Ok(Lacks {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    artist: r.get(2)?,
                    cover: r.get(3)?,
                    year: r.get(4)?,
                    genre: r.get(5)?,
                    album_artist: r.get(6)?,
                    track_no: r.get(7)?,
                    musicbrainz: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter(Lacks::any).collect())
    }

    /// Every artist a track credits (on compilations and as guests too): Discover's "New to you" is for none of them.
    pub fn track_artist_names(&self) -> Result<Vec<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT DISTINCT name FROM song_artists")?;
        let names = st.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(names)
    }

    /// The albums without a cover: Home leaves them out (the owner, 2026-10-24).
    pub fn coverless(&self) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id FROM albums WHERE cover IS NULL")?;
        let ids = st.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    /// Each of these songs' catalogue folders, one for each of its files (those the library has).
    pub fn song_folders(&self, ids: &[String]) -> Result<std::collections::HashMap<String, Vec<i64>>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT folder FROM copy_files WHERE song = ?1")?;
        let mut out = std::collections::HashMap::new();
        for id in ids {
            let folders = st.query_map([id], |r| r.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            if !folders.is_empty() {
                out.insert(id.clone(), folders);
            }
        }
        Ok(out)
    }

    pub fn counts(&self) -> Result<(u64, u64, u64)> {
        let db = self.db();
        let n = |t: &str| -> Result<u64> { Ok(db.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get::<_, i64>(0))? as u64) };
        Ok((n("songs")?, n("albums")?, n("artists")?))
    }
}

/// Each folder's cover files, as a print (`catalogue::cover_prints`): by (library folder, folder in it).
pub type Covers = HashMap<(i64, String), String>;

/// The kept facts albums are built with besides their files (`Library::set_added` and the rest).
struct Facts {
    added: HashMap<String, i64>,
    originals: HashMap<String, u32>,
    extra_genres: HashMap<String, Vec<String>>,
    release_types: HashMap<String, Kept>,
    incomplete: HashMap<String, Whole>,
    overrides: HashMap<String, Overrides>,
    artists: HashMap<String, i64>,
    kinds: Vec<(i64, Kind)>,
}

impl Facts {
    /// An artist's id as served. `credit` gave every name one, so one without is a fault there: a build that went on
    /// would serve someone under no id.
    fn artist(&self, name: &str) -> Result<String> {
        self.artists.get(&ids::artist_key(name)).map(|id| ids::artist(*id)).with_context(|| format!("{name} has no id"))
    }
}

/// A file as built: its folder there, its album and its row's print.
struct BuiltFile {
    folder: i64,
    dir: String,
    album: String,
    src: String,
}

/// A record as built: what its print was made from (`album_src`), and that print.
struct BuiltAlbum {
    id: String,
    release: Option<String>,
    /// The copy that plays.
    folder: i64,
    dir: String,
    src: String,
}

/// What a folder's library is, and its place in the libraries' order. With no word from the app (a test without one;
/// `main` sets them before anything is served): the first folder is the Music folder, the others wait in Incoming.
fn kind_in(kinds: &[(i64, Kind)], folder: i64) -> (Kind, usize) {
    match kinds.iter().position(|(f, _)| *f == folder) {
        Some(i) => (kinds[i].1, i),
        None if folder == 0 && kinds.is_empty() => (Kind::Main, 0),
        None => (Kind::Incoming, kinds.len() + folder.max(0) as usize),
    }
}

/// A record's Discogs types.
fn discogs_types<'a>(facts: &'a Facts, album: &str) -> Option<&'a Kept> {
    facts.release_types.get(&format!("dc:{album}"))
}

/// The print of the row a song is built from: everything in it (its tags, its file, its analysis); another print, a
/// song to build again.
fn row_src(r: &ServedRow) -> String {
    use md5::Digest;
    format!("{:x}", md5::Md5::digest(format!("{r:?}").as_bytes()))
}

/// The print of what an album is built from besides its songs' rows: its kept facts (added date, original year,
/// Discogs' genres, release types, missing tracks, what is set by hand), each by the record, and the cover files of the
/// folder of the copy that plays.
fn album_src(album: &str, release: Option<&str>, folder: i64, dir: &str, facts: &Facts, covers: &Covers) -> String {
    use md5::Digest;
    let said = format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        facts.added.get(album),
        release.and_then(|m| facts.originals.get(m)),
        facts.extra_genres.get(album),
        release.and_then(|m| facts.release_types.get(&format!("mb:{m}"))),
        discogs_types(facts, album),
        covers.get(&(folder, dir.to_string())),
        facts.incomplete.get(album),
        facts.overrides.get(album),
    );
    format!("{:x}", md5::Md5::digest(said.as_bytes()))
}

/// What the library is built from besides its files: the catalogue's generation, its folders and their kinds, the rules.
fn context(generation: &str, roots: &[(i64, String)], kinds: &[(i64, Kind)], rules: &Rules) -> Result<String> {
    use md5::Digest;
    Ok(format!("{LAYOUT} {generation} {} {} {:x}", serde_json::to_string(roots)?, serde_json::to_string(kinds)?, md5::Md5::digest(serde_json::to_string(rules)?.as_bytes())))
}

/// The files these rows make, each folder one album (`one_album_per_folder`): the library's own grouping, records or not.
fn grouped<'a>(rows: &[(i64, &'a ServedRow)], rules: &Rules) -> Vec<Song<'a>> {
    let mut songs: Vec<Song> = rows.iter().filter(|(_, r)| r.track.is_some()).map(|(folder, r)| song(r, *folder, rules)).collect();
    one_album_per_folder(&mut songs);
    songs
}

/// The record a group's rows say it is: the catalogue's own vote, so a list serves an album as what `assign_records` gave it.
/// Where that has no answer the group has yet to be settled, or holds nothing but other albums' tracks: the record most of
/// them hold, the lowest on a tie. A row with no record has no say here.
fn group_vote<'a>(rows: impl Iterator<Item = &'a ServedRow>) -> Option<i64> {
    let said: Vec<Say> = rows.map(|r| Say { record: r.record, free: !r.shared, back: false }).collect();
    vote_of(&said, &|_| 0).or_else(|| {
        let mut held: BTreeMap<i64, usize> = BTreeMap::new();
        said.iter().filter_map(|s| s.record).for_each(|record| *held.entry(record).or_default() += 1);
        held.into_iter().max_by_key(|(record, n)| (*n, std::cmp::Reverse(*record))).map(|(record, _)| record)
    })
}

/// The songs these rows serve, each with its record (`group_vote`). A row with no record yet is not served, and it still had
/// its say in the grouping, as `album_copies` must find the same albums.
fn songs_of<'a>(rows: &[(i64, &'a ServedRow)], rules: &Rules) -> Vec<Song<'a>> {
    let mut songs = grouped(rows, rules);
    let mut groups: HashMap<&str, Vec<&ServedRow>> = HashMap::new();
    for s in &songs {
        groups.entry(s.group.as_str()).or_default().push(s.row);
    }
    let records: HashMap<String, i64> = groups.into_iter().filter_map(|(group, rows)| Some((group.to_string(), group_vote(rows.into_iter())?))).collect();
    songs.retain_mut(|s| {
        let held = s.row.record.is_some();
        if let Some(record) = records.get(&s.group).filter(|_| held) {
            s.record = *record;
            s.album_id = ids::record(*record);
        }
        held
    });
    songs
}

/// A copy of a record as a write gathers it: the files of one folder that share a `dir`, as `copy_for` reads them.
struct Gathered<'s, 'r> {
    copy: Copy,
    /// By path.
    songs: Vec<&'s Song<'r>>,
    size: i64,
}

/// Each record's copies among these songs: a group's files are one copy, at the folder they share. So an album over CD1
/// and CD2 is one, and two groups of one record at one place are one too, as a copy's key says.
fn gather<'s, 'r>(songs: &'s [Song<'r>], kinds: &[(i64, Kind)]) -> BTreeMap<String, Vec<Gathered<'s, 'r>>> {
    let mut groups: BTreeMap<(&str, &str), Vec<&'s Song<'r>>> = BTreeMap::new();
    for s in songs {
        groups.entry((s.album_id.as_str(), s.group.as_str())).or_default().push(s);
    }
    let mut records: BTreeMap<String, BTreeMap<(i64, String), Vec<&'s Song<'r>>>> = BTreeMap::new();
    for ((album, _), list) in groups {
        let dirs: Vec<&str> = list.iter().map(|s| s.row.rel.rsplit_once('/').map_or("", |(d, _)| d)).collect();
        records.entry(album.to_string()).or_default().entry((list[0].folder, common_dir(&dirs))).or_default().extend(list);
    }
    records
        .into_iter()
        .map(|(album, copies)| {
            let mut gathered: Vec<Gathered> = copies.into_iter().map(|((folder, dir), mut songs)| {
                songs.sort_by(|a, b| a.row.rel.cmp(&b.row.rel));
                // One format for the whole copy, else none (mixed).
                let formats: BTreeSet<&str> = songs.iter().filter_map(|s| s.row.format.as_deref()).collect();
                let format = (formats.len() == 1).then(|| formats.iter().next().map(|f| f.to_string())).flatten();
                let (kind, order) = kind_in(kinds, folder);
                // A copy is as good as its worst file. `None` sorts first, so one file the reader has not reached leaves the
                // copy unread, and an unread copy ranks last until it gets there.
                let copy = Copy {
                    folder,
                    dir,
                    kind,
                    order,
                    format,
                    min_kbps: songs.iter().filter_map(|s| s.row.kbps).min().map(i64::from),
                    bits: songs.iter().map(|s| s.row.bits).min().flatten().map(i64::from),
                    rate: songs.iter().map(|s| s.row.rate).min().flatten().map(i64::from),
                    short: false,
                    added: songs.iter().filter_map(|s| s.row.came).min(),
                };
                let size = songs.iter().map(|s| s.row.size).sum::<u64>() as i64;
                Gathered { copy, songs, size }
            }).collect();
            // What a record lists is every copy's.
            let tracks = |c: &Gathered| c.songs.iter().filter_map(|s| s.row.track).collect::<BTreeSet<i64>>();
            let all = gathered.iter().flat_map(tracks).collect::<BTreeSet<i64>>().len();
            gathered.iter_mut().for_each(|c| c.copy.short = tracks(c).len() < all);
            (album, gathered)
        })
        .collect()
}

/// The indexes of these copies in the order they play for most people: the row copy first, then what would play were it
/// gone. Copies given, one always does (`copy_for`, with nothing off).
fn row_order(copies: &[Gathered]) -> Vec<usize> {
    let plain: Vec<Copy> = copies.iter().map(|c| c.copy.clone()).collect();
    in_order(&plain, None).iter().filter_map(|c| plain.iter().position(|p| p == c)).collect()
}

/// Which of a track's files a build writes into `songs`: the lowest wins. The one in the record the track holds, whichever
/// album a file of it was moved into; then the first of its record's copies in `row_order` (`place`) that holds it, so the row
/// copy's where it has one, and a copy lacking the file leaves the record's list as it was; then by place, so a whole build
/// and a build of some folders write the same one.
fn written_first<'a>(s: &'a Song, place: usize) -> (bool, i64, usize, i64, &'a str) {
    (s.row.record != Some(s.record), s.record, place, s.folder, s.row.rel.as_str())
}

/// Every name these rows credit, as a song reads them. One song at a time: at a real library's size a list of them is not
/// worth holding for this. A song's album artists are never settled to a name no song of that folder credits, so this holds
/// what `one_album_per_folder` leaves too.
fn credited_artists<'r>(rows: impl Iterator<Item = (i64, &'r ServedRow)>, rules: &Rules) -> BTreeSet<String> {
    rows.filter(|(_, r)| r.track.is_some())
        .flat_map(|(folder, r)| {
            let s = song(r, folder, rules);
            s.artists.into_iter().chain(s.album_artists)
        })
        .collect()
}

/// Stands in for the catalogue's artists in tests with no catalogue, as `with_records` does for records: a stable number from
/// the name.
#[cfg(test)]
fn artist_number_in_tests(name: &str) -> i64 {
    use md5::Digest;
    let hash = md5::Md5::digest(ids::artist_key(name).as_bytes());
    (u64::from_le_bytes(hash[..8].try_into().unwrap_or_default()) >> 2) as i64
}

/// Stands in for `assign_records` in tests with no catalogue: each group a record of its own, from a stable hash of it,
/// unless the test named one. So hand-made rows mean what they meant, and production has no fallback.
#[cfg(test)]
fn with_records(folders: &[FolderRows], rules: &Rules) -> Vec<FolderRows> {
    use md5::Digest;
    let rows: Vec<(i64, &ServedRow)> = folders.iter().flat_map(|f| f.rows.iter().map(move |r| (f.folder, r))).collect();
    let groups: HashMap<(i64, String), String> = grouped(&rows, rules).into_iter().map(|s| ((s.folder, s.row.rel.clone()), s.group)).collect();
    let record = |group: &String| {
        let hash = md5::Md5::digest(group.as_bytes());
        (u64::from_le_bytes(hash[..8].try_into().unwrap_or_default()) >> 2) as i64
    };
    let of = |f: &FolderRows, r: &ServedRow| ServedRow { record: r.record.or_else(|| groups.get(&(f.folder, r.rel.clone())).map(record)), ..r.clone() };
    folders.iter().map(|f| FolderRows { folder: f.folder, root: f.root.clone(), rows: f.rows.iter().map(|r| of(f, r)).collect() }).collect()
}

/// The albums these folders make, each as the catalogue gives it a record: the library's own grouping, so a record is
/// what a list shows. Pure.
pub fn album_copies(folders: &[FolderRows], rules: &Rules) -> Vec<crate::catalogue::AlbumCopy> {
    use crate::catalogue::identity::{Listed, RecordFacts, fold};
    let rows: Vec<(i64, &ServedRow)> = folders.iter().flat_map(|f| f.rows.iter().map(move |r| (f.folder, r))).collect();
    let songs = grouped(&rows, rules);
    let mut albums: BTreeMap<&str, Vec<&Song>> = BTreeMap::new();
    for s in &songs {
        albums.entry(&s.group).or_default().push(s);
    }
    let place = |s: &Song| (s.row.first("discnumber").and_then(leading_number), s.row.first("tracknumber").and_then(leading_number));
    let mut holding: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    let mut copies: Vec<crate::catalogue::AlbumCopy> = albums
        .into_values()
        .enumerate()
        .map(|(at, mut list)| {
            let record = group_vote(list.iter().map(|s| s.row));
            if let Some(record) = record {
                holding.entry(record).or_default().push(at);
            }
            list.sort_by(|a, b| {
                let ((ad, an), (bd, bn)) = (place(*a), place(*b));
                (ad.unwrap_or(1), an.unwrap_or(u32::MAX), &a.row.rel).cmp(&(bd.unwrap_or(1), bn.unwrap_or(u32::MAX), &b.row.rel))
            });
            let first = list[0];
            // The file that named the album: one of a folder's that disagrees was settled to it, and its own tags say another.
            let named = list.iter().copied().find(|s| ids::album(s.row, &s.album_artist, s.folder) == s.group).unwrap_or(first);
            crate::catalogue::AlbumCopy {
                facts: RecordFacts {
                    release: list.iter().find_map(|s| s.row.first("musicbrainz_albumid")).map(str::to_lowercase),
                    artist: fold(&first.album_artist),
                    title: fold(&first.album),
                    tracks: list.iter().map(|s| Listed::of(&s.title, s.row.duration_ms.unwrap_or(0) as i64)).collect(),
                },
                title: first.album.clone(),
                artist: first.album_artist.clone(),
                year: list.iter().filter_map(|s| s.row.first("date").and_then(year_of)).max(),
                tracks: list
                    .iter()
                    .filter_map(|s| {
                        let (disc, number) = place(*s);
                        Some((s.row.track?, disc, number))
                    })
                    .collect(),
                files: list.iter().filter(|s| s.row.track.is_some()).map(|s| (s.folder, s.row.rel.clone())).collect(),
                // A shared track stays with its record, wherever a file of it lies: that settles nothing.
                split: list.iter().any(|s| s.row.record.is_none() || (s.row.record != record && !s.row.shared)),
                // The build's to give: it knows the libraries' kinds.
                rank: 0,
                folder: first.folder,
                old_id: first.group.clone(),
                old_base: first.album_base.clone(),
                pid: ids::album_pid(named.row, &named.album_artist),
            }
        })
        .collect();
    // Albums holding one record are its copies, and copies hold the same tracks. Ids apart are a track to take, or a copy that
    // no longer is one: one disc of a double album retagged, each unanimous on the record, or a track added to one of two. Of the very
    // same release one short of the longest is as it should be, and left to say otherwise every build would read it all again.
    let held = |copies: &[crate::catalogue::AlbumCopy], i: usize| copies[i].tracks.iter().map(|t| t.0).collect::<BTreeSet<i64>>();
    for albums in holding.values().filter(|a| a.len() > 1) {
        let Some(longest) = albums.iter().copied().max_by_key(|i| copies[*i].tracks.len()) else { continue };
        let all = held(&copies, longest);
        let settled = |i: usize| {
            let mine = held(&copies, i);
            mine == all || (mine.is_subset(&all) && crate::catalogue::identity::same_release(&copies[i].facts, &copies[longest].facts))
        };
        if !albums.iter().all(|i| settled(*i)) {
            albums.iter().for_each(|i| copies[*i].split = true);
        }
    }
    copies
}

/// Every album the first start gives a record to, read from the catalogue: what the libraries list, what no list shows and can
/// come back, and every name either credits. No build has run, and 0.15's `library.sqlite` has gone with the layout. Blocking.
pub fn first_albums(cat: &crate::catalogue::Catalogue, rules: &Rules) -> Result<crate::catalogue::FirstAlbums> {
    let folders = cat.folders().iter().map(|(folder, root)| Ok(FolderRows { folder, root: root.display().to_string(), rows: cat.served_rows(folder)? })).collect::<Result<Vec<_>>>()?;
    let mut names = credited_artists(folders.iter().flat_map(|f| f.rows.iter().map(move |r| (f.folder, r))), rules);
    let served = album_copies(&folders, rules);
    drop(folders);
    let mut unserved = Vec::new();
    for (folder, id, rows) in cat.unserved_rows()? {
        // By the id it was served under, so its hash is the one it had. A parked folder's rows lie under another now.
        let away = [FolderRows { folder: id, root: String::new(), rows }];
        names.extend(credited_artists(away[0].rows.iter().map(|r| (id, r)), rules));
        for mut album in album_copies(&away, rules) {
            album.files.iter_mut().for_each(|place| place.0 = folder);
            unserved.push(album);
        }
    }
    Ok(crate::catalogue::FirstAlbums { served, unserved, names })
}

/// The albums these rows make.
fn album_ids(rows: &[(i64, &ServedRow)], rules: &Rules) -> BTreeSet<String> {
    songs_of(rows, rules).into_iter().map(|s| s.album_id).collect()
}

/// Writes the songs and albums these rows make (their artists counted apart, `artists_again`): what a whole build
/// and a build of some folders share. A record is one row in `albums` and a row in `songs` for each of its tracks, the file
/// of the copy that plays for most people where it has one; every file of every copy has its row in `copy_files`. The albums
/// written, and every artist they credit.
fn write_albums(
    tx: &Connection,
    rows: &[(i64, &ServedRow)],
    roots: &HashMap<i64, &str>,
    rules: &Rules,
    facts: &Facts,
    covers: &Covers,
) -> Result<(Vec<String>, BTreeSet<String>)> {
    let mut songs = songs_of(rows, rules);
    // Genres set by hand (0.13) are its tracks' too: the axes and crates read the tracks'.
    for s in songs.iter_mut() {
        if let Some(g) = facts.overrides.get(&s.album_id).and_then(|o| o.genres.clone()) {
            s.genres = g;
        }
    }
    let records = gather(&songs, &facts.kinds);
    let orders: HashMap<&str, Vec<usize>> = records.iter().map(|(album, copies)| (album.as_str(), row_order(copies))).collect();
    // A track has one row in `songs`, whichever records its files lie in: keyed by track across the whole write, not record
    // by record, or one file moved into another album stops the library building for good. Every copy's tracks, so a copy
    // lacking a file for now takes nothing from the record's list. With each the place of its copy in `row_order`.
    let mut lead: HashMap<i64, (usize, &Song)> = HashMap::new();
    for (album, copies) in &records {
        for (place, copy) in orders[album.as_str()].iter().enumerate() {
            for s in copies[*copy].songs.iter().copied() {
                let held = lead.entry(s.row.track.unwrap_or_default()).or_insert((place, s));
                if written_first(s, place) < written_first(held.1, held.0) {
                    *held = (place, s);
                }
            }
        }
    }
    let mut credited: BTreeSet<String> = BTreeSet::new();
    let mut albums: BTreeMap<String, Vec<&Song>> = BTreeMap::new();
    {
        let mut ins_song = tx.prepare_cached(
            "INSERT INTO songs (id, pid, folder, rel, album_id, title, artist, album, album_artist, track_no, disc_no, year,
               genre, bpm, duration_ms, size, suffix, kbps, created, mbid, order_title, path,
               rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak, genres, dir, src, track_total)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22,
               ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30)",
        )?;
        let mut ins_fts = tx.prepare_cached("INSERT INTO songs_fts (rowid, title, artist, album) VALUES (?1, ?2, ?3, ?4)")?;
        let mut ins_tri = tx.prepare_cached("INSERT INTO songs_tri_fts (rowid, letters) VALUES (?1, cratarr_letters(?2))")?;
        let mut ins_artist = tx.prepare_cached("INSERT INTO song_artists (song_id, artist_id, name, position) VALUES (?1, ?2, ?3, ?4)")?;
        let mut ins_file = tx.prepare_cached(
            "INSERT INTO copy_files (folder, rel, dir, song, album, src, size, suffix, kbps, duration_ms, bpm,
               rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        )?;
        // In the order of the rows, as every file is one of these: `n` is the library's order, and it must not turn on how a
        // record's number happens to sort.
        for s in &songs {
            let r = s.row;
            let id = ids::song(r.track.unwrap_or_default());
            let suffix = r.rel.rsplit_once('.').map(|(_, e)| e.to_lowercase());
            let g = gains(r);
            let (dir, src, bpm, length) = (crate::catalogue::dir_of(&r.rel), row_src(r), bpm_of(r), r.duration_ms.map(|d| d as i64));
            ins_file.execute(params![
                s.folder,
                r.rel,
                dir,
                id,
                s.album_id,
                src,
                r.size as i64,
                suffix,
                r.kbps,
                length,
                bpm,
                g.track_gain,
                g.track_peak,
                g.album_gain,
                g.album_peak,
            ])?;
            if !lead.get(&r.track.unwrap_or_default()).is_some_and(|l| std::ptr::eq(l.1, s)) {
                continue;
            }
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
                bpm,
                length,
                r.size as i64,
                suffix,
                r.kbps,
                r.born,
                r.first("musicbrainz_trackid"),
                order_name(&s.title),
                format!("{}/{}", roots.get(&s.folder).map_or("", |r| r.trim_end_matches('/')), r.rel),
                g.track_gain,
                g.track_peak,
                g.album_gain,
                g.album_peak,
                s.genres.join("\u{1f}"),
                dir,
                src,
                total_of(r),
            ])?;
            let n = tx.last_insert_rowid();
            ins_fts.execute(params![n, s.title, s.artist_credit.as_deref().unwrap_or(""), s.album])?;
            ins_tri.execute(params![n, s.title])?;
            for (i, name) in s.artists.iter().enumerate() {
                let aid = facts.artist(name)?;
                ins_artist.execute(params![id, aid, name, i as i64])?;
                credited.insert(aid);
            }
            albums.entry(s.album_id.clone()).or_default().push(s);
        }
    }
    // The copy that plays for most people first, then by path: rows come in the order of a whole read or of the folders a
    // build reached, and an album's first song names it.
    let place = |s: &Song| lead.get(&s.row.track.unwrap_or_default()).map_or(0, |l| l.0);
    albums.values_mut().for_each(|list| list.sort_by(|a, b| (place(a), &a.row.rel).cmp(&(place(b), &b.row.rel))));
    let mut ins_album = tx.prepare_cached(
        "INSERT INTO albums (id, name, artist, artist_id, year, original_year, genre, genres, song_count, duration_ms,
           size, discs, created, mbid, compilation, order_name, format, min_kbps, cover,
           release_type, type_from, secondary, src, type_why, set_fields, whole_tracks, record)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
           ?21, ?22, ?23, ?24, ?25, ?26, ?27)",
    )?;
    let mut ins_fts = tx.prepare_cached("INSERT INTO albums_fts (rowid, name, artist) VALUES (?1, ?2, ?3)")?;
    let mut ins_tri = tx.prepare_cached("INSERT INTO albums_tri_fts (rowid, letters) VALUES (?1, cratarr_letters(?2))")?;
    let mut ins_aa = tx.prepare_cached("INSERT INTO album_artists (album_id, artist_id, name, position, mbid) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    let mut ins_copy = tx.prepare_cached(
        "INSERT INTO copies (album, folder, dir, format, min_kbps, size, added, tracks, bits, rate, short) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for (album_id, copies) in &records {
        for c in copies {
            let tracks: BTreeSet<i64> = c.songs.iter().filter_map(|s| s.row.track).collect();
            ins_copy.execute(params![
                album_id,
                c.copy.folder,
                c.copy.dir,
                c.copy.format,
                c.copy.min_kbps,
                c.size,
                c.songs.iter().filter_map(|s| s.row.came).min(),
                tracks.len() as i64,
                c.copy.bits,
                c.copy.rate,
                c.copy.short,
            ])?;
        }
    }
    for (album_id, songs) in &albums {
        let first = songs[0];
        let copies = &records[album_id];
        let row = &copies[orders[album_id.as_str()][0]];
        // What the record is: what the copy that plays for most people says, and where it says nothing, any other's, as a record must
        // not lack a year or a release because that copy came without one. Never someone's own: what one person holds changes nothing
        // others see (the owner, 2026-11-07), so a record in a main library goes by that, and one only in people's own music by the
        // first copy added, which alone speaks there.
        let only_own = copies.iter().all(|c| c.copy.kind == Kind::Own);
        let speaks = |s: &Song| if only_own { s.folder == row.copy.folder } else { kind_in(&facts.kinds, s.folder).0 != Kind::Own };
        let said: Vec<&Song> = songs.iter().copied().filter(|s| speaks(s)).collect();
        let spoken: Vec<&Song> = said.iter().copied().chain(copies.iter().flat_map(|c| c.songs.iter().copied()).filter(|s| speaks(s))).collect();
        let every = || spoken.iter().copied();
        let release = every().find_map(|s| s.row.first("musicbrainz_albumid"));
        let mut years: Vec<u32> = said.iter().filter_map(|s| s.row.first("date").and_then(year_of)).collect();
        if years.is_empty() {
            years = every().filter_map(|s| s.row.first("date").and_then(year_of)).collect();
        }
        // Its original year: its files' original date, else MusicBrainz's for its release (0.9.5); shown as its year
        // where it is earlier than its release's (a reissue's), so every list and order has it.
        let original = every().filter_map(|s| s.row.first("originaldate").and_then(year_of)).min().or_else(|| release.and_then(|m| facts.originals.get(m)).copied());
        let mut album_year = match (original, years.iter().max().copied()) {
            (Some(o), Some(d)) => Some(o.min(d)),
            (o, d) => o.or(d),
        };
        // What is set by hand wins (0.13): a year set is its original year and the one shown.
        let set = facts.overrides.get(album_id);
        let original = match set.and_then(|o| o.year) {
            Some(y) => {
                album_year = Some(y);
                Some(y)
            }
            None => original,
        };
        let mut genres = genres_of(said.iter().copied());
        if genres.is_empty() {
            genres = genres_of(every());
        }
        // None in any file: Discogs' genres and styles where they were asked (0.9.5; the files never written).
        if genres.is_empty()
            && let Some(g) = facts.extra_genres.get(album_id)
        {
            genres = g.clone();
        }
        let dir = &row.copy.dir;
        let discs = said.iter().filter_map(|s| s.row.first("discnumber").and_then(leading_number)).max().unwrap_or(1);
        let compilation = said.iter().any(|s| s.row.first("compilation").is_some_and(|c| c == "1"));
        let artist_id = facts.artist(&first.album_artists[0])?;
        // The art's id: the album's, with a fingerprint of its picture (see `art`).
        let root = roots.get(&row.copy.folder).copied().unwrap_or("");
        let first_track = row.songs.first().map(|s| Path::new(root).join(&s.row.rel));
        let cover = super::art::album_print(&Path::new(root).join(dir), first_track.as_deref()).map(|p| format!("{album_id}_{p}"));
        let duration_ms = songs.iter().filter_map(|s| s.row.duration_ms).sum::<u64>();
        let whole = facts.incomplete.get(album_id);
        let shape = Shape {
            tracks: songs.len(),
            duration_ms,
            longest_ms: songs.iter().filter_map(|s| s.row.duration_ms).max().unwrap_or(0),
            name: &first.album,
            various: first.album_artist.eq_ignore_ascii_case("Various Artists"),
        }
        .whole(whole);
        let (mut kind, mut type_from, mut type_why, mut secondary) =
            release_type_of(release.and_then(|m| facts.release_types.get(&format!("mb:{m}"))), discogs_types(facts, album_id), &shape);
        if let Some(k) = set.and_then(|o| o.kind.clone()) {
            (kind, type_from, type_why) = (Some(k), Some("set"), None);
        }
        if let Some(s) = set.and_then(|o| o.secondary.clone()) {
            secondary = s;
        }
        ins_album.execute(params![
            album_id,
            first.album,
            first.album_artist,
            artist_id,
            album_year,
            original,
            genres.first(),
            genres.join("\u{1f}"),
            songs.len() as i64,
            duration_ms as i64,
            row.size,
            discs,
            every().filter_map(|s| s.row.born).chain(facts.added.get(album_id).copied()).min(),
            release,
            compilation,
            order_name(&first.album),
            row.copy.format,
            row.copy.min_kbps,
            cover,
            kind,
            type_from,
            secondary.join("\u{1f}"),
            album_src(album_id, release, row.copy.folder, dir, facts, covers),
            type_why,
            set.map(|o| o.fields().join("\u{1f}")).unwrap_or_default(),
            whole.map(|w| w.tracks).filter(|t| *t > songs.len()).map(|t| t as i64),
            first.record,
        ])?;
        let n = tx.last_insert_rowid();
        ins_fts.execute(params![n, first.album, first.album_artist])?;
        ins_tri.execute(params![n, first.album])?;
        for (i, name) in first.album_artists.iter().enumerate() {
            let aid = facts.artist(name)?;
            let mbid = if i == 0 { first.row.first("musicbrainz_albumartistid") } else { None };
            ins_aa.execute(params![album_id, aid, name, i as i64, mbid])?;
            credited.insert(aid);
        }
    }
    Ok((albums.into_keys().collect(), credited))
}

/// These songs' genres, each once, as the first to name it spells it.
fn genres_of<'s, 'r: 's>(songs: impl Iterator<Item = &'s Song<'r>>) -> Vec<String> {
    let mut genres: Vec<String> = Vec::new();
    for s in songs {
        for g in &s.genres {
            if !genres.iter().any(|x| x.eq_ignore_ascii_case(g)) {
                genres.push(g.clone());
            }
        }
    }
    genres
}

/// The artists counted again from what the songs and albums credit now: all of them, or `only` these (those no longer
/// credited go). Each one's name is the one it is credited by most (the shorter spelling first among equals), its
/// MusicBrainz id the first album's (by id) that names it first, its counts its albums and its songs.
fn artists_again(tx: &Connection, only: Option<&BTreeSet<String>>) -> Result<()> {
    let wanted = |aid: &str| only.is_none_or(|o| o.contains(aid));
    let mut names: HashMap<String, (String, i64)> = HashMap::new();
    {
        let mut st = tx.prepare(
            "SELECT artist_id, name, count(*) FROM (SELECT artist_id, name FROM song_artists UNION ALL SELECT artist_id, name FROM album_artists)
             GROUP BY artist_id, name",
        )?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))? {
            let (aid, name, n) = row?;
            if !wanted(&aid) {
                continue;
            }
            let e = names.entry(aid).or_insert_with(|| (name.clone(), n));
            if n > e.1 || (n == e.1 && name < e.0) {
                *e = (name, n);
            }
        }
    }
    let counts = |sql: &str| -> Result<HashMap<String, i64>> {
        let mut st = tx.prepare(sql)?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    };
    let songs = counts("SELECT artist_id, count(*) FROM song_artists GROUP BY artist_id")?;
    let albums = counts("SELECT artist_id, count(DISTINCT album_id) FROM album_artists GROUP BY artist_id")?;
    let mut mbids: HashMap<String, String> = HashMap::new();
    {
        let mut st = tx.prepare("SELECT artist_id, mbid FROM album_artists WHERE position = 0 AND mbid IS NOT NULL ORDER BY album_id")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (aid, mbid) = row?;
            mbids.entry(aid).or_insert(mbid);
        }
    }
    let existing: HashMap<String, i64> = {
        let mut st = tx.prepare("SELECT id, n FROM artists")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        rows.collect::<rusqlite::Result<HashMap<_, _>>>()?.into_iter().filter(|(a, _)| wanted(a)).collect()
    };
    let mut ids: BTreeSet<&String> = names.keys().collect();
    ids.extend(existing.keys());
    let mut ins = tx.prepare_cached("INSERT INTO artists (id, name, mbid, album_count, song_count, order_name) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
    let mut upd = tx.prepare_cached("UPDATE artists SET name = ?2, mbid = ?3, album_count = ?4, song_count = ?5, order_name = ?6 WHERE id = ?1")?;
    let mut del = tx.prepare_cached("DELETE FROM artists WHERE id = ?1")?;
    let mut ins_fts = tx.prepare_cached("INSERT INTO artists_fts (rowid, name) VALUES (?1, ?2)")?;
    let mut del_fts = tx.prepare_cached("DELETE FROM artists_fts WHERE rowid = ?1")?;
    let mut ins_tri = tx.prepare_cached("INSERT INTO artists_tri_fts (rowid, letters) VALUES (?1, cratarr_letters(?2))")?;
    let mut del_tri = tx.prepare_cached("DELETE FROM artists_tri_fts WHERE rowid = ?1")?;
    for aid in ids {
        let had = existing.get(aid).copied();
        let Some((name, _)) = names.get(aid) else {
            if let Some(n) = had {
                del_fts.execute([n])?;
                del_tri.execute([n])?;
                del.execute([aid])?;
            }
            continue;
        };
        let values = params![aid, name, mbids.get(aid), albums.get(aid).copied().unwrap_or(0), songs.get(aid).copied().unwrap_or(0), order_name(name)];
        let n = match had {
            Some(n) => {
                upd.execute(values)?;
                del_fts.execute([n])?;
                del_tri.execute([n])?;
                n
            }
            None => {
                ins.execute(values)?;
                tx.last_insert_rowid()
            }
        };
        ins_fts.execute(params![n, name])?;
        ins_tri.execute(params![n, name])?;
    }
    Ok(())
}

/// The genres counted again: each genre's songs and albums, named as the first song (by album, then as built) has it.
fn genres_again(tx: &Connection) -> Result<()> {
    let mut genre_songs: BTreeMap<String, (String, u32, BTreeSet<String>)> = BTreeMap::new();
    {
        let mut st = tx.prepare("SELECT album_id, genres FROM songs ORDER BY album_id, n")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (album, genres) = row?;
            for g in genres.split('\u{1f}').filter(|g| !g.is_empty()) {
                let e = genre_songs.entry(g.to_lowercase()).or_insert_with(|| (g.to_string(), 0, BTreeSet::new()));
                e.1 += 1;
                e.2.insert(album.clone());
            }
        }
    }
    tx.execute("DELETE FROM genres", [])?;
    let mut ins = tx.prepare_cached("INSERT OR IGNORE INTO genres (name, song_count, album_count) VALUES (?1, ?2, ?3)")?;
    for (_, (name, songs, albums)) in genre_songs {
        ins.execute(params![name, songs, albums.len() as i64])?;
    }
    Ok(())
}

/// One album per folder (Cratarr's music layout, decision 0016; gonic's model): songs of one folder whose tags disagree
/// (a date or an album artist on some tracks only) would otherwise split it into several albums. The folder's album is
/// the one most of its songs name, the first in path order on a tie, so its id still comes from the tags and survives
/// a move.
fn one_album_per_folder(songs: &mut [Song]) {
    let dir = |s: &Song| (s.folder, s.row.rel.rsplit_once('/').map_or(String::new(), |(d, _)| d.to_string()));
    let mut votes: HashMap<(i64, String), Vec<(usize, &str)>> = HashMap::new();
    let mut order: Vec<usize> = (0..songs.len()).collect();
    order.sort_by(|a, b| songs[*a].row.rel.cmp(&songs[*b].row.rel));
    for &i in &order {
        let s = &songs[i];
        let list = votes.entry(dir(s)).or_default();
        match list.iter_mut().find(|(_, id)| *id == s.group) {
            Some(v) => v.0 += 1,
            None => list.push((1, s.group.as_str())),
        }
    }
    let chosen: HashMap<(i64, String), (String, usize)> = votes
        .into_iter()
        .map(|(d, list)| {
            let best = list.iter().max_by(|a, b| a.0.cmp(&b.0).then(std::cmp::Ordering::Greater)).map_or("", |v| v.1).to_string();
            (d, (best, list.len()))
        })
        .collect();
    // The album's own name and artist come from a song that named it.
    let mut lead: HashMap<String, (String, String, Vec<String>, String)> = HashMap::new();
    for s in songs.iter() {
        if let Some((id, _)) = chosen.get(&dir(s))
            && *id == s.group
        {
            lead.entry(id.clone()).or_insert_with(|| (s.album.clone(), s.album_artist.clone(), s.album_artists.clone(), s.album_base.clone()));
        }
    }
    for s in songs.iter_mut() {
        let Some((id, kinds)) = chosen.get(&dir(s)) else { continue };
        if *kinds > 1
            && let Some((album, artist, artists, base)) = lead.get(id)
        {
            s.group = id.clone();
            s.album_base = base.clone();
            s.album = album.clone();
            s.album_artist = artist.clone();
            s.album_artists = artists.clone();
        }
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

    // ---- Built again where it changed (0.9.8): always what a whole build gives.

    /// Every table as rows of text, without the row numbers (a build of some folders numbers its rows anew), sorted; and
    /// what each search index finds for a few words, as ids.
    fn dump(lib: &Library) -> Vec<String> {
        let db = lib.db();
        let mut out = Vec::new();
        for (table, skip_n) in
            [("songs", true), ("song_artists", false), ("albums", true), ("album_artists", false), ("artists", true), ("genres", false), ("copies", false), ("copy_files", false)]
        {
            let mut st = db.prepare(&format!("SELECT * FROM {table}")).unwrap();
            let cols = st.column_count();
            let mut rows: Vec<String> = st
                .query_map([], |r| {
                    let mut line = format!("{table}:");
                    for i in usize::from(skip_n)..cols {
                        let v: rusqlite::types::Value = r.get(i)?;
                        line.push_str(&format!(" {v:?}"));
                    }
                    Ok(line)
                })
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            rows.sort();
            out.extend(rows);
        }
        for word in ["björk", "orb", "hunter", "clouds", "homogenic", "new", "move"] {
            for (fts, table) in [("songs_fts", "songs"), ("albums_fts", "albums"), ("artists_fts", "artists")] {
                let mut st = db.prepare(&format!("SELECT t.id FROM {fts} f JOIN {table} t ON t.n = f.rowid WHERE {fts} MATCH ?1")).unwrap();
                let mut ids: Vec<String> = st.query_map([format!("\"{word}\"*")], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
                ids.sort();
                out.push(format!("{fts} {word}: {ids:?}"));
            }
        }
        // The trigram tables (0.12) as well: what each finds by letters, and how many rows in all it finds (a row left
        // behind would count where the join hides it).
        for letters in ["bjork", "orb", "hunter", "fluffyclouds", "homogenic", "orbital", "joga", "triphop"] {
            for (tri, table, _) in TRIGRAMS {
                let q = format!("\"{letters}\"");
                let mut st = db.prepare(&format!("SELECT t.id FROM {tri} f JOIN {table} t ON t.n = f.rowid WHERE {tri} MATCH ?1")).unwrap();
                let mut ids: Vec<String> = st.query_map([&q], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
                ids.sort();
                let all: i64 = db.query_row(&format!("SELECT count(*) FROM {tri} WHERE {tri} MATCH ?1"), [&q], |r| r.get(0)).unwrap();
                out.push(format!("{tri} {letters}: {ids:?} of {all}"));
            }
        }
        out
    }

    /// `before` built whole, then brought to `after` where it changed; against `after` built whole. The folders built
    /// again.
    fn same_as_whole(before: &[FolderRows], after: &[FolderRows], facts: impl Fn(&Library)) -> usize {
        let rules = Rules::default();
        let step = Library::open(None).unwrap();
        step.build(before, &rules).unwrap();
        facts(&step);
        step.credit_in_tests(after, &rules);
        let n = step.update_rows(&with_records(after, &rules), &rules, &Covers::new()).unwrap();
        let whole = Library::open(None).unwrap();
        facts(&whole);
        whole.build(after, &rules).unwrap();
        let (a, b) = (dump(&step), dump(&whole));
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x, y, "built again where it changed, against built whole");
        }
        assert_eq!(a.len(), b.len());
        n
    }

    fn with(mut f: Vec<FolderRows>, change: impl FnOnce(&mut Vec<ServedRow>)) -> Vec<FolderRows> {
        change(&mut f[0].rows);
        f
    }

    fn tag(r: &mut ServedRow, name: &str, value: &str) {
        r.tags.retain(|(n, _)| n != name);
        r.tags.push((name.into(), value.into()));
        r.mtime += 1;
    }

    #[test]
    fn nothing_changed_builds_nothing_again() {
        assert_eq!(same_as_whole(&sample(), &sample(), |_| {}), 0);
    }

    #[test]
    fn a_retagged_track_builds_its_album_again_and_nothing_else() {
        let after = with(sample(), |rows| tag(&mut rows[1], "genre", "Electronic; Trip Hop"));
        assert_eq!(same_as_whole(&sample(), &after, |_| {}), 1);
        // A new album artist: the old artist gone.
        let after = with(sample(), |rows| {
            tag(&mut rows[2], "artist", "Orbital");
            tag(&mut rows[2], "albumartist", "Orbital");
        });
        assert_eq!(same_as_whole(&sample(), &after, |_| {}), 1);
    }

    /// As the catalogue has it, a retag changes an album's group and its record stays: `with_records` alone would give it
    /// another by hash, and these two retags would never meet a group new to a known record.
    #[test]
    fn a_retag_keeps_its_record_and_builds_as_a_whole_build_would() {
        let kinds = [(0, Kind::Main)];
        let before = || vec![in_folder(0, copy_rows(1, "A/Album", 5, "FLAC", 900))];
        let genre = with(before(), |rows| tag(&mut rows[1], "genre", "Electronic; Trip Hop"));
        assert_eq!(same_as_whole_in(&kinds, &before(), &genre), 1);
        let renamed = with(before(), |rows| {
            for r in rows.iter_mut() {
                tag(r, "artist", "Orbital");
                tag(r, "albumartist", "Orbital");
            }
        });
        assert_eq!(same_as_whole_in(&kinds, &before(), &renamed), 1);
        let lib = built_with(&kinds, &renamed);
        assert_eq!(lib.db().query_row("SELECT id FROM albums", [], |r| r.get::<_, String>(0)).unwrap(), "al-5", "a new group, the same record");
    }

    #[test]
    fn files_that_come_go_or_move_are_built_as_a_whole_build_would() {
        let new_one = |rows: &mut Vec<ServedRow>| {
            rows.push(row(4, "Björk/Homogenic (1997)/03 Unravel.flac", &[
                ("title", "Unravel"),
                ("artist", "Björk"),
                ("albumartist", "Björk"),
                ("album", "Homogenic"),
                ("tracknumber", "03"),
                ("genre", "Electronic"),
            ]))
        };
        assert_eq!(same_as_whole(&sample(), &with(sample(), new_one), |_| {}), 1);
        assert_eq!(same_as_whole(&sample(), &with(sample(), |rows| drop(rows.remove(0))), |_| {}), 1);
        // The Orb's album moved to another folder: the old one empty, the new one new.
        let moved = with(sample(), |rows| rows[2].rel = "The Orb/Adventures Beyond (1991)/01 Little Fluffy Clouds.mp3".into());
        assert_eq!(same_as_whole(&sample(), &moved, |_| {}), 2);
        // A whole album gone, and its artist with it; a new album by a new artist.
        let swapped = with(sample(), |rows| {
            rows.remove(2);
            rows.push(row(9, "New Order/Movement (1981)/01 Dreams Never End.flac", &[("title", "Dreams Never End"), ("artist", "New Order"), ("album", "Movement")]));
        });
        assert_eq!(same_as_whole(&sample(), &swapped, |_| {}), 2);
    }

    #[test]
    fn an_album_over_two_folders_is_built_again_whole() {
        let two_discs = |disc2: &str| {
            with(sample(), |rows| {
                for (t, d, title) in [(5, "CD1", "Hunter"), (6, "CD2", disc2)] {
                    rows.push(row(t, &format!("Björk/Live Box/{d}/01 {title}.flac"), &[
                        ("title", title),
                        ("artist", "Björk"),
                        ("albumartist", "Björk"),
                        ("album", "Live Box"),
                        ("discnumber", if d == "CD1" { "1" } else { "2" }),
                    ]));
                }
            })
        };
        // One file of the second disc changed: both discs' folders built again (the album's songs are in both).
        assert_eq!(same_as_whole(&two_discs("Jóga"), &two_discs("Jóga (Live)"), |_| {}), 2);
    }

    #[test]
    fn an_albums_kept_facts_build_it_again() {
        let none = with(sample(), |rows| rows[2].tags.retain(|(n, _)| n != "genre"));
        let orb = || {
            let lib = Library::open(None).unwrap();
            lib.build(&none, &Rules::default()).unwrap();
            let db = lib.db();
            db.query_row("SELECT id FROM albums WHERE name LIKE 'Adventures%'", [], |r| r.get::<_, String>(0)).unwrap()
        };
        let key = orb();
        let step = Library::open(None).unwrap();
        step.build(&none, &Rules::default()).unwrap();
        // Discogs' genres for it came, and an earlier added date.
        step.set_extra_genres(HashMap::from([(key.clone(), vec!["Ambient House".to_string()])]));
        step.set_added(HashMap::from([(key.clone(), 1_600_000_000)]));
        assert_eq!(step.update_rows(&with_records(&none, &Rules::default()), &Rules::default(), &Covers::new()).unwrap(), 1, "{key}");
        let whole = Library::open(None).unwrap();
        whole.set_extra_genres(HashMap::from([(key.clone(), vec!["Ambient House".to_string()])]));
        whole.set_added(HashMap::from([(key, 1_600_000_000)]));
        whole.build(&none, &Rules::default()).unwrap();
        assert_eq!(dump(&step), dump(&whole));
    }

    /// C32: a build of some folders and a whole one read every kept fact by the record, so a fact set between them builds the same
    /// album both ways, for each kind of fact: an old hash left behind would show in one and not the other.
    #[test]
    fn same_as_whole_with_every_fact_by_the_record() {
        let none = with(sample(), |rows| rows[2].tags.retain(|(n, _)| n != "genre"));
        let lib = Library::open(None).unwrap();
        lib.build(&none, &Rules::default()).unwrap();
        let key: String = lib.db().query_row("SELECT id FROM albums WHERE name LIKE 'Adventures%'", [], |r| r.get(0)).unwrap();
        let facts = |lib: &Library| {
            lib.set_extra_genres(HashMap::from([(key.clone(), vec!["Ambient House".to_string()])]));
            lib.set_added(HashMap::from([(key.clone(), 1_600_000_000)]));
            lib.set_release_types(HashMap::from([(format!("dc:{key}"), (Some("album".to_string()), vec!["live".to_string()]))]));
            lib.set_incomplete(HashMap::from([(key.clone(), Whole { tracks: 9, duration_ms: None, longest_ms: None })]));
            lib.set_overrides(HashMap::from([(key.clone(), Overrides { year: Some(1991), ..Overrides::default() })]));
        };
        assert_eq!(same_as_whole(&none, &none, facts), 1, "that album alone");
        facts(&lib);
        lib.build(&none, &Rules::default()).unwrap();
        let (year, kind, whole): (i64, String, i64) = lib.db().query_row("SELECT year, secondary, whole_tracks FROM albums WHERE id = ?1", [&key], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!((year, kind.as_str(), whole), (1991, "live", 9), "each of them read");
    }

    #[test]
    fn new_analyses_build_their_albums_again() {
        let analysed = with(sample(), |rows| {
            rows[0].analysis = Some(crate::catalogue::RowAnalysis { track_gain_db: -7.5, track_peak: 0.98, bpm: Some(92.0), album: Some((-7.0, 0.99)) })
        });
        assert_eq!(same_as_whole(&sample(), &analysed, |_| {}), 1);
    }

    /// A catalogue on a folder of its own, for a test named `name`.
    fn catalogue_in(name: &str) -> (std::path::PathBuf, crate::catalogue::Catalogue) {
        let root = std::env::temp_dir().join(format!("cratarr-records-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();
        let cat = crate::catalogue::Catalogue::open(None, vec![root.clone()]).unwrap();
        (root, cat)
    }

    /// A catalogue on two folders of their own, both kept, and a library of them.
    fn two_kept(name: &str) -> ([std::path::PathBuf; 2], crate::catalogue::Catalogue, Library) {
        let roots = [0, 1].map(|n| std::env::temp_dir().join(format!("cratarr-records-{name}-{n}-{}", std::process::id())));
        for root in &roots {
            std::fs::remove_dir_all(root).ok();
            std::fs::create_dir_all(root).unwrap();
        }
        let cat = crate::catalogue::Catalogue::open(None, roots.to_vec()).unwrap();
        let lib = Library::open(None).unwrap();
        lib.set_kinds(TWO_KEPT.to_vec());
        (roots, cat, lib)
    }

    /// These files, (path, album, title), as the catalogue holds them once read and identified.
    fn add_files(cat: &crate::catalogue::Catalogue, root: &Path, files: &[(&str, &str, &str)]) {
        add_files_to(cat, 0, root, files);
    }

    /// `add_files` in one of a catalogue's folders.
    fn add_files_to(cat: &crate::catalogue::Catalogue, folder: i64, root: &Path, files: &[(&str, &str, &str)]) {
        for (rel, _, _) in files {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, rel).unwrap();
        }
        cat.walk_all();
        cat.pretend_read_for_tests();
        for (rel, album, title) in files {
            cat.tag_for_tests(folder, rel, &[("album", album), ("albumartist", "Artist"), ("title", title)]);
        }
        cat.identify().unwrap();
    }

    /// Every album's folders as the catalogue reads them now: what a build gives `assign_records`.
    fn albums_now(cat: &crate::catalogue::Catalogue) -> Vec<crate::catalogue::AlbumCopy> {
        let folders: Vec<FolderRows> = cat.folders().iter().map(|(folder, root)| FolderRows { folder, root: root.display().to_string(), rows: cat.served_rows(folder).unwrap() }).collect();
        album_copies(&folders, &Rules::default())
    }

    /// An album knows the hashes 0.15 served it by, and of a folder that disagrees, the one the folder was settled to: the map is
    /// made of nothing else, so a wrong one reads as an old row that matched nothing.
    #[test]
    fn an_album_knows_the_ids_it_had_whatever_its_files_disagree_on() {
        let (roots, cat, _) = two_kept("old-ids");
        add_files_to(&cat, 1, &roots[1], &[("A/01 One.flac", "Most", "One"), ("A/02 Two.flac", "Most", "Two"), ("A/03 Odd.flac", "Odd", "Three")]);
        let albums = albums_now(&cat);
        let album = &albums[0];
        assert_eq!(albums.len(), 1, "one folder, one album");
        let by = album_pid_for_tests("Most", "Artist");
        assert_eq!((album.folder, album.pid.as_str()), (1, by.as_str()), "the first track by place says Most, and so does the album");
        assert_eq!(album.old_id, ids::album_at(&by, 1));
        assert_eq!(album.old_base, ids::album_at(&by, 0));

        // The first by place is the odd one out: the pid is still the album's.
        cat.tag_for_tests(1, "A/01 One.flac", &[("album", "Odd"), ("albumartist", "Artist"), ("title", "One")]);
        cat.tag_for_tests(1, "A/03 Odd.flac", &[("album", "Most"), ("albumartist", "Artist"), ("title", "Three")]);
        cat.tag_for_tests(1, "A/02 Two.flac", &[("album", "Most"), ("albumartist", "Artist"), ("title", "Two")]);
        let albums = albums_now(&cat);
        assert_eq!((albums[0].pid.as_str(), albums[0].old_id.as_str()), (by.as_str(), ids::album_at(&by, 1).as_str()));
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// What `ids::album` hashes, from an album's title and artist alone.
    fn album_pid_for_tests(album: &str, artist: &str) -> String {
        ids::album_pid(&row(1, "A/01.flac", &[("album", album)]), artist)
    }

    /// CR13, read from a real catalogue: what no list shows comes under the id it was served by, its files named as they lie.
    #[test]
    fn the_first_albums_are_what_the_lists_show_and_what_can_come_back() {
        let (roots, cat, _) = two_kept("first-albums");
        add_files_to(&cat, 0, &roots[0], &[("Here/01 One.flac", "Here", "One")]);
        add_files_to(&cat, 1, &roots[1], &[("Away/01 One.flac", "Away", "One")]);
        cat.mark_trashed(1, "Away", "token").unwrap();
        let found = first_albums(&cat, &Rules::default()).unwrap();
        assert_eq!(found.served.iter().map(|a| a.title.as_str()).collect::<Vec<_>>(), ["Here"]);
        assert_eq!(found.unserved.iter().map(|a| a.title.as_str()).collect::<Vec<_>>(), ["Away"]);
        let away = &found.unserved[0];
        let by = album_pid_for_tests("Away", "Artist");
        assert_eq!((away.folder, away.old_id.as_str(), away.old_base.as_str()), (1, ids::album_at(&by, 1).as_str(), ids::album_at(&by, 0).as_str()));
        assert_eq!(away.files, [(1, ".cratarr-trash/token/01 One.flac".to_string())]);
        assert!(found.names.contains("Artist"));

        // A folder out of use: its rows lie under a negative id, and its albums were known by the one it had.
        let track = cat.track_of(0, "Here/01 One.flac");
        cat.set_folders(vec![roots[1].clone()]).unwrap();
        let found = first_albums(&cat, &Rules::default()).unwrap();
        let parked = found.unserved.iter().find(|a| a.title == "Here").expect("what a parked folder held");
        let by = album_pid_for_tests("Here", "Artist");
        assert_eq!((parked.folder, parked.old_id.as_str()), (0, ids::album_at(&by, 0).as_str()), "by the id it had");
        assert!(parked.files[0].0 < 0, "and it lies where the catalogue keeps it: {:?}", parked.files);
        assert_eq!(parked.tracks[0].0, track.unwrap());
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// CR13, through: an album in a trash at the first start has its record, and its Undo brings it back to it, so what names the
    /// album still does. With none it would come back as new, and all that stayed under its old id for good.
    #[test]
    fn an_album_undone_from_a_trash_comes_back_to_its_record() {
        let (roots, cat, lib) = two_kept("first-trash");
        add_files_to(&cat, 0, &roots[0], &[("Away/01 One.flac", "Away", "One"), ("Here/01 Two.flac", "Here", "Two")]);
        cat.mark_trashed(0, "Away", "token").unwrap();
        cat.first_records(first_albums(&cat, &Rules::default()).unwrap(), &TWO_KEPT).unwrap();
        let old = ids::album_at(&album_pid_for_tests("Away", "Artist"), 0);
        let record = cat.named_now(&old).expect("the id it was served by leads to a record");
        build_now(&lib, &cat);
        assert!(lib.album(&record).unwrap().is_none(), "in a trash, it is listed nowhere");
        cat.unmark_trashed(0, "Away", "token").unwrap();
        build_now(&lib, &cat);
        assert_eq!(lib.album(&record).unwrap().map(|a| a.name).as_deref(), Some("Away"), "back on its record");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    const ONE_TWO: [(&str, &str, &str); 2] = [("A/01 One.flac", "A", "One"), ("A/02 Two.flac", "A", "Two")];

    /// `taken` on two libraries of these kinds: the two folders, the catalogue, the library, the album and Music's first track.
    fn a_record_in(name: &str, second: Kind) -> ([std::path::PathBuf; 2], crate::catalogue::Catalogue, Library, String, Option<i64>) {
        let (roots, cat, lib) = two_kept(name);
        lib.set_kinds(vec![(0, Kind::Main), (1, second)]);
        let album = taken(&roots, &cat, &lib);
        let track = cat.track_of(0, "A/01 One.flac");
        assert_eq!(track, cat.track_of(1, "A/01 One.flac"), "one set of tracks");
        (roots, cat, lib, album, track)
    }

    /// Music holds what it held: its id, its tracks and all that names them.
    fn stays_with_music(cat: &crate::catalogue::Catalogue, lib: &Library, album: &str, track: Option<i64>) {
        assert_eq!(cat.track_of(0, "A/01 One.flac"), track, "Music's file on its track");
        assert_eq!(lib.album_in(0, "A").unwrap().as_deref(), Some(album), "and Music's copy the record");
        assert_ne!(cat.track_of(1, "A/01 One.flac"), track, "the other copy left");
        assert_ne!(lib.album_in(1, "A").unwrap().as_deref(), Some(album));
    }

    /// CR18, the owner: who keeps a record goes by kind first, and track count decides only between copies of one kind. Nobody
    /// has checked an Incoming copy: with a track more it leaves, and what names the record stays on Music's files.
    #[test]
    fn a_kept_copy_keeps_its_record_from_an_incoming_copy_with_a_track_more() {
        let (roots, cat, lib, album, track) = a_record_in("kept-more", Kind::Incoming);
        add_files_to(&cat, 1, &roots[1], &[("A/03 Three.flac", "A", "Three")]);
        build_now(&lib, &cat);
        stays_with_music(&cat, &lib, &album, track);
    }

    /// CR18: one bad track removed from Music must not hand the album to Incoming.
    #[test]
    fn a_kept_copy_keeps_its_record_from_an_incoming_copy_when_a_file_left_it() {
        let (roots, cat, lib, album, track) = a_record_in("kept-less", Kind::Incoming);
        std::fs::remove_file(roots[0].join("A/02 Two.flac")).unwrap();
        cat.walk_all();
        cat.gone_long_ago_for_tests();
        cat.identify().unwrap();
        build_now(&lib, &cat);
        stays_with_music(&cat, &lib, &album, track);
    }

    /// CR18: Music gaining more than it held is no majority for it, and the Incoming copy still holding the record's files must
    /// not stop it being the record's by what is kept.
    #[test]
    fn a_kept_copy_that_gained_more_than_it_held_keeps_its_record_from_an_incoming_copy() {
        let (roots, cat, lib, album, track) = a_record_in("kept-grown", Kind::Incoming);
        add_files_to(&cat, 0, &roots[0], &[("A/03 Three.flac", "A", "Three"), ("A/04 Four.flac", "A", "Four"), ("A/05 Five.flac", "A", "Five")]);
        build_now(&lib, &cat);
        stays_with_music(&cat, &lib, &album, track);
    }

    /// CR18: one account's act must not rewrite what every account has.
    #[test]
    fn someones_own_copy_never_takes_a_record_from_music() {
        let (roots, cat, lib, album, track) = a_record_in("own-more", Kind::Own);
        add_files_to(&cat, 1, &roots[1], &[("A/03 Three.flac", "A", "Three")]);
        build_now(&lib, &cat);
        stays_with_music(&cat, &lib, &album, track);
    }

    /// Between copies of one kind the track count decides, as Q8 said, and CR18 leaves that as it was.
    #[test]
    fn of_two_copies_of_one_kind_the_one_with_more_keeps_the_record() {
        let (roots, cat, lib, album, track) = a_record_in("main-more", Kind::Main);
        add_files_to(&cat, 1, &roots[1], &[("A/03 Three.flac", "A", "Three")]);
        build_now(&lib, &cat);
        assert_eq!(cat.track_of(1, "A/01 One.flac"), track);
        assert_eq!(lib.album_in(1, "A").unwrap().as_deref(), Some(album.as_str()));
        assert_ne!(cat.track_of(0, "A/01 One.flac"), track);
    }

    /// These files of one release in `folder`, read and identified, tagged as a tagger leaves them.
    fn add_release_to(cat: &crate::catalogue::Catalogue, folder: i64, root: &Path, files: &[(&str, &str)]) {
        for (rel, _) in files {
            std::fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
            std::fs::write(root.join(rel), rel).unwrap();
        }
        cat.walk_all();
        cat.pretend_read_for_tests();
        for (rel, title) in files {
            cat.tag_for_tests(folder, rel, &[("album", "A"), ("albumartist", "Artist"), ("title", title), ("musicbrainz_albumid", "release-1")]);
        }
        cat.identify().unwrap();
    }

    /// One record of three tracks in Music, its copy in the other library holding the last alone: a folder that has only just begun.
    fn a_copy_begun(name: &str, kind: Kind) -> ([std::path::PathBuf; 2], crate::catalogue::Catalogue, Library, String) {
        let (roots, cat, lib) = two_kept(name);
        lib.set_kinds(vec![(0, Kind::Main), (1, kind)]);
        add_release_to(&cat, 0, &roots[0], &[("A/01 One.flac", "One"), ("A/02 Two.flac", "Two"), ("A/03 Three.flac", "Three")]);
        build_now(&lib, &cat);
        let album = lib.album_in(0, "A").unwrap().expect("in Music");
        add_release_to(&cat, 1, &roots[1], &[("A/03 Three.flac", "Three")]);
        build_now(&lib, &cat);
        (roots, cat, lib, album)
    }

    /// The owner, 2026-11-07, on real rows: one record, one card, three tracks, the copy saying it holds one of them, and nothing for
    /// the next build to read.
    #[test]
    fn a_folder_of_the_very_same_release_is_a_copy_from_its_first_file() {
        let (roots, cat, lib, album) = a_copy_begun("begun", Kind::Incoming);
        assert_eq!(cat.track_of(1, "A/03 Three.flac"), cat.track_of(0, "A/03 Three.flac"), "the track it is");
        assert_eq!(lib.album_in(1, "A").unwrap().as_deref(), Some(album.as_str()), "a copy of the record");
        assert_eq!(one_number(&lib, "SELECT count(*) FROM albums"), 1, "one card");
        assert_eq!(lib.album(&album).unwrap().expect("the record").song_count, 3, "listing what it lists");
        let short: Vec<(i64, i64, bool)> = lib.db().prepare("SELECT folder, tracks, short FROM copies ORDER BY folder").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
        assert_eq!(short, [(0, 3, false), (1, 1, true)]);
        assert!(albums_now(&cat).iter().all(|a| !a.split), "nothing left to settle");
        assert_eq!(build_now(&lib, &cat), (false, 0));
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// What plays stays what it was until the copy is all there: Music's, in another kept library as in Incoming, whatever its quality.
    /// Named by `from`, the copy is heard for what it holds, and a track it lacks is passed over, not asked of it.
    #[test]
    fn a_copy_short_of_tracks_changes_nothing_that_plays() {
        for kind in [Kind::Incoming, Kind::Main, Kind::Own] {
            let (roots, cat, lib, album) = a_copy_begun("plays", kind);
            let [music, other] = [0, 1].map(|folder| (folder, "A/03 Three.flac".to_string()));
            let song = |rel: &str| ids::song(cat.track_of(0, rel).unwrap());
            let (three, one) = (song("A/03 Three.flac"), song("A/01 One.flac"));
            let hears = |id: &str, from: Option<i64>| {
                let who = crate::serve::listener::Listener { account: Some("ann".into()), from, ..Default::default() };
                lib.file_for(id, &[], &who).unwrap()
            };
            assert_eq!(hears(&three, None), Some(music.clone()), "{kind:?}: Music's, as before it came");
            assert_eq!(lib.row_copy(&album).unwrap().map(|c| c.folder), Some(0), "{kind:?}: the record's facts and cover are Music's");
            assert_eq!(hears(&three, Some(1)), Some(other), "{kind:?}: by `from`");
            assert_eq!(hears(&one, Some(1)), Some((0, "A/01 One.flac".to_string())), "{kind:?}: a track it does not hold");
            roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
        }
    }

    /// Its last file in, it is a copy like any other: whole, and on the record's tracks, each its own.
    #[test]
    fn a_copy_all_there_counts_as_whole() {
        let (roots, cat, lib, album) = a_copy_begun("whole", Kind::Main);
        add_release_to(&cat, 1, &roots[1], &[("A/01 One.flac", "One")]);
        build_now(&lib, &cat);
        add_release_to(&cat, 1, &roots[1], &[("A/02 Two.flac", "Two")]);
        build_now(&lib, &cat);
        for rel in ["A/01 One.flac", "A/02 Two.flac", "A/03 Three.flac"] {
            assert_eq!(cat.track_of(1, rel), cat.track_of(0, rel), "{rel}");
        }
        assert_eq!(lib.copies_of(&album).unwrap().iter().map(|c| c.short).collect::<Vec<_>>(), [false, false]);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM albums"), 1);
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// A build of some folders writes what a whole build writes: a copy's word on itself goes with the record, as every copy of it is written.
    #[test]
    fn a_copy_says_it_is_short_however_it_was_built() {
        let (roots, cat, lib, album) = a_copy_begun("built", Kind::Incoming);
        let whole = Library::open(None).unwrap();
        whole.set_kinds(lib.kinds());
        build_now(&whole, &cat);
        let read = |l: &Library| l.copies_of(&album).unwrap().iter().map(|c| (c.folder, c.short)).collect::<Vec<_>>();
        assert_eq!(read(&lib), read(&whole));
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// What a record says, as every list reads it: (year, genre, release).
    fn says(lib: &Library, album: &str) -> (Option<u32>, Option<String>, Option<String>) {
        let a = lib.album(album).unwrap().expect("the record");
        (a.year, a.genre, a.music_brainz_id)
    }

    /// These tags on the files of `folder`, as `add_files_to` left them but for what is added.
    fn tagged(cat: &crate::catalogue::Catalogue, folder: i64, tags: &[(&str, &str)]) {
        for (rel, album, title) in ONE_TWO {
            let tags: Vec<(&str, &str)> = [("album", album), ("albumartist", "Artist"), ("title", title)].into_iter().chain(tags.iter().copied()).collect();
            cat.tag_for_tests(folder, rel, &tags);
        }
    }

    /// The owner, 2026-11-07: a person's own copy never changes what others see. Saying more than Music's, a year, a genre, a release,
    /// it says none of it for the record, and the record reads as it did before that copy came.
    #[test]
    fn someones_own_copy_says_nothing_of_a_record_in_a_main_library() {
        let (roots, cat, lib, album, _) = a_record_in("own-says", Kind::Own);
        let before = says(&lib, &album);
        assert_eq!(before, (None, None, None), "Music's says none of it");
        tagged(&cat, 1, &[("date", "1999"), ("genre", "Own Word"), ("musicbrainz_albumid", "own-release")]);
        build_now(&lib, &cat);
        assert_eq!(says(&lib, &album), before);
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// What a main library says stands over anything else's.
    #[test]
    fn a_main_library_decides_over_every_other_copy() {
        for second in [Kind::Own, Kind::Incoming] {
            let (roots, cat, lib, album, _) = a_record_in("main-says", second);
            tagged(&cat, 0, &[("date", "2001"), ("genre", "Music's Word"), ("musicbrainz_albumid", "music-release")]);
            tagged(&cat, 1, &[("date", "1999"), ("genre", "Other Word"), ("musicbrainz_albumid", "other-release")]);
            build_now(&lib, &cat);
            assert_eq!(says(&lib, &album), (Some(2001), Some("Music's Word".to_string()), Some("music-release".to_string())), "{second:?}");
            roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
        }
    }

    /// Incoming is nobody's own: where Music says nothing its copy still fills in, as a record must not lack a year for that.
    #[test]
    fn an_incoming_copy_still_fills_in_what_a_main_library_does_not_say() {
        let (roots, cat, lib, album, _) = a_record_in("incoming-says", Kind::Incoming);
        tagged(&cat, 1, &[("date", "1999"), ("genre", "Its Word"), ("musicbrainz_albumid", "its-release")]);
        build_now(&lib, &cat);
        assert_eq!(says(&lib, &album), (Some(1999), Some("Its Word".to_string()), Some("its-release".to_string())));
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// Only in people's own music the first copy added decides: the second person's, better or saying more, changes nothing, in the
    /// later library or the earlier one, as the libraries' order is not when a copy came.
    #[test]
    fn of_a_record_only_in_peoples_own_music_the_first_copy_added_decides() {
        for (first, second) in [(0, 1), (1, 0)] {
            let (roots, cat, lib) = two_kept("first-says");
            lib.set_kinds(vec![(0, Kind::Own), (1, Kind::Own)]);
            add_files_to(&cat, first, &roots[first as usize], &ONE_TWO);
            cat.came_for_tests(first, 100);
            tagged(&cat, first, &[("date", "2001")]);
            build_now(&lib, &cat);
            let album = lib.album_in(first, "A").unwrap().expect("a record");
            add_files_to(&cat, second, &roots[second as usize], &ONE_TWO);
            cat.came_for_tests(second, 200);
            tagged(&cat, second, &[("date", "1999"), ("genre", "Later Word"), ("musicbrainz_albumid", "later-release")]);
            build_now(&lib, &cat);
            assert_eq!(lib.album_in(second, "A").unwrap().as_deref(), Some(album.as_str()), "one record");
            assert_eq!(says(&lib, &album), (Some(2001), None, None), "first folder {first}");
            roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
        }
    }

    /// A record in the first folder, then a copy of it in the second: what the take leaves. The album's id.
    fn taken(roots: &[std::path::PathBuf; 2], cat: &crate::catalogue::Catalogue, lib: &Library) -> String {
        add_files_to(cat, 0, &roots[0], &ONE_TWO);
        build_now(lib, cat);
        add_files_to(cat, 1, &roots[1], &ONE_TWO);
        build_now(lib, cat);
        lib.db().query_row("SELECT id FROM albums", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn a_copy_takes_its_records_tracks_and_the_record_lists_them_once() {
        let (roots, cat, lib) = two_kept("take");
        let album = taken(&roots, &cat, &lib);
        assert_eq!(cat.track_of(1, "A/02 Two.flac"), cat.track_of(0, "A/02 Two.flac"), "the second copy's file on the first's track");
        assert_eq!(lib.counts().unwrap().0, 2, "no track twice");
        assert_eq!(lib.album(&album).unwrap().expect("one album").song_count, 2);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM copies"), 2);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM copy_files"), 4);
        assert!(albums_now(&cat).iter().all(|a| !a.split), "settled: nothing for the next build to read");
        assert_eq!(build_now(&lib, &cat), (false, 0), "built with its tracks: nothing to build again");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// The row copy lacks a file for now: the record still lists the track, from the copy that has it, and a stream reaches it.
    #[test]
    fn a_copy_lacking_a_file_still_lists_every_track() {
        let (roots, cat, lib) = two_kept("lacking");
        let album = taken(&roots, &cat, &lib);
        let (rel, track) = ("A/02 Two.flac", cat.track_of(0, "A/02 Two.flac").unwrap());
        let row = lib.row_copy(&album).unwrap().unwrap().folder;
        let (row_root, other) = (&roots[row as usize], 1 - row);
        std::fs::remove_file(row_root.join(rel)).unwrap();
        cat.walk_all();
        build_now(&lib, &cat);
        assert_eq!(lib.album(&album).unwrap().expect("one album").song_count, 2, "every track");
        assert_eq!(lib.file_of(&ids::song(track)).unwrap(), Some((other, rel.to_string())), "from the copy that has it");
        let who = crate::serve::listener::Listener::default();
        assert_eq!(lib.file_for(&ids::song(track), &cat.files_of(track), &who).unwrap(), Some((other, rel.to_string())));
        assert_eq!(one_number(&lib, "SELECT count(*) FROM albums"), 1, "for now: it splits nothing");
        assert_eq!(cat.track_of(row, "A/01 One.flac"), cat.track_of(other, "A/01 One.flac"), "and gives the copy no tracks of its own");
        // Back: the same again.
        std::fs::write(row_root.join(rel), rel).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        build_now(&lib, &cat);
        assert_eq!(cat.track_of(row, rel), Some(track));
        assert_eq!(lib.file_of(&ids::song(track)).unwrap(), Some((row, rel.to_string())), "the row copy's again");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// N3 where the file joined its album's record first, as a track Cratarr wrote does: nothing lacks a record, and only the
    /// two copies' ids say there is a track to take. Left alone the record lists a track more.
    #[test]
    fn a_fresh_file_in_a_copy_is_taken_at_the_next_build() {
        let (roots, cat, lib) = two_kept("fresh");
        let three = [("A/01 One.flac", "A", "One"), ("A/02 Two.flac", "A", "Two"), ("A/03 Three.flac", "A", "Three")];
        add_files_to(&cat, 0, &roots[0], &three);
        build_now(&lib, &cat);
        add_files_to(&cat, 1, &roots[1], &three);
        build_now(&lib, &cat);
        let track = cat.track_of(0, "A/03 Three.flac");
        assert_eq!(cat.track_of(1, "A/03 Three.flac"), track);
        // Replaced under another name, with no artist to pair the two by: a fresh id.
        std::fs::remove_file(roots[1].join("A/03 Three.flac")).unwrap();
        add_files_to(&cat, 1, &roots[1], &[("A/03 Replaced.flac", "A", "Three")]);
        assert_ne!(cat.track_of(1, "A/03 Replaced.flac"), track);
        lib.update_files(&cat, &Rules::default(), &[roots[1].join("A/03 Replaced.flac")]).unwrap();
        assert!(!cat.records_unsettled(), "it joined its album's record");
        build_now(&lib, &cat);
        assert_eq!(cat.track_of(1, "A/03 Replaced.flac"), track);
        assert_eq!(lib.counts().unwrap().0, 3, "not a track more");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// Q8 on a real build: two albums, the one with the track added under the record's id, every track served, and a pass
    /// with nothing left to settle.
    #[test]
    fn a_track_added_to_one_of_two_copies_is_two_records() {
        let (roots, cat, lib) = two_kept("added");
        let album = taken(&roots, &cat, &lib);
        let first = cat.track_of(0, "A/01 One.flac");
        add_files_to(&cat, 1, &roots[1], &[("A/03 Three.flac", "A", "Three")]);
        build_now(&lib, &cat);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM albums"), 2);
        assert_eq!(lib.album(&album).unwrap().expect("the copy with more tracks keeps it").song_count, 3);
        assert_eq!(lib.album_in(1, "A").unwrap().as_deref(), Some(album.as_str()));
        assert_eq!(cat.track_of(1, "A/01 One.flac"), first, "and its tracks");
        assert_ne!(cat.track_of(0, "A/01 One.flac"), first, "the other takes tracks of its own");
        assert_ne!(lib.album_in(0, "A").unwrap().as_deref(), Some(album.as_str()));
        assert_eq!(lib.counts().unwrap().0, 5, "every file served");
        assert!(albums_now(&cat).iter().all(|a| !a.split));
        assert_eq!(build_now(&lib, &cat), (false, 0));
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// m1: t1, t2 shared with an album of the lower record, t3, t4 free on the higher. The catalogue gave the album the higher
    /// record, and a list must serve it so, or `split` would stay true and every build read it all again.
    #[test]
    fn an_album_is_served_as_the_record_the_catalogue_gave_it() {
        let (root, cat) = catalogue_in("one-vote");
        add_files(&cat, &root, &[("Low/01 One.flac", "Low", "One"), ("Low/02 Two.flac", "Low", "Two")]);
        add_files(&cat, &root, &[("High/03 Three.flac", "High", "Three"), ("High/04 Four.flac", "High", "Four")]);
        let lib = Library::open(None).unwrap();
        build_now(&lib, &cat);
        let (low, high) = (record_at(&cat, "Low/01 One.flac").unwrap(), record_at(&cat, "High/03 Three.flac").unwrap());
        assert!(low < high, "the lower record would win a tie");
        // Two of the higher's files moved into a folder with two of the lower's, those being another file of its tracks.
        std::fs::create_dir_all(root.join("Mix")).unwrap();
        for name in ["03 Three.flac", "04 Four.flac"] {
            std::fs::rename(root.join("High").join(name), root.join("Mix").join(name)).unwrap();
        }
        add_files(&cat, &root, &[("Mix/01 Again.flac", "Mix", "One"), ("Mix/02 Again.flac", "Mix", "Two")]);
        for (name, title) in [("03 Three.flac", "Three"), ("04 Four.flac", "Four")] {
            cat.tag_for_tests(0, &format!("Mix/{name}"), &[("album", "Mix"), ("albumartist", "Artist"), ("title", title)]);
        }
        let shared = ["Low/01 One.flac", "Low/02 Two.flac"].map(|rel| cat.track_of(0, rel));
        cat.set_tracks(&[((0, "Mix/01 Again.flac".to_string()), shared[0]), ((0, "Mix/02 Again.flac".to_string()), shared[1])]).unwrap();
        build_now(&lib, &cat);
        let served: Vec<String> = lib.albums_of_files(&[(0, "Mix/03 Three.flac".to_string())]).unwrap();
        assert_eq!(served, [ids::record(high)], "what `Held::vote` says, not the lower of a tie");
        assert!(albums_now(&cat).iter().all(|a| !a.split), "nothing left to settle");
        std::fs::remove_dir_all(&root).ok();
    }

    fn build_now(lib: &Library, cat: &crate::catalogue::Catalogue) -> (bool, usize) {
        let (_, print) = lib.change(cat, &Rules::default()).unwrap();
        lib.build_from(cat, &Rules::default(), &print).unwrap()
    }

    /// A file's artist tags as a person's tagger leaves them, read back by the next pass.
    fn credit(cat: &crate::catalogue::Catalogue, rel: &str, album: &str, artist: &str, title: &str) {
        cat.tag_for_tests(0, rel, &[("album", album), ("albumartist", artist), ("artist", artist), ("title", title)]);
    }

    fn served_artists(lib: &Library) -> Vec<(String, String)> {
        lib.db().prepare("SELECT id, name FROM artists ORDER BY name").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    }

    /// An id for life (0.16): nothing a name goes through without becoming another name moves it. Spelt in capitals by a
    /// tagger, gone from the library, back again: what the user data holds under it still finds them.
    #[test]
    fn an_artist_keeps_its_id_across_a_rename_of_nothing() {
        let d = std::env::temp_dir().join(format!("cratarr-artist-for-life-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let cat = crate::catalogue::Catalogue::open(None, vec![d.clone()]).unwrap();
        let lib = Library::open(None).unwrap();
        add_files(&cat, &d, &[("A/01 a.flac", "Early", "One"), ("B/01 b.flac", "Later", "Two")]);
        credit(&cat, "A/01 a.flac", "Early", "Salt Meridian", "One");
        credit(&cat, "B/01 b.flac", "Later", "Orbit Lane", "Two");
        build_now(&lib, &cat);
        let id = cat.artist_of("Salt Meridian").unwrap().expect("an id, given by the build");
        assert_eq!(id, cat.artist_of("salt meridian").unwrap().unwrap());
        let served = ids::artist(id);
        assert!(served.starts_with("ar-") && served[3..].parse::<i64>().is_ok(), "{served}");
        assert!(served_artists(&lib).contains(&(served.clone(), "Salt Meridian".to_string())));

        credit(&cat, "A/01 a.flac", "Early", "SALT MERIDIAN", "One");
        build_now(&lib, &cat);
        assert!(served_artists(&lib).contains(&(served.clone(), "SALT MERIDIAN".to_string())), "the same id, as the files now spell them");

        credit(&cat, "A/01 a.flac", "Early", "Orbit Lane", "One");
        build_now(&lib, &cat);
        assert!(!served_artists(&lib).iter().any(|(a, _)| *a == served), "no file credits them");
        credit(&cat, "A/01 a.flac", "Early", "Salt Meridian", "One");
        build_now(&lib, &cat);
        assert!(served_artists(&lib).contains(&(served, "Salt Meridian".to_string())), "back on their id");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A name first met in a build of some folders is given its id like any other, and what that build writes names it: songs,
    /// the album's artist and the artist's own row all agree, as a whole build would have them.
    #[test]
    fn a_new_name_in_a_build_of_one_folder_is_served_by_its_id() {
        let d = std::env::temp_dir().join(format!("cratarr-artist-partial-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let cat = crate::catalogue::Catalogue::open(None, vec![d.clone()]).unwrap();
        let lib = Library::open(None).unwrap();
        add_files(&cat, &d, &[("A/01 a.flac", "Early", "One")]);
        build_now(&lib, &cat);
        add_files(&cat, &d, &[("A/01 a.flac", "Early", "One"), ("B/01 b.flac", "Later", "Two")]);
        credit(&cat, "B/01 b.flac", "Later", "Orbit Lane", "Two");
        let (whole, _) = build_now(&lib, &cat);
        assert!(!whole, "a build of the folder that came");
        let orbit = ids::artist(cat.artist_of("Orbit Lane").unwrap().unwrap());
        let of = |sql: &str| -> String { lib.db().query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(of("SELECT artist_id FROM albums WHERE name = 'Later'"), orbit);
        assert_eq!(of("SELECT artist_id FROM song_artists WHERE name = 'Orbit Lane'"), orbit);
        assert_eq!(of("SELECT artist_id FROM album_artists WHERE name = 'Orbit Lane'"), orbit);
        assert_eq!(of("SELECT id FROM artists WHERE name = 'Orbit Lane'"), orbit);
        std::fs::remove_dir_all(&d).ok();
    }

    /// Cratarr's own write builds at once by `update_files`, with no `build_from` before it: after a restart no id has been
    /// read yet, and a retag can bring a name no build has met. Either would stop the write from showing until the next pass.
    #[test]
    fn a_write_after_a_restart_finds_its_artists_and_gives_a_new_name_its_id() {
        let d = std::env::temp_dir().join(format!("cratarr-artist-write-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        let (root, file, rel) = (d.join("music"), d.join("library.sqlite"), "A/01 a.flac");
        let cat = crate::catalogue::Catalogue::open(None, vec![root.clone()]).unwrap();
        let lib = Library::open(Some(&file)).unwrap();
        add_files(&cat, &root, &[(rel, "Early", "One")]);
        build_now(&lib, &cat);
        drop(lib);

        let lib = Library::open(Some(&file)).unwrap();
        let wrote = |lib: &Library| lib.update_files(&cat, &Rules::default(), &[root.join(rel)]).unwrap();
        cat.tag_for_tests(0, rel, &[("album", "Early"), ("albumartist", "Artist"), ("title", "One again")]);
        assert_eq!(wrote(&lib), 1, "a name built before");
        credit(&cat, rel, "Early", "Orbit Lane", "One again");
        assert_eq!(wrote(&lib), 1, "a name no build has met");
        let orbit = ids::artist(cat.artist_of("Orbit Lane").unwrap().unwrap());
        assert!(served_artists(&lib).contains(&(orbit, "Orbit Lane".to_string())));
        std::fs::remove_dir_all(&d).ok();
    }

    /// `build_from` gives every name its id: one without is a fault there, and must stop the write, not reach the lists.
    #[test]
    fn a_build_refuses_a_name_with_no_id() {
        let facts = Library::open(None).unwrap().facts();
        assert!(facts.artist("Nobody").unwrap_err().to_string().contains("Nobody"));
    }

    fn record_at(cat: &crate::catalogue::Catalogue, rel: &str) -> Option<i64> {
        cat.served_rows(0).unwrap().iter().find(|r| r.rel == rel).expect("served").record
    }

    /// Built from rows read before the records were given, a new track would have none, and its row would print otherwise
    /// at the next pass and build again.
    #[test]
    fn a_retag_of_one_disc_keeps_every_track() {
        let (root, cat) = catalogue_in("one-disc");
        let files = [("X/Album/CD1/01 One.flac", "Album", "One"), ("X/Album/CD1/02 Two.flac", "Album", "Two"), ("X/Album/CD2/01 Three.flac", "Album", "Three"), ("X/Album/CD2/02 Four.flac", "Album", "Four")];
        add_files(&cat, &root, &files);
        let lib = Library::open(None).unwrap();
        build_now(&lib, &cat);
        assert_eq!((lib.counts().unwrap().0, one_number(&lib, "SELECT count(*) FROM albums")), (4, 1), "one album over two folders");
        for (rel, album, title) in &files[2..] {
            cat.tag_for_tests(0, rel, &[("album", album), ("albumartist", "Artist"), ("title", title), ("date", "1999")]);
        }
        cat.changed_outside_pass();
        build_now(&lib, &cat);
        assert_eq!((lib.counts().unwrap().0, one_number(&lib, "SELECT count(*) FROM albums")), (4, 2), "two albums again, as before records, every track served");
        assert_ne!(record_at(&cat, files[0].0), record_at(&cat, files[2].0));
        build_now(&lib, &cat);
        assert_eq!(lib.counts().unwrap().0, 4, "and it stays so");
        std::fs::remove_dir_all(&root).ok();
    }

    /// The other way to come by it, with one disc moved outside Cratarr: what is left and what moved are not the same tracks.
    #[test]
    fn two_groups_of_one_record_that_are_other_tracks_are_two_records() {
        use crate::catalogue::identity::same_record;
        let rows = |first: i64, titles: [&str; 2], date: &str| -> Vec<ServedRow> {
            titles.iter().zip(first..).map(|(title, track)| ServedRow { record: Some(7), ..row(track, &format!("X/{date}/{track} {title}.flac"), &[("title", title), ("albumartist", "Artist"), ("album", "Album"), ("date", date)]) }).collect()
        };
        let copies = album_copies(&[in_folder(0, rows(1, ["One", "Two"], "1998")), in_folder(1, rows(3, ["Three", "Four"], "1999"))], &Rules::default());
        assert_eq!(copies.len(), 2);
        assert!(!same_record(&copies[0].facts, &copies[1].facts));
        assert!(copies.iter().all(|a| a.split), "worth the catalogue's reading, each held by one record though its tracks are unanimous");
        let copies = album_copies(&two_copies(), &Rules::default());
        assert!(copies.iter().all(|a| !a.split), "two copies of one record are as they should be");
    }

    #[test]
    fn a_new_track_has_its_record_in_the_same_build() {
        let (root, cat) = catalogue_in("same-build");
        add_files(&cat, &root, &[("A/01 One.flac", "A", "One"), ("A/02 Two.flac", "A", "Two")]);
        let lib = Library::open(None).unwrap();
        assert_eq!(build_now(&lib, &cat), (true, 0));
        assert!(record_at(&cat, "A/01 One.flac").is_some());
        assert_eq!(build_now(&lib, &cat), (false, 0), "built with its record: nothing to build again");
        add_files(&cat, &root, &[("A/03 Three.flac", "A", "Three")]);
        assert_eq!(build_now(&lib, &cat), (false, 1));
        assert_eq!(record_at(&cat, "A/03 Three.flac"), record_at(&cat, "A/01 One.flac"));
        assert_eq!(build_now(&lib, &cat), (false, 0), "built with its record: nothing to build again");
        std::fs::remove_dir_all(&root).ok();
    }

    /// m5 on a real build: a favourite names `al-<record>`, so more files added at once than it had must leave that id served.
    #[test]
    fn a_record_stays_served_under_its_id_when_more_came_in_than_it_had() {
        let (root, cat) = catalogue_in("came-in");
        add_files(&cat, &root, &[("A/01 One.flac", "A", "One"), ("A/02 Two.flac", "A", "Two"), ("A/03 Three.flac", "A", "Three")]);
        let lib = Library::open(None).unwrap();
        build_now(&lib, &cat);
        let id: String = lib.db().query_row("SELECT id FROM albums", [], |r| r.get(0)).unwrap();
        let more: Vec<(String, String)> = (4..=10).map(|n| (format!("A/{n:02} More.flac"), format!("More {n}"))).collect();
        let more: Vec<(&str, &str, &str)> = more.iter().map(|(rel, title)| (rel.as_str(), "A", title.as_str())).collect();
        add_files(&cat, &root, &more);
        build_now(&lib, &cat);
        assert_eq!(lib.counts().unwrap().0, 10);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM albums"), 1);
        assert_eq!(lib.album(&id).unwrap().expect("still served").song_count, 10);
        std::fs::remove_dir_all(&root).ok();
    }

    /// A copy's kind decides which copy `songs` holds, so a library that changes kind leaves no album as it was.
    #[test]
    fn a_changed_kind_builds_whole() {
        let (root, cat) = catalogue_in("kinds");
        add_files(&cat, &root, &[("A/01 One.flac", "A", "One")]);
        let lib = Library::open(None).unwrap();
        lib.set_kinds(vec![(0, Kind::Main)]);
        assert_eq!(build_now(&lib, &cat), (true, 0));
        assert_eq!(lib.change(&cat, &Rules::default()).unwrap().0, Change::None);
        lib.set_kinds(vec![(0, Kind::Incoming)]);
        assert_eq!(lib.change(&cat, &Rules::default()).unwrap().0, Change::Songs, "a check sees it with no file changed");
        assert_eq!(build_now(&lib, &cat), (true, 0));
        assert_eq!(lib.change(&cat, &Rules::default()).unwrap().0, Change::None);
        std::fs::remove_dir_all(&root).ok();
    }

    /// Folders have their ids for life: a second main library or someone's own music has an id above 0, and Incoming can
    /// have the lowest.
    #[test]
    fn a_later_folder_can_be_kept() {
        let lib = Library::open(None).unwrap();
        let mut folders = sample();
        let orb = folders[0].rows.pop().unwrap();
        folders.push(FolderRows { folder: 3, root: "/more".into(), rows: vec![orb] });
        lib.refresh(&folders, &Rules::default()).unwrap();
        let waiting = |lib: &Library| lib.incoming_albums().unwrap().into_iter().map(|a| a.name).collect::<Vec<_>>();
        assert_eq!(waiting(&lib), ["Adventures Beyond the Ultraworld"], "with no word from the app, as it was");
        let orb: String = lib.db().query_row("SELECT album FROM copies WHERE folder = 3", [], |r| r.get(0)).unwrap();
        let bjork: String = lib.db().query_row("SELECT album FROM copies WHERE folder = 0", [], |r| r.get(0)).unwrap();
        for kept in [Kind::Main, Kind::Own] {
            lib.set_kinds(vec![(0, Kind::Main), (3, kept)]);
            assert!(waiting(&lib).is_empty(), "{kept:?}");
            assert!(!lib.in_incoming(&orb).unwrap(), "{kept:?}");
        }
        lib.set_kinds(vec![(3, Kind::Main), (0, Kind::Incoming)]);
        assert_eq!(waiting(&lib), ["Homogenic"], "the first folder is no longer kept");
        assert!(lib.in_incoming(&bjork).unwrap());
        assert_eq!(lib.kind_of(3), (Kind::Main, 0), "its place in the libraries' order, not its id");
    }

    /// `covers::place`, `record_health` and `place_track` write where a record plays for most people, not in the lowest
    /// folder id.
    #[test]
    fn the_row_copy_is_the_best_kept_one_not_the_lowest_folder() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        let album: String = lib.db().query_row("SELECT id FROM albums WHERE name = 'Homogenic'", [], |r| r.get(0)).unwrap();
        lib.db()
            .execute_batch(
                "UPDATE copies SET format = 'MP3', min_kbps = 192, bits = NULL WHERE dir = 'Björk/Homogenic (1997)';
                 INSERT INTO copies SELECT album, 1, 'Better', 'FLAC', 900, size, added, tracks, 16, 44100, 0 FROM copies WHERE dir = 'Björk/Homogenic (1997)';
                 INSERT INTO copies SELECT album, 2, 'Waiting', 'FLAC', 900, size, added, tracks, 24, 96000, 0 FROM copies WHERE dir = 'Björk/Homogenic (1997)';",
            )
            .unwrap();
        lib.set_kinds(vec![(0, Kind::Main), (1, Kind::Main), (2, Kind::Incoming)]);
        assert_eq!(lib.row_copy(&album).unwrap().map(|c| (c.folder, c.dir)), Some((1, "Better".to_string())));
        let copy = lib.copies_of(&album).unwrap().remove(1);
        assert_eq!((copy.kind, copy.order, copy.format.as_deref(), copy.min_kbps, copy.bits, copy.rate), (Kind::Main, 1, Some("FLAC"), Some(900), Some(16), Some(44_100)));
        assert_eq!(lib.row_copy("al-nothing").unwrap(), None);
    }

    /// A track Cratarr added shows at once, so it takes its album's record there and then. A new folder must wait: held
    /// against these few albums it would be a record of its own for life.
    #[test]
    fn update_files_joins_and_never_gives_birth() {
        let (root, cat) = catalogue_in("join");
        let rules = Rules::default();
        add_files(&cat, &root, &[("A/01 One.flac", "A", "One"), ("A/02 Two.flac", "A", "Two")]);
        let lib = Library::open(None).unwrap();
        build_now(&lib, &cat);
        add_files(&cat, &root, &[("A/03 Three.flac", "A", "Three")]);
        assert_eq!(lib.update_files(&cat, &rules, &[root.join("A/03 Three.flac")]).unwrap(), 1);
        assert_eq!(record_at(&cat, "A/03 Three.flac"), record_at(&cat, "A/01 One.flac"), "joined by its album's tracks");
        assert_eq!(build_now(&lib, &cat), (false, 0), "and built with it");
        add_files(&cat, &root, &[("B/01 Alone.flac", "B", "Alone")]);
        assert_eq!(lib.update_files(&cat, &rules, &[root.join("B/01 Alone.flac")]).unwrap(), 1);
        assert_eq!(record_at(&cat, "B/01 Alone.flac"), None, "no album to join, and no birth");
        assert!(cat.records_unsettled(), "the next build's to give");
        build_now(&lib, &cat);
        let alone = record_at(&cat, "B/01 Alone.flac");
        assert!(alone.is_some() && alone != record_at(&cat, "A/01 One.flac"));
        std::fs::remove_dir_all(&root).ok();
    }

    /// No track lacks a record here, and still one is wrong: the build must look at its albums to know.
    #[test]
    fn a_build_moves_a_stray_to_the_album_it_sits_in() {
        let (root, cat) = catalogue_in("stray");
        let a = [("A/01 One.flac", "A", "One"), ("A/02 Two.flac", "A", "Two"), ("A/03 Three.flac", "A", "Three")];
        let b = [("B/01 Uno.flac", "B", "Uno"), ("B/02 Dos.flac", "B", "Dos"), ("B/03 Tres.flac", "B", "Tres")];
        add_files(&cat, &root, &a);
        add_files(&cat, &root, &b);
        let lib = Library::open(None).unwrap();
        build_now(&lib, &cat);
        let (first, second) = (record_at(&cat, "A/01 One.flac"), record_at(&cat, "B/01 Uno.flac"));
        assert_ne!(first, second);
        let track = cat.track_of(0, "A/03 Three.flac");
        std::fs::rename(root.join("A/03 Three.flac"), root.join("B/03 Three.flac")).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.tag_for_tests(0, "B/03 Three.flac", &[("album", "A"), ("albumartist", "Artist"), ("title", "Three")]);
        cat.identify().unwrap();
        assert_eq!(cat.track_of(0, "B/03 Three.flac"), track, "moved outside Cratarr, it keeps its track");
        assert!(!cat.records_unsettled());
        build_now(&lib, &cat);
        assert_eq!(record_at(&cat, "B/03 Three.flac"), second, "its only file is in that record's album");
        assert_eq!(build_now(&lib, &cat), (false, 0), "built with it");
        std::fs::remove_dir_all(&root).ok();
    }

    /// The library's own grouping is what gets a record: tracks in disc and number order, whatever the paths say.
    #[test]
    fn an_album_goes_to_its_record_in_order() {
        let mut one = row(1, "A/2-01 Late.flac", &[("album", "Album"), ("albumartist", "Artist"), ("title", "Late"), ("discnumber", "2"), ("tracknumber", "1"), ("date", "2001-02-03")]);
        let mut two = row(2, "A/1-02 Early.flac", &[("album", "Album"), ("albumartist", "Artist"), ("title", "Early"), ("discnumber", "1"), ("tracknumber", "2"), ("musicbrainz_albumid", "ABC")]);
        one.duration_ms = Some(100_000);
        two.duration_ms = Some(200_000);
        let folders = [FolderRows { folder: 0, root: "/music".into(), rows: vec![one, two] }];
        let copies = album_copies(&folders, &Rules::default());
        let [album] = &copies[..] else { panic!("one album: {copies:?}") };
        assert_eq!(album.tracks, [(2, Some(1), Some(2)), (1, Some(2), Some(1))]);
        assert_eq!((album.title.as_str(), album.artist.as_str(), album.year), ("Album", "Artist", Some(2001)));
        assert_eq!(album.facts.release.as_deref(), Some("abc"));
        assert_eq!(album.facts.tracks[0].title, "early");
        assert_eq!(album.facts.tracks[0].duration_ms, 200_000);
    }

    // ---- The flip: an album is a record, whatever copies it has.

    /// A copy of a record: two tracks numbered from `first`, in `dir`, with the same tags as any other copy's.
    fn copy_rows(first: i64, dir: &str, record: i64, format: &str, kbps: u32) -> Vec<ServedRow> {
        let lossless = format == "FLAC";
        [("One", "01"), ("Two", "02")]
            .into_iter()
            .zip(first..)
            .map(|((title, n), track)| ServedRow {
                format: Some(format.into()),
                kbps: Some(kbps),
                bits: lossless.then_some(16),
                rate: Some(44_100),
                record: Some(record),
                came: Some(1_800_000_000 + track),
                ..row(track, &format!("{dir}/{n} {title}.{}", format.to_lowercase()), &[("title", title), ("artist", "Artist"), ("albumartist", "Artist"), ("album", "Album"), ("tracknumber", n)])
            })
            .collect()
    }

    fn in_folder(folder: i64, rows: Vec<ServedRow>) -> FolderRows {
        FolderRows { folder, root: format!("/library{folder}"), rows }
    }

    /// `rows` as a copy's after the take: each file's track has another file.
    fn shared(rows: Vec<ServedRow>) -> Vec<ServedRow> {
        rows.into_iter().map(|r| ServedRow { shared: true, ..r }).collect()
    }

    /// Music's MP3 and a second main library's FLAC of record 5, on one set of tracks.
    fn two_copies() -> Vec<FolderRows> {
        vec![in_folder(0, shared(copy_rows(1, "A/Album", 5, "MP3", 192))), in_folder(1, shared(copy_rows(1, "A/Album", 5, "FLAC", 900)))]
    }

    const TWO_KEPT: [(i64, Kind); 2] = [(0, Kind::Main), (1, Kind::Main)];

    fn built_with(kinds: &[(i64, Kind)], folders: &[FolderRows]) -> Library {
        let lib = Library::open(None).unwrap();
        lib.set_kinds(kinds.to_vec());
        lib.build(folders, &Rules::default()).unwrap();
        lib
    }

    fn one_number(lib: &Library, sql: &str) -> i64 {
        lib.db().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    /// `same_as_whole` where it matters what each folder's library is.
    fn same_as_whole_in(kinds: &[(i64, Kind)], before: &[FolderRows], after: &[FolderRows]) -> usize {
        let rules = Rules::default();
        let step = built_with(kinds, before);
        step.credit_in_tests(after, &rules);
        let n = step.update_rows(&with_records(after, &rules), &rules, &Covers::new()).unwrap();
        let whole = built_with(kinds, after);
        let (a, b) = (dump(&step), dump(&whole));
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x, y, "built again where it changed, against built whole");
        }
        assert_eq!(a.len(), b.len());
        n
    }

    /// `n` is the library's order: `all_song_ids`, a server's listing and the radio proof's accounts all read it. It follows
    /// the rows, never how record numbers sort as text.
    #[test]
    fn songs_are_numbered_in_the_order_of_the_rows_whatever_their_records() {
        // Record 9's rows first: as text "al-10" sorts before "al-9", so numbered by record these would swap.
        let mut rows = [copy_rows(1, "A/Album", 9, "FLAC", 900), copy_rows(3, "B/Album", 10, "FLAC", 900)].concat();
        rows[2..].iter_mut().for_each(|r| tag(r, "album", "Other"));
        let lib = built_with(&[(0, Kind::Main)], &[in_folder(0, rows)]);
        assert_eq!(lib.all_song_ids().unwrap(), ["tr-1", "tr-2", "tr-3", "tr-4"]);
    }

    /// A retag can put two copies of one folder in one group, so two files of one track reach one group, and `songs.id` must
    /// hold for any input: a failed insert here stops the library updating for good.
    #[test]
    fn two_files_of_one_track_make_one_song() {
        let mut once = copy_rows(1, "A/Album", 5, "FLAC", 900);
        let mut twice = once.clone();
        twice.push(ServedRow { rel: "A/Album/01 One (again).flac".into(), ..once[0].clone() });
        let lib = built_with(&[(0, Kind::Main)], &[in_folder(0, twice.clone())]);
        assert_eq!(lib.counts().unwrap().0, 2, "two tracks, one of them with two files");
        assert_eq!(one_number(&lib, "SELECT count(*) FROM copy_files"), 3, "every file is built");
        assert_eq!(one_number(&lib, "SELECT tracks FROM copies"), 2);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM song_artists WHERE song_id = 'tr-1'"), 1);
        assert_eq!(lib.file_of("tr-1").unwrap(), Some((0, "A/Album/01 One (again).flac".to_string())), "the first by path, however they came");
        twice.reverse();
        assert_eq!(built_with(&[(0, Kind::Main)], &[in_folder(0, twice.clone())]).file_of("tr-1").unwrap(), lib.file_of("tr-1").unwrap());
        same_as_whole_in(&[(0, Kind::Main)], &[in_folder(0, once.clone())], &[in_folder(0, twice.clone())]);
        same_as_whole_in(&[(0, Kind::Main)], &[in_folder(0, twice)], &[in_folder(0, once.clone())]);
        // Over two folders of one group, as a copy over CD1 and CD2 can be.
        once[1].rel = "A/Album/CD2/02 Two.flac".into();
        once.push(ServedRow { rel: "A/Album/CD1/01 One.flac".into(), ..once[0].clone() });
        once[0].rel = "A/Album/CD2/01 One.flac".into();
        assert_eq!(built_with(&[(0, Kind::Main)], &[in_folder(0, once)]).counts().unwrap().0, 2);
    }

    /// A library's list says each copy as it is there, and goes by when that copy came: a new copy of an old record is news
    /// in Incoming, and with a limit it would otherwise be the first to fall off.
    #[test]
    fn a_library_lists_its_own_copy_of_a_record_by_when_it_came() {
        let mut old = copy_rows(1, "A/Album", 5, "MP3", 192);
        old.iter_mut().for_each(|r| r.born = Some(100));
        let mut other = copy_rows(21, "B/Other", 6, "FLAC", 900);
        other.iter_mut().for_each(|r| {
            r.born = Some(5_000);
            r.came = Some(5_000);
            tag(r, "album", "Other");
        });
        let mut new = copy_rows(1, "A/Album", 5, "FLAC", 900);
        new.iter_mut().for_each(|r| {
            r.born = Some(100);
            r.came = Some(9_000);
        });
        let lib = built_with(&[(0, Kind::Main), (1, Kind::Incoming)], &[in_folder(0, old), in_folder(1, [new, other].concat())]);
        let listed = lib.albums_in(1, 1).unwrap();
        assert_eq!(listed.len(), 1);
        let c = &listed[0];
        assert_eq!((c.album.id.as_str(), c.dir.as_str(), c.format.as_deref(), c.kbps, c.bits, c.rate, c.added), ("al-5", "A/Album", Some("FLAC"), Some(900), Some(16), Some(44_100), Some(9_000)));
        assert_eq!(c.size, 40_000_000, "its two files' bytes");
        assert_eq!(lib.albums_in(0, 10).unwrap()[0].format.as_deref(), Some("MP3"), "the other library's is its own");
    }

    /// Copies hold one set of tracks, so a record shows them once, each from the copy that plays.
    #[test]
    fn two_copies_are_one_album_with_one_set_of_songs() {
        let lib = built_with(&TWO_KEPT, &two_copies());
        assert_eq!(lib.counts().unwrap().0, 2);
        assert_eq!(lib.counts().unwrap().1, 1);
        assert_eq!(one_number(&lib, "SELECT record FROM albums"), 5);
        let album = lib.album("al-5").unwrap().expect("the record's id");
        assert_eq!(album.song_count, 2);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM copy_files"), 4);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM copies"), 2);
        let (format, kbps): (String, i64) = lib.db().query_row("SELECT format, min_kbps FROM albums", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((format.as_str(), kbps), ("FLAC", 900), "the copy that plays");
        assert_eq!(lib.file_of("tr-1").unwrap(), Some((1, "A/Album/01 One.flac".to_string())), "the row copy's file: `file_for` falls back on it");
        assert_eq!(lib.albums_of_files(&[(0, "A/Album/01 One.mp3".to_string())]).unwrap(), ["al-5"], "and the other copy's file says its record");
        let (folder, dir, _, first) = lib.album_art("al-5").unwrap().unwrap();
        assert_eq!((folder, dir.as_str(), first.as_deref()), (1, "A/Album", Some("A/Album/01 One.flac")));
    }

    /// A track the copy that plays lacks is listed all the same, from the next copy that holds it, and a build where it changed
    /// must come to the same. `albums`' row stays that of the copy that plays, whichever file sorts first: a path from another
    /// library joined to its folder finds no cover, and its tags would name the record. Each copy lacks a track here, as a whole one
    /// would be the one that plays.
    #[test]
    fn a_record_lists_every_track_the_row_copys_file_first() {
        let mut lacking = two_copies();
        lacking[0].rows.remove(1);
        lacking[1].rows.remove(0);
        lacking[1].rows.iter_mut().for_each(|r| tag(r, "album", "Album (the one that plays)"));
        let lib = built_with(&TWO_KEPT, &lacking);
        assert_eq!(lib.file_of("tr-2").unwrap(), Some((1, "A/Album/02 Two.flac".to_string())), "the row copy's");
        assert_eq!(lib.file_of("tr-1").unwrap(), Some((0, "A/Album/01 One.mp3".to_string())), "the other copy's, as the row copy lacks it");
        assert_eq!(lib.album("al-5").unwrap().expect("one album").song_count, 2);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM copy_files"), 2);
        let name: String = lib.db().query_row("SELECT name FROM albums", [], |r| r.get(0)).unwrap();
        assert_eq!(name, "Album (the one that plays)", "though the other copy's file sorts first");
        let ranked: i64 = lib.db().query_row("SELECT count(*) FROM copies WHERE tracks = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(ranked, 2, "each copy counts what it holds");
        same_as_whole_in(&TWO_KEPT, &two_copies(), &lacking);
        same_as_whole_in(&TWO_KEPT, &lacking, &two_copies());
    }

    /// A record's picture is found by the copy that plays, its print made from that copy's first file. Two copies can lie in one
    /// folder, and `songs` lists a track the row copy lacks from the other: that file sorts first, and is not its picture. The other
    /// lacks one too, as a whole copy would be the one that plays.
    #[test]
    fn a_records_art_is_its_row_copys_though_the_other_lies_beside_it() {
        let (mut plain, mut better) = (copy_rows(1, "B/Other", 5, "MP3", 192), copy_rows(1, "C/Row", 5, "FLAC", 900));
        plain.iter_mut().for_each(|r| tag(r, "date", "1999"));
        plain.remove(1);
        better.remove(0);
        let lib = built_with(&[(0, Kind::Main)], &[in_folder(0, [shared(plain), shared(better)].concat())]);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM copies"), 2, "two copies in one library, or this proves nothing");
        assert_eq!(lib.file_of("tr-1").unwrap(), Some((0, "B/Other/01 One.mp3".to_string())), "the track the row copy lacks");
        let (folder, dir, _, first) = lib.album_art("al-5").unwrap().unwrap();
        assert_eq!((folder, dir.as_str(), first.as_deref()), (0, "C/Row", Some("C/Row/02 Two.flac")));
        // A copy at the top of its library has no folder to go by.
        let lib = built_with(&[(0, Kind::Main)], &[in_folder(0, copy_rows(1, "x", 5, "FLAC", 900).into_iter().map(|r| ServedRow { rel: r.rel.trim_start_matches("x/").to_string(), ..r }).collect())]);
        assert_eq!(lib.album_art("al-5").unwrap().unwrap().3.as_deref(), Some("01 One.flac"));
    }

    /// Copies share their tracks, so two songs of one persistent id are two tracks tagged alike. Going by the first folder a
    /// favourite would move to the other at the next build, whichever record holds it.
    #[test]
    fn a_persistent_id_on_two_tracks_says_neither() {
        let lib = built_with(&TWO_KEPT, &[in_folder(0, copy_rows(1, "A/Album", 5, "FLAC", 900)), in_folder(1, copy_rows(3, "A/Album", 6, "FLAC", 900))]);
        assert_eq!(lib.counts().unwrap().0, 4, "two records, or this proves nothing");
        assert_eq!(lib.pids().unwrap(), Vec::new());
        assert_eq!(built_with(&TWO_KEPT, &two_copies()).pids().unwrap().len(), 2, "two files of one track are one song");
    }

    /// An Incoming copy plays only when named, though it is the better, and it is no album of its own beside the kept one.
    #[test]
    fn an_incoming_copy_of_a_kept_record_is_a_copy_not_an_album() {
        let kinds = [(0, Kind::Main), (1, Kind::Incoming)];
        let lib = built_with(&kinds, &two_copies());
        assert_eq!(lib.counts().unwrap().1, 1);
        assert_eq!(lib.file_of("tr-1").unwrap(), Some((0, "A/Album/01 One.mp3".to_string())));
        assert_eq!(lib.incoming_albums().unwrap().iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["al-5"], "it waits to be looked at");
        assert!(!lib.in_incoming("al-5").unwrap(), "the record is kept: not \"unreviewed\"");
        assert_eq!(lib.album_in(1, "A/Album").unwrap().as_deref(), Some("al-5"));
        let only = built_with(&kinds, &[in_folder(1, copy_rows(11, "A/Album", 5, "FLAC", 900))]);
        assert!(only.in_incoming("al-5").unwrap(), "nothing else holds it");
        assert_eq!(only.file_of("tr-11").unwrap(), Some((1, "A/Album/01 One.flac".to_string())), "its one copy plays");
    }

    #[test]
    fn a_cd1_cd2_album_stays_one_copy() {
        let discs = |first: i64, record: i64| {
            let mut rows = copy_rows(first, "A/Album", record, "FLAC", 900);
            rows[0].rel = "A/Album/CD1/01 One.flac".into();
            rows[1].rel = "A/Album/CD2/02 Two.flac".into();
            rows
        };
        let lib = built_with(&[(0, Kind::Main)], &[in_folder(0, discs(1, 5))]);
        let dirs: Vec<(String, i64)> = lib.db().prepare("SELECT dir, tracks FROM copies").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
        assert_eq!(dirs, [("A/Album".to_string(), 2)]);
        assert_eq!(one_number(&lib, "SELECT count(DISTINCT dir) FROM copy_files"), 2);
        let lib = built_with(&[(0, Kind::Main), (1, Kind::Main)], &[in_folder(0, discs(1, 5)), in_folder(1, copy_rows(1, "Elsewhere", 5, "MP3", 192))]);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM copies"), 2, "and a second copy beside it");
        assert_eq!(lib.counts().unwrap().0, 2);
    }

    /// A copy's key is (album, folder, dir), and a group's `dir` is only what its files share: two groups of one record can
    /// meet at one, where each was its own album before the flip.
    #[test]
    fn two_groups_of_one_record_at_one_place_are_one_copy() {
        let dated = |track: i64, rel: &str, date: &str| ServedRow {
            record: Some(5),
            ..row(track, rel, &[("title", "T"), ("artist", "Artist"), ("albumartist", "Artist"), ("album", "Album"), ("date", date)])
        };
        let rows = vec![dated(1, "P/a/1.flac", "2001"), dated(2, "P/b/2.flac", "2001"), dated(3, "P/c/3.flac", "2002"), dated(4, "P/d/4.flac", "2002")];
        let lib = built_with(&[(0, Kind::Main)], &[in_folder(0, rows)]);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM copies"), 1);
        assert_eq!(one_number(&lib, "SELECT tracks FROM copies"), 4);
        assert_eq!(lib.counts().unwrap().0, 4);
    }

    /// Which copy plays can change with any of them, so a record goes whole: built where it changed must still be what a
    /// whole build gives.
    #[test]
    fn a_record_of_two_copies_is_built_again_as_a_whole_build_would() {
        let retagged = |mut f: Vec<FolderRows>, folder: usize| {
            tag(&mut f[folder].rows[0], "genre", "Dub");
            f
        };
        assert_eq!(same_as_whole_in(&TWO_KEPT, &two_copies(), &two_copies()), 0);
        assert_eq!(same_as_whole_in(&TWO_KEPT, &two_copies(), &retagged(two_copies(), 0)), 2, "a copy that does not play: both are built");
        assert_eq!(same_as_whole_in(&TWO_KEPT, &two_copies(), &retagged(two_copies(), 1)), 2, "the copy that plays");
        // A better copy comes, and the other goes: what plays, and with it every song, changes hands.
        let first = two_copies().swap_remove(0);
        let alone = || vec![in_folder(0, first.rows.clone())];
        assert_eq!(same_as_whole_in(&TWO_KEPT, &alone(), &two_copies()), 2);
        let second = two_copies().swap_remove(1);
        assert_eq!(same_as_whole_in(&TWO_KEPT, &two_copies(), &alone()), 2, "the copy that played goes");
        assert_eq!(same_as_whole_in(&TWO_KEPT, &two_copies(), &[in_folder(1, second.rows.clone())]), 2, "the one that did not");
        // One file of the copy that plays goes, and a copy with a track fewer is still the better.
        let mut short = two_copies();
        short[1].rows.pop();
        assert_eq!(same_as_whole_in(&TWO_KEPT, &two_copies(), &short), 2);
    }

    /// CR1: copies A and B share a track, and B's file is moved into another album's folder outside Cratarr. The album's vote
    /// gives that file another record, so two records hold the track, and a write keyed by record writes it twice.
    #[test]
    fn one_track_in_two_records_is_written_once() {
        let kinds = [(0, Kind::Main)];
        let (a, mut b) = (copy_rows(1, "Z/First", 1, "FLAC", 900), copy_rows(3, "B/Second", 2, "FLAC", 900));
        // Tags of its own, or the two folders are one group, one album, and no track lies in two.
        b.iter_mut().for_each(|r| tag(r, "album", "Second"));
        let moved = || ServedRow { rel: "B/Second/09 One.flac".into(), ..a[0].clone() };
        let apart = || vec![in_folder(0, [a.clone(), b.clone()].concat())];
        let shared = || vec![in_folder(0, [a.clone(), b.clone(), vec![moved()]].concat())];
        let lib = built_with(&kinds, &shared());
        assert_eq!(one_number(&lib, "SELECT count(*) FROM songs WHERE id = 'tr-1'"), 1);
        assert_eq!(lib.albums_of(&["tr-1".to_string()]).unwrap()["tr-1"], "al-1", "in the record it holds");
        assert_eq!(lib.file_of("tr-1").unwrap(), Some((0, "Z/First/01 One.flac".to_string())));
        assert_eq!(one_number(&lib, "SELECT count(*) FROM albums"), 2, "two records, or this proves nothing");
        assert_eq!(lib.album("al-2").unwrap().unwrap().song_count, 2, "not a third song in the other");
        assert_eq!(one_number(&lib, "SELECT count(*) FROM copy_files WHERE song = 'tr-1'"), 2, "both files are built");
        let who = crate::serve::listener::Listener::default();
        let files = [(0, "B/Second/09 One.flac".to_string()), (0, "Z/First/01 One.flac".to_string())];
        assert_eq!(lib.file_for("tr-1", &files, &who).unwrap(), Some(files[1].clone()), "its own record's copy, though the other file sorts first");

        // Built where it changed: the other record lies outside what changed, and only the track reaches it.
        assert_eq!(same_as_whole_in(&kinds, &apart(), &shared()), 2);
        let retagged = |mut f: Vec<FolderRows>, at: usize| {
            tag(&mut f[0].rows[at], "genre", "Dub");
            f
        };
        assert_eq!(same_as_whole_in(&kinds, &shared(), &retagged(shared(), 3)), 2, "a file of the album it was moved into");
        assert_eq!(same_as_whole_in(&kinds, &shared(), &retagged(shared(), 1)), 2, "a file of its own record");
        assert_eq!(same_as_whole_in(&kinds, &shared(), &apart()), 2, "the moved file gone");
        // Its own record's file gone: the song goes over to the file left, in the album it lies in.
        let left = || {
            let mut f = shared();
            f[0].rows.remove(0);
            f
        };
        assert_eq!(same_as_whole_in(&kinds, &shared(), &left()), 2);
        assert_eq!(built_with(&kinds, &left()).albums_of(&["tr-1".to_string()]).unwrap()["tr-1"], "al-2");
    }

    /// The same through the catalogue: a real build, built where it changed, where a failed insert would stop `Serve::refresh`.
    #[test]
    fn a_track_of_two_albums_builds_and_settles() {
        let (root, cat) = catalogue_in("two-albums");
        add_files(&cat, &root, &[("A/01 One.flac", "A", "One"), ("A/02 Two.flac", "A", "Two"), ("A/03 Three.flac", "A", "Three")]);
        add_files(&cat, &root, &[("B/01 Uno.flac", "B", "Uno"), ("B/02 Dos.flac", "B", "Dos"), ("B/03 Tres.flac", "B", "Tres")]);
        let lib = Library::open(None).unwrap();
        build_now(&lib, &cat);
        let (first, second) = (record_at(&cat, "A/01 One.flac"), record_at(&cat, "B/01 Uno.flac"));
        assert_ne!(first, second);
        let three = cat.track_of(0, "A/03 Three.flac");
        cat.set_tracks(&[((0, "B/03 Tres.flac".to_string()), three)]).unwrap();
        assert_eq!(build_now(&lib, &cat), (false, 2), "built where it changed, both albums");
        assert_eq!(lib.counts().unwrap().0, 5);
        assert_eq!((record_at(&cat, "A/03 Three.flac"), record_at(&cat, "B/03 Tres.flac")), (first, first), "a shared track stays with its record");
        assert_eq!(lib.file_of(&ids::song(three.unwrap())).unwrap(), Some((0, "A/03 Three.flac".to_string())));
        assert_eq!(build_now(&lib, &cat), (false, 0), "and it settles");
        std::fs::remove_dir_all(&root).ok();
    }

    /// A row with no record is not served, or an album would be built under an id nothing keeps. Every track of one album
    /// shared and none holding a record: nothing else would ever give them one, and the album would never show.
    #[test]
    fn two_files_of_one_track_with_no_record_yet_are_served() {
        let (root, cat) = catalogue_in("no-record");
        add_files(&cat, &root, &[("A/01 One.flac", "A", "One"), ("A/01 Again.flac", "A", "One")]);
        cat.set_tracks(&[((0, "A/01 Again.flac".to_string()), cat.track_of(0, "A/01 One.flac"))]).unwrap();
        let lib = Library::open(None).unwrap();
        build_now(&lib, &cat);
        assert!(record_at(&cat, "A/01 One.flac").is_some());
        assert_eq!(lib.counts().unwrap().0, 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_row_with_no_record_is_not_served() {
        let rules = Rules::default();
        let lib = Library::open(None).unwrap();
        lib.build_with(&sample(), &rules, &Covers::new()).unwrap();
        assert_eq!(lib.counts().unwrap(), (0, 0, 0));
        // One of a folder's files has none yet: it had its say in the grouping, and it waits. Nothing to build again for it.
        let mut rows = copy_rows(1, "A/Album", 5, "FLAC", 900);
        rows[1].record = None;
        let folders = [in_folder(0, rows)];
        lib.credit_in_tests(&folders, &rules);
        lib.build_with(&folders, &rules, &Covers::new()).unwrap();
        assert_eq!(lib.counts().unwrap().0, 1);
        assert_eq!(one_number(&lib, "SELECT count(*) FROM copy_files"), 1);
        assert_eq!(lib.update_rows(&folders, &rules, &Covers::new()).unwrap(), 0);
    }

    /// `update_rows` once looked at each copy with a print made from the one that plays. What shows of a record's cover is
    /// the copy that plays, so only its files count.
    #[test]
    fn a_cover_changed_in_a_copy_that_does_not_play_builds_nothing() {
        let rules = Rules::default();
        let folders = with_records(&two_copies(), &rules);
        let covers = |at: i64, print: &str| Covers::from([((at, "A/Album".to_string()), print.to_string())]);
        let lib = Library::open(None).unwrap();
        lib.set_kinds(TWO_KEPT.to_vec());
        lib.credit_in_tests(&folders, &rules);
        lib.build_with(&folders, &rules, &covers(1, "a")).unwrap();
        assert_eq!(lib.update_rows(&folders, &rules, &covers(1, "a")).unwrap(), 0);
        let both = Covers::from([((0, "A/Album".to_string()), "b".to_string()), ((1, "A/Album".to_string()), "a".to_string())]);
        assert_eq!(lib.update_rows(&folders, &rules, &both).unwrap(), 0, "the other copy's");
        assert_eq!(lib.update_rows(&folders, &rules, &covers(1, "c")).unwrap(), 2, "the one that plays: the record goes whole");
    }

    /// A fact is the record's: the first start moved what was kept under a hash, and a row it could not move is no fact of
    /// any album, or two albums could read one.
    #[test]
    fn a_fact_is_found_under_the_records_id_and_nowhere_else() {
        let genreless = || {
            let mut rows = copy_rows(1, "A/Album", 5, "FLAC", 900);
            rows.iter_mut().for_each(|r| r.tags.retain(|(n, _)| n != "genre"));
            vec![in_folder(0, rows)]
        };
        let genres = |key: String| {
            let lib = Library::open(None).unwrap();
            lib.set_extra_genres(HashMap::from([(key, vec!["Dub".to_string()])]));
            lib.build(&genreless(), &Rules::default()).unwrap();
            let genres: String = lib.db().query_row("SELECT genres FROM albums", [], |r| r.get(0)).unwrap();
            genres
        };
        assert_eq!(genres("al-5".to_string()), "Dub");
        let rows = genreless();
        let hash = ids::album(&rows[0].rows[0], "Artist", 0);
        assert_eq!(genres(hash), "", "the hash it had");
    }

    /// A record must not read as lacking what one of its copies has: staging's Trym has no year on the copy a list could show.
    #[test]
    fn a_record_takes_what_its_copy_that_plays_lacks_from_another() {
        let mut plain = copy_rows(1, "A/Album", 5, "MP3", 192);
        plain.iter_mut().for_each(|r| r.tags.retain(|(n, _)| n != "date"));
        let mut tagged = copy_rows(1, "A/Album", 5, "MP3", 128);
        for r in &mut tagged {
            r.tags.push(("date".into(), "2023".into()));
            r.tags.push(("musicbrainz_albumid".into(), "abc".into()));
            r.tags.push(("genre".into(), "Dub".into()));
        }
        let lib = built_with(&[(0, Kind::Main), (1, Kind::Incoming)], &[in_folder(0, plain), in_folder(1, tagged)]);
        let (mbid, year, genres): (String, i64, String) = lib.db().query_row("SELECT mbid, year, genres FROM albums", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!((mbid.as_str(), year, genres.as_str()), ("abc", 2023, "Dub"));
    }

    /// 12 albums on the real library have the date `0001-01-01`: L.A. Woman says 1971 in its original date, and the year 1
    /// beat it.
    #[test]
    fn a_placeholder_date_is_no_year() {
        assert_eq!(year_of("0001-01-01"), None);
        assert_eq!(year_of("0000"), None);
        assert_eq!(year_of("1971-04-19"), Some(1971));
        let mut rows = copy_rows(1, "A/Album", 5, "FLAC", 900);
        for r in &mut rows {
            r.tags.push(("date".into(), "0001-01-01".into()));
            r.tags.push(("originaldate".into(), "1971".into()));
        }
        let folders = [in_folder(0, rows)];
        assert_eq!(album_copies(&folders, &Rules::default())[0].year, None);
        assert_eq!(one_number(&built_with(&[(0, Kind::Main)], &folders), "SELECT year FROM albums"), 1971);
    }

    /// Two folders under one album id, as staging's Stellar - Tunnel: one group, one copy at what they share, every file built.
    #[test]
    fn one_group_in_two_artists_folders_is_one_copy_at_their_root() {
        let mut rows = copy_rows(1, "x", 5, "FLAC", 900);
        rows[0].rel = "One Artist/Album/01 One.flac".into();
        rows[1].rel = "Other Artist/Album/02 Two.flac".into();
        let lib = built_with(&[(0, Kind::Main)], &[in_folder(0, rows.clone())]);
        let dir: String = lib.db().query_row("SELECT dir FROM copies", [], |r| r.get(0)).unwrap();
        assert_eq!(dir, "", "the library's root");
        assert_eq!(one_number(&lib, "SELECT count(DISTINCT dir) FROM copy_files"), 2);
        let mut changed = rows.clone();
        tag(&mut changed[1], "genre", "Dub");
        assert_eq!(same_as_whole_in(&[(0, Kind::Main)], &[in_folder(0, rows)], &[in_folder(0, changed)]), 2, "both its folders");
    }

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
            analysis: None,
            ..ServedRow::default()
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
            root: "/music".into(),
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
            .query_row("SELECT a.song_count, a.genres, c.dir, a.year, a.created FROM albums a JOIN copies c ON c.album = a.id WHERE a.name = 'Homogenic'", [], |r| {
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

    /// What a copy says of its files, and of each one apart. A file with no track yet serves nothing, so it is in neither.
    #[test]
    fn a_copy_is_one_folders_files() {
        let tags = |disc: &'static str| [("title", "t"), ("artist", "Björk"), ("albumartist", "Björk"), ("album", "Live Box"), ("discnumber", disc)];
        let mut one = row(1, "Björk/Live Box/CD1/01 Hunter.flac", &tags("1"));
        let mut two = row(2, "Björk/Live Box/CD2/01 Jóga.flac", &tags("2"));
        (one.size, one.kbps, one.came, one.bits, one.rate, one.record) = (30, Some(900), Some(200), Some(24), Some(96_000), Some(5));
        (two.size, two.kbps, two.came, two.bits, two.rate, two.record) = (20, Some(700), Some(100), Some(16), Some(44_100), Some(5));
        one.analysis = Some(crate::catalogue::RowAnalysis { track_gain_db: -4.0, track_peak: 0.5, bpm: Some(120.0), album: Some((-5.0, 0.75)) });
        let mut trackless = row(3, "Björk/Live Box/CD2/02 New.flac", &tags("2"));
        trackless.track = None;
        let lib = Library::open(None).unwrap();
        lib.refresh(&[FolderRows { folder: 4, root: "/music".into(), rows: vec![one.clone(), two.clone(), trackless] }], &Rules::default()).unwrap();
        let db = lib.db();
        let album: String = db.query_row("SELECT id FROM albums", [], |r| r.get(0)).unwrap();
        type Copy = (String, i64, String, Option<String>, Option<i64>, i64, Option<i64>, i64, Option<i64>, Option<i64>);
        let copies: Vec<Copy> = db
            .prepare("SELECT album, folder, dir, format, min_kbps, size, added, tracks, bits, rate FROM copies")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(copies, [(album.clone(), 4, "Björk/Live Box".to_string(), Some("FLAC".to_string()), Some(700), 50, Some(100), 2, Some(16), Some(44_100))], "two discs, one copy");
        type File = (String, String, String, String, i64, Option<String>, Option<i64>, Option<i64>, Option<i64>);
        let files: Vec<File> = db
            .prepare("SELECT rel, dir, song, src, size, suffix, kbps, duration_ms, bpm FROM copy_files WHERE folder = 4 AND album = ?1 ORDER BY rel")
            .unwrap()
            .query_map([&album], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let flac = || Some("flac".to_string());
        assert_eq!(files, [
            (one.rel.clone(), "Björk/Live Box/CD1".to_string(), ids::song(1), row_src(&one), 30, flac(), Some(900), Some(200_000), Some(120)),
            (two.rel.clone(), "Björk/Live Box/CD2".to_string(), ids::song(2), row_src(&two), 20, flac(), Some(700), Some(200_000), None),
        ]);
        let rg: (Option<f64>, Option<f64>, Option<f64>, Option<f64>) = db
            .query_row("SELECT rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak FROM copy_files WHERE rel = ?1", [&one.rel], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap();
        assert_eq!(rg, (Some(-4.0), Some(0.5), Some(-5.0), Some(0.75)));
    }

    /// `None` sorts first: this is the line between a copy read and one that ranks last until the reader gets there.
    #[test]
    fn one_unread_file_leaves_its_copy_unread() {
        let read = |r: ServedRow| ServedRow { bits: Some(16), rate: Some(44_100), ..r };
        let tags = [("title", "t"), ("artist", "A"), ("album", "X")];
        let (one, two) = (read(row(1, "A/X/01 t.flac", &tags)), row(2, "A/X/02 t.flac", &tags));
        let lib = Library::open(None).unwrap();
        lib.refresh(&[FolderRows { folder: 0, root: "/music".into(), rows: vec![one, two] }], &Rules::default()).unwrap();
        let copy: (Option<i64>, Option<i64>) = lib.db().query_row("SELECT bits, rate FROM copies", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(copy, (None, None));
    }

    /// The same tags in two folders with tracks of their own are two records until something says otherwise: a file of one
    /// changing is that record's alone.
    #[test]
    fn the_same_album_in_two_folders_is_built_again_where_it_changed() {
        let second = |mut f: Vec<FolderRows>| {
            let rows = f[0].rows.iter().map(|r| ServedRow { track: r.track.map(|t| t + 10), ..r.clone() }).collect();
            f.push(FolderRows { folder: 1, root: "/incoming".into(), rows });
            f
        };
        let before = second(sample());
        let lib = Library::open(None).unwrap();
        lib.build(&before, &Rules::default()).unwrap();
        let copies: i64 = lib.db().query_row("SELECT count(*) FROM copies", [], |r| r.get(0)).unwrap();
        assert_eq!(copies, 4);
        let mut after = second(sample());
        tag(&mut after[1].rows[1], "genre", "Electronic; Trip Hop");
        assert_eq!(same_as_whole(&before, &after, |_| {}), 1);
    }

    /// What the flip does to a file of a copy that does not play: its row in `copy_files` stays, it has none in `songs`.
    fn off_the_songs(lib: &Library, rel: &str) {
        let db = lib.db();
        let (n, id): (i64, String) = db.query_row("SELECT n, id FROM songs WHERE rel = ?1", [rel], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        db.execute("DELETE FROM songs_fts WHERE rowid = ?1", [n]).unwrap();
        db.execute("DELETE FROM songs_tri_fts WHERE rowid = ?1", [n]).unwrap();
        db.execute("DELETE FROM song_artists WHERE song_id = ?1", [&id]).unwrap();
        db.execute("DELETE FROM songs WHERE n = ?1", [n]).unwrap();
    }

    /// Compared with `songs`, such a file would look new at every pass, and a folder of nothing else would have nothing
    /// to clear: its album would still stand, and writing it again would break `albums.id`.
    #[test]
    fn a_file_with_no_row_in_songs_is_neither_new_nor_left_behind() {
        let rules = Rules::default();
        let lib = Library::open(None).unwrap();
        lib.build(&sample(), &rules).unwrap();
        off_the_songs(&lib, &sample()[0].rows[2].rel);
        assert_eq!(lib.update_rows(&with_records(&sample(), &rules), &rules, &Covers::new()).unwrap(), 0, "as it was built");
        let changed = with(sample(), |rows| tag(&mut rows[2], "genre", "Ambient"));
        assert_eq!(lib.update_rows(&with_records(&changed, &rules), &rules, &Covers::new()).unwrap(), 1);
        let whole = Library::open(None).unwrap();
        whole.build(&changed, &rules).unwrap();
        assert_eq!(dump(&lib), dump(&whole));
    }

    /// Once a copy holds the record's track, `songs` names the other file. The lookups by place read the file's own row,
    /// or Health's and a playlist's files in that copy would go blind.
    #[test]
    fn a_file_of_a_copy_that_does_not_play_is_found_by_its_place() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        let (rel, other) = ("Björk/Homogenic (1997)/01 Hunter.flac", "Elsewhere/01 Hunter.flac");
        lib.db()
            .execute_batch(&format!(
                "INSERT INTO copy_files SELECT 1, '{other}', 'Elsewhere', song, album, src, size, suffix, kbps, duration_ms, bpm,
                   rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak FROM copy_files WHERE rel = '{rel}';
                 INSERT INTO copies SELECT album, 1, 'Elsewhere', format, min_kbps, size, added, tracks, bits, rate, short FROM copies WHERE dir = 'Björk/Homogenic (1997)';"
            ))
            .unwrap();
        let (song, album): (String, String) = lib.db().query_row("SELECT song, album FROM copy_files WHERE rel = ?1", [other], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(song, ids::song(1));
        assert_eq!(lib.song_at(1, other).unwrap(), Some((song.clone(), album.clone(), "Hunter".to_string())));
        assert_eq!(lib.albums_of_files(&[(1, other.to_string())]).unwrap(), [album.clone()]);
        let by_rel = lib.songs_by_rel(1).unwrap();
        assert_eq!((by_rel.len(), by_rel[other].song.as_str(), by_rel[other].album.as_str()), (1, song.as_str(), album.as_str()));
        assert_eq!(by_rel[other].genres, ["Electronic", "Art Pop"], "its track's");
        assert_eq!(lib.songs_by_rel(0).unwrap().len(), 3, "the other folder's as they were");
        let (copies, files) = lib.album_arrivals().unwrap();
        assert!(copies.contains(&(album.clone(), 1, Some(1_700_000_001))), "{copies:?}");
        assert!(files.contains(&(song, album, 1, other.to_string())), "{files:?}");
    }

    /// The filters ask `copies` and `copy_files`, as `songs` and `albums` hold one place for what can lie in two. A library
    /// off takes a track away only when no file of it is left on.
    #[test]
    fn a_song_is_listed_by_any_folder_it_has_a_file_in() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        lib.db()
            .execute_batch(
                "INSERT INTO copy_files SELECT 1, 'Elsewhere/' || rel, 'Elsewhere', song, album, src, size, suffix, kbps, duration_ms, bpm,
                   rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak FROM copy_files WHERE dir = 'Björk/Homogenic (1997)';
                 INSERT INTO copies SELECT album, 1, 'Elsewhere', format, min_kbps, size, added, tracks, bits, rate, short FROM copies WHERE dir = 'Björk/Homogenic (1997)';",
            )
            .unwrap();
        let titles = |songs: &[super::super::api::Song]| {
            let mut t: Vec<String> = songs.iter().map(|s| s.title.clone()).collect();
            t.sort();
            t
        };
        let both = ["Hunter", "Jóga"];
        for folder in [Some(0), Some(1)] {
            let albums = lib.album_list(&super::super::read::AlbumOrder::ByName, 10, 0, folder).unwrap();
            assert!(albums.iter().any(|a| a.name == "Homogenic"), "{folder:?}");
            assert_eq!(titles(&lib.random_songs(10, Some("Art Pop"), (None, None), folder).unwrap()), both, "{folder:?}");
            assert_eq!(titles(&lib.songs_by_genre("Art Pop", 10, 0, folder).unwrap()), both, "{folder:?}");
            for query in ["", "Homogenic"] {
                let (_, albums, songs) = lib.search(query, (0, 0), (10, 0), (10, 0), folder).unwrap();
                assert!(albums.iter().any(|a| a.name == "Homogenic"), "{query:?} in {folder:?}");
                assert!(both.iter().all(|t| songs.iter().any(|s| s.title == *t)), "{query:?} in {folder:?}");
            }
        }
        assert_eq!(lib.album_list(&super::super::read::AlbumOrder::ByName, 10, 0, Some(1)).unwrap().len(), 1, "the other album has no copy there");
        assert!(lib.album_list(&super::super::read::AlbumOrder::ByName, 10, 0, Some(2)).unwrap().is_empty());
        assert!(lib.random_songs(10, None, (None, None), Some(2)).unwrap().is_empty());
        assert!(lib.songs_by_genre("Art Pop", 10, 0, Some(2)).unwrap().is_empty());
        let (_, albums, songs) = lib.search("Homogenic", (0, 0), (10, 0), (10, 0), Some(2)).unwrap();
        assert!(albums.is_empty() && songs.is_empty());

        let on = |off: &[i64]| {
            let mut t: Vec<String> = lib.track_rows(&[], off).unwrap().into_iter().map(|r| r.title).collect();
            t.sort();
            t
        };
        assert_eq!(on(&[0]), both, "Music off: the file in the other folder keeps it");
        assert_eq!(on(&[1]).len(), 3, "the other folder off: Music holds all three");
        assert!(on(&[0, 1]).is_empty(), "both off");

        let album: String = lib.db().query_row("SELECT id FROM albums WHERE name = 'Homogenic'", [], |r| r.get(0)).unwrap();
        let hunter = ids::song(1);
        assert_eq!(lib.song_folders(std::slice::from_ref(&hunter)).unwrap()[&hunter].iter().copied().collect::<BTreeSet<i64>>(), [0, 1].into());
        assert_eq!(lib.album_folders().unwrap()[&album].iter().copied().collect::<BTreeSet<i64>>(), [0, 1].into());
        let folders = lib.artist_albums().unwrap().into_iter().find(|r| r.3 == album).unwrap().4;
        assert_eq!(folders.into_iter().collect::<BTreeSet<i64>>(), [0, 1].into());
        assert_eq!(lib.albums_in(1, 10).unwrap().iter().map(|c| (c.album.id.clone(), c.dir.clone())).collect::<Vec<_>>(), [(album.clone(), "Elsewhere".to_string())], "the copy's own folder");
        let copies: Vec<(i64, String)> = lib.copies_of(&album).unwrap().into_iter().map(|c| (c.folder, c.dir)).collect();
        assert_eq!(copies, [(0, "Björk/Homogenic (1997)".to_string()), (1, "Elsewhere".to_string())]);
        assert_eq!(lib.album_in(1, "Elsewhere").unwrap(), Some(album));
    }

    /// `album_arrivals` gives a row per copy and per file, where Insights once had a row per album and per song. The same
    /// record in a second folder, under the ids it will share, is what Insights must still read as one album.
    #[test]
    fn insights_reads_the_arrivals_of_an_album_in_two_folders_as_one_album() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        lib.db()
            .execute_batch(
                "INSERT INTO copy_files SELECT 1, 'Elsewhere/' || rel, 'Elsewhere', song, album, src, size, suffix, kbps, duration_ms, bpm,
                   rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak FROM copy_files WHERE dir = 'Björk/Homogenic (1997)';
                 INSERT INTO copies SELECT album, 1, 'Elsewhere', format, min_kbps, size, added, tracks, bits, rate, short FROM copies WHERE dir = 'Björk/Homogenic (1997)';",
            )
            .unwrap();
        let (copies, files) = lib.album_arrivals().unwrap();
        assert_eq!((copies.len(), files.len()), (3, 5), "a row each");
        let figures = |shown: &[i64]| {
            let shown = shown.iter().copied().collect();
            let o = crate::insights::overview(
                &copies,
                &files,
                &shown,
                &Default::default(),
                &Default::default(),
                &Default::default(),
                None,
                (crate::insights::Step::Year, None),
                1_792_800_000,
                0,
            );
            (o.albums, o.tracks, o.added.iter().map(|c| c.n).sum::<usize>())
        };
        assert_eq!(figures(&[0, 1]), (2, 3, 2), "as if the other folder were not there");
        assert_eq!(figures(&[0]), (2, 3, 2));
        assert_eq!(figures(&[1]), (1, 2, 1), "Homogenic alone");
    }

    /// A library from before these tables is built again, as any of another layout is: nothing carries over to go wrong.
    #[test]
    fn a_library_of_the_layout_before_is_built_again_with_its_copies() {
        let dir = std::env::temp_dir().join(format!("cratarr-layout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("library.sqlite");
        let _ = std::fs::remove_file(&file);
        {
            let lib = Library::open(Some(file.as_path())).unwrap();
            assert!(lib.refresh(&sample(), &Rules::default()).unwrap());
            lib.db().execute_batch(&format!("DROP TABLE copies; DROP TABLE copy_files; PRAGMA user_version = {}", LAYOUT - 1)).unwrap();
        }
        let lib = Library::open(Some(file.as_path())).unwrap();
        assert_eq!(lib.counts().unwrap(), (0, 0, 0), "emptied, not read as it was");
        assert!(lib.refresh(&sample(), &Rules::default()).unwrap(), "and built");
        assert_eq!(lib.db().query_row("SELECT count(*) FROM copies", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
        drop(lib);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_songs_tagged_total_is_kept_from_its_number_or_its_total_tag() {
        // The Orb's "1/10"; Homogenic's second track with TOTALTRACKS, its first with none.
        let tagged = with(sample(), |rows| rows[1].tags.push(("totaltracks".into(), "10".into())));
        let lib = Library::open(None).unwrap();
        lib.refresh(&tagged, &Rules::default()).unwrap();
        let mut totals: Vec<Vec<u32>> = lib.track_totals().unwrap().into_values().collect();
        totals.sort();
        assert_eq!(totals, [vec![10], vec![10]]);
    }

    #[test]
    fn an_albums_length_is_its_songs_summed_and_unknown_when_one_is() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        let listed = lib.albums_in(0, 10).unwrap();
        let homogenic = listed.iter().find(|c| c.album.name == "Homogenic").unwrap();
        assert_eq!(homogenic.secs, Some(400.0), "two songs of 200 s");
        assert_eq!(lib.album_facts(&homogenic.album.id).unwrap().unwrap().3, Some(400.0));
        let untimed = with(sample(), |rows| rows[1].duration_ms = None);
        let lib = Library::open(None).unwrap();
        lib.refresh(&untimed, &Rules::default()).unwrap();
        let listed = lib.albums_in(0, 10).unwrap();
        let homogenic = listed.iter().find(|c| c.album.name == "Homogenic").unwrap();
        assert_eq!(homogenic.secs, None, "one song's length unknown: the album's is");
        assert_eq!(lib.album_facts(&homogenic.album.id).unwrap().unwrap().3, None);
        let orb = listed.iter().find(|c| c.album.name == "Adventures Beyond the Ultraworld").unwrap();
        assert_eq!(orb.secs, Some(200.0), "another album's is its own");
    }

    #[test]
    fn a_crates_ids_are_named_as_the_library_has_them_and_one_it_lacks_left_out() {
        // 0.13, decision 0088: an artist's name, an album's and a track's title.
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        let homogenic: String = lib.db().query_row("SELECT id FROM albums WHERE name = 'Homogenic'", [], |r| r.get(0)).unwrap();
        let (bjork, hunter) = (ids::artist(artist_number_in_tests("Björk")), ids::song(1));
        let asked = [bjork.clone(), homogenic.clone(), hunter.clone(), "tr-999".to_string(), "ar-nobody".to_string(), "al-none".to_string(), "Björk".to_string()];
        let names = lib.names_of(&asked).unwrap();
        assert_eq!(names.len(), 3, "{names:?}");
        assert_eq!((names[&bjork].as_str(), names[&homogenic].as_str(), names[&hunter].as_str()), ("Björk", "Homogenic", "Hunter"));
    }

    #[test]
    fn a_songs_album_is_found_by_the_songs_id() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        let all: HashMap<String, String> = {
            let db = lib.db();
            let mut st = db.prepare("SELECT id, album_id FROM songs").unwrap();
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap()
        };
        assert_eq!(all.values().collect::<std::collections::HashSet<_>>().len(), 2);
        let asked: Vec<String> = all.keys().cloned().chain(["tr-none".to_string()]).collect();
        assert_eq!(lib.albums_of(&asked).unwrap(), all, "each song with its album; one not in the library left out");
    }

    #[test]
    fn gains_come_from_the_analysis_once_there_else_from_the_files_tags() {
        let tagged = row(1, "A/X/01 t.flac", &[("replaygain_track_gain", "-6.53 dB"), ("replaygain_track_peak", "0.988123"), ("replaygain_album_gain", "-7 dB")]);
        assert_eq!(gains(&tagged), Gains { track_gain: Some(-6.53), track_peak: Some(0.988123), album_gain: Some(-7.0), album_peak: None });
        // Opus counts from -23 LUFS in 1/256 dB: 5 dB more for ReplayGain's -18.
        let opus = row(2, "A/X/02 t.opus", &[("r128_track_gain", "-512")]);
        assert_eq!(gains(&opus).track_gain, Some(3.0));
        let mut analysed = tagged.clone();
        analysed.analysis = Some(crate::catalogue::RowAnalysis { track_gain_db: -4.0, track_peak: 0.5, bpm: Some(127.6), album: None });
        let g = gains(&analysed);
        assert_eq!((g.track_gain, g.track_peak, g.album_gain), (Some(-4.0), Some(0.5), Some(-7.0)), "the album's waits for all its tracks");
        analysed.analysis.as_mut().unwrap().album = Some((-5.0, 0.9));
        assert_eq!((gains(&analysed).album_gain, gains(&analysed).album_peak), (Some(-5.0), Some(0.8999999761581421)));
        // Tempo: the file's own tag first, then the analysis'.
        assert_eq!(bpm_of(&analysed), Some(128));
        let mut tagged_bpm = analysed.clone();
        tagged_bpm.tags.push(("bpm".into(), "90".into()));
        assert_eq!(bpm_of(&tagged_bpm), Some(90));
    }

    #[test]
    fn a_song_carries_its_replay_gain() {
        let lib = Library::open(None).unwrap();
        let mut r = row(1, "A/X (2001)/01 t.flac", &[("title", "t"), ("artist", "A"), ("album", "X"), ("replaygain_track_gain", "-6.5 dB")]);
        r.analysis = Some(crate::catalogue::RowAnalysis { track_gain_db: -4.25, track_peak: 0.75, bpm: Some(120.0), album: Some((-5.0, 0.9)) });
        lib.refresh(&[FolderRows { folder: 0, root: "/music".into(), rows: vec![r] }], &Rules::default()).unwrap();
        let id = crate::serve::ids::song(1);
        let song = lib.songs_by_id(&[id]).unwrap().pop().unwrap();
        let rg = song.replay_gain.unwrap();
        assert_eq!((rg.track_gain, rg.track_peak, rg.album_gain), (Some(-4.25), Some(0.75), Some(-5.0)));
        assert_eq!(song.bpm, Some(120));
        let json = serde_json::to_value(lib.songs_by_id(&[crate::serve::ids::song(1)]).unwrap().pop().unwrap()).unwrap();
        assert_eq!(json["replayGain"]["trackGain"], -4.25);
    }

    #[test]
    fn an_artists_work_is_their_albums_newest_first_then_what_they_appear_on() {
        let lib = Library::open(None).unwrap();
        let t = |n: i64, rel: &str, artist: &str, album_artist: &str, album: &str, date: &str| {
            let title = format!("t{n}");
            row(n, rel, &[("title", &title), ("artist", artist), ("albumartist", album_artist), ("album", album), ("tracknumber", &n.to_string()), ("date", date)])
        };
        let folders = vec![FolderRows {
            folder: 0,
            root: "/music".into(),
            rows: vec![
                t(1, "A/Old (2001)/01 t.flac", "A", "A", "Old", "2001"),
                t(2, "A/New (2010)/01 t.flac", "A", "A", "New", "2010"),
                t(3, "A/New (2010)/02 t.flac", "C", "A", "New", "2010"),
                t(4, "B/Mix (2005)/01 t.flac", "B", "B", "Mix", "2005"),
                t(5, "B/Mix (2005)/02 t.flac", "A", "B", "Mix", "2005"),
            ],
        }];
        lib.refresh(&folders, &Rules::default()).unwrap();
        let (own, on, songs) = lib.artist_works(&ids::artist(artist_number_in_tests("a"))).unwrap();
        let names = |l: &[crate::serve::api::Album]| l.iter().map(|a| a.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&own), ["New", "Old"]);
        assert_eq!(names(&on), ["Mix"], "credited on a track only");
        let titles: Vec<&str> = songs.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["t2", "t3", "t5", "t1"], "every track of their albums, and theirs on others', album by album");
    }

    #[test]
    fn a_folder_is_one_album_even_when_its_tags_disagree() {
        let lib = Library::open(None).unwrap();
        let t = |n: i64, date: &str| row(n, &format!("A/X (2013)/0{n} t.flac"), &[("title", "t"), ("artist", "A"), ("album", "X"), ("date", date)]);
        let folders = vec![FolderRows { folder: 0, root: "/music".into(), rows: vec![t(1, "2013"), t(2, "2013"), t(3, "2015")] }];
        lib.refresh(&folders, &Rules::default()).unwrap();
        assert_eq!(lib.counts().unwrap().1, 1);
        let year: i64 = lib.db().query_row("SELECT year FROM albums", [], |r| r.get(0)).unwrap();
        assert_eq!(year, 2015, "the album's aggregates still count every song");
    }

    #[test]
    fn an_album_shows_its_original_year_from_its_files_else_musicbrainz() {
        let album = |t: i64, name: &str, tags: &[(&str, &str)]| {
            let mut all = vec![("title", "A"), ("artist", "Ott"), ("albumartist", "Ott"), ("album", name)];
            all.extend_from_slice(tags);
            row(t, &format!("Ott/{name}/01 A.flac"), &all)
        };
        let folders = || {
            vec![FolderRows {
                folder: 0,
                root: "/music".into(),
                rows: vec![
                    album(1, "Skylon", &[("date", "2012-09-24"), ("musicbrainz_albumid", "rel-skylon")]),
                    album(2, "Mir", &[("date", "2015"), ("originaldate", "2011-03-01"), ("musicbrainz_albumid", "rel-mir")]),
                    album(3, "Odd", &[("date", "2005"), ("musicbrainz_albumid", "rel-odd")]),
                ],
            }]
        };
        let years = |lib: &Library| -> Vec<(String, Option<i64>, Option<i64>)> {
            let db = lib.db();
            let mut st = db.prepare("SELECT name, year, original_year FROM albums ORDER BY name").unwrap();
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
        };
        let lib = Library::open(None).unwrap();
        lib.refresh(&folders(), &Rules::default()).unwrap();
        assert_eq!(years(&lib)[2], ("Skylon".into(), Some(2012), None), "a reissue's year without anything better");
        let lib = Library::open(None).unwrap();
        lib.set_originals([("rel-skylon".into(), 2008), ("rel-mir".into(), 1999), ("rel-odd".into(), 2009)].into());
        lib.refresh(&folders(), &Rules::default()).unwrap();
        assert_eq!(
            years(&lib),
            [("Mir".into(), Some(2011), Some(2011)), ("Odd".into(), Some(2005), Some(2009)), ("Skylon".into(), Some(2008), Some(2008))],
            "the files' original date first; MusicBrainz's where they have none; never later than the release's"
        );
        // Release types that come after a build build it again (0.9.6: on the sim they never showed).
        let before = lib.originals_print();
        lib.set_release_types([("mb:rel-odd".into(), (Some("ep".into()), vec!["live".into()]))].into());
        assert_ne!(lib.originals_print(), before, "new types change what the library was built from (`change`)");
        let odd: String = lib.db().query_row("SELECT id FROM albums WHERE name = 'Odd'", [], |r| r.get(0)).unwrap();
        let before = lib.originals_print();
        lib.set_incomplete([(odd, Whole { tracks: 12, duration_ms: None, longest_ms: None })].into());
        assert_ne!(lib.originals_print(), before, "an album found missing tracks too");
        lib.build(&folders(), &Rules::default()).unwrap();
        let t: (Option<String>, Option<String>, String, Option<String>) = lib
            .db()
            .query_row("SELECT release_type, type_from, secondary, type_why FROM albums WHERE name = 'Odd'", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap();
        assert_eq!(t, (Some("album".into()), Some("musicbrainz".into()), "live".into(), Some("tracks".into())), "found missing tracks: typed as its twelve-track release");
    }

    #[test]
    fn facts_set_by_hand_win_over_the_files_and_the_rule() {
        let track = |t: i64| {
            let tags = [("title", "A"), ("artist", "Ott"), ("albumartist", "Ott"), ("album", "Skylon"), ("date", "2012"), ("genre", "Dub")];
            row(t, &format!("Ott/Skylon/0{t} A.flac"), &tags)
        };
        let folders = || vec![FolderRows { folder: 0, root: "/music".into(), rows: vec![track(1), track(2)] }];
        let lib = Library::open(None).unwrap();
        lib.refresh(&folders(), &Rules::default()).unwrap();
        let base: String = lib.db().query_row("SELECT id FROM albums", [], |r| r.get(0)).unwrap();
        let set = Overrides {
            kind: Some("album".into()),
            secondary: Some(vec!["live".into()]),
            year: Some(2008),
            genres: Some(vec!["Psybient".into()]),
            ..Overrides::default()
        };
        let before = lib.originals_print();
        lib.set_overrides([(base, set)].into());
        assert_ne!(lib.originals_print(), before, "what is set builds the library again");
        lib.build(&folders(), &Rules::default()).unwrap();
        let album: (String, Option<String>, Option<String>, String, i64, i64, String, String) = lib
            .db()
            .query_row("SELECT release_type, type_from, type_why, secondary, year, original_year, genres, set_fields FROM albums", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
            })
            .unwrap();
        let f = '\u{1f}';
        assert_eq!(album, ("album".into(), Some("set".into()), None, "live".into(), 2008, 2008, "Psybient".into(), format!("type{f}secondary{f}year{f}genres")));
        let songs: Vec<String> = lib.db().prepare("SELECT genres FROM songs").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!(songs, ["Psybient", "Psybient"], "its tracks' genres too");
        lib.set_overrides(HashMap::new());
        lib.build(&folders(), &Rules::default()).unwrap();
        let kind: (String, Option<String>) = lib.db().query_row("SELECT release_type, type_why FROM albums", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(kind, ("single".into(), Some("tracks".into())), "reset: as the rule finds it");
    }

    #[test]
    fn an_imported_added_date_comes_first_when_earlier() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        let id: String = lib.db().query_row("SELECT id FROM albums WHERE name = 'Homogenic'", [], |r| r.get(0)).unwrap();
        lib.set_added([(id.clone(), 5)].into());
        lib.build(&sample(), &Rules::default()).unwrap();
        let created: i64 = lib.db().query_row("SELECT created FROM albums WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
        assert_eq!(created, 5);
        lib.set_added([(id.clone(), 9_999_999_999)].into());
        lib.build(&sample(), &Rules::default()).unwrap();
        let created: i64 = lib.db().query_row("SELECT created FROM albums WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
        assert_eq!(created, 1_700_000_001, "a later one does not move it");
    }

    #[test]
    fn a_song_has_all_its_genres_and_is_found_by_each() {
        let lib = Library::open(None).unwrap();
        assert!(lib.refresh(&sample(), &Rules::default()).unwrap());
        let found = lib.songs_by_genre("Art Pop", 10, 0, None).unwrap();
        assert_eq!(found.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["Hunter", "Jóga"], "found by its second genre too");
        assert_eq!(found[0].genres.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["Electronic", "Art Pop"]);
        assert_eq!(found[0].genre.as_deref(), Some("Electronic"), "the one-genre field keeps the first");
        assert_eq!(lib.songs_by_genre("art pop", 10, 0, None).unwrap().len(), 2, "regardless of capitals");
        assert_eq!(lib.random_songs(10, Some("Art Pop"), (None, None), None).unwrap().len(), 2);
        assert!(lib.songs_by_genre("Art", 10, 0, None).unwrap().is_empty(), "a whole genre, not a part of one");
        assert_eq!(lib.songs_by_rel(0).unwrap()["Björk/Homogenic (1997)/01 Hunter.flac"].genres, ["Electronic", "Art Pop"]);
    }

    #[test]
    fn a_genre_is_found_by_its_aliases_in_search() {
        // 0.12: one matcher, genre aliases (the rules crate's hand-kept list).
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        lib.db().execute("INSERT INTO genres (name, song_count, album_count) VALUES ('Drum and Bass', 4, 1), ('Dub', 9, 2), ('R&B', 2, 1)", []).unwrap();
        let words = |q: &str| crate::search::words(q);
        let f = lib.find(&words("dnb"), 10).unwrap();
        assert_eq!((f.genres, f.totals.genres), (vec![("Drum and Bass".to_string(), 4)], 1), "by its alias, Dub not by its letters");
        assert_eq!(lib.find(&words("rnb"), 10).unwrap().genres, [("R&B".to_string(), 2)]);
        assert!(lib.find(&words("jungle"), 10).unwrap().genres.is_empty());
    }

    #[test]
    fn new_analyses_alone_are_told_apart_from_changed_songs() {
        let (first, print) = change_between(None, "catalogue 1 2 a rules f files 10/5", "3/99");
        assert_eq!(first, Change::Songs, "never built");
        assert_eq!(change_between(Some(&print), "catalogue 1 2 a rules f files 10/5", "3/99").0, Change::None);
        assert_eq!(change_between(Some(&print), "catalogue 1 2 a rules f files 10/5", "4/120").0, Change::Analyses);
        assert_eq!(change_between(Some(&print), "catalogue 1 2 a rules f files 11/6", "4/120").0, Change::Songs);
        assert_eq!(change_between(Some("catalogue 1 2 a rules f added 10/5 analysis 3/99"), "catalogue 1 2 a rules f files 10/5", "3/99").0, Change::Songs, "a print from before");
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
    fn a_search_for_more_than_the_index_is_widened_to_finds_them_all() {
        // Show all's fifth page of 50 asks for 250: past `WIDEST`, where the release build's panic stopped the server.
        let lib = Library::open(None).unwrap();
        let cloud = |n: i64| row(n, &format!("The Orb/Adventures (1991)/{n:03} Cloud.flac"), &[("title", &format!("Cloud {n}")), ("artist", "The Orb"), ("album", "Adventures"), ("tracknumber", &n.to_string())]);
        lib.refresh(&[FolderRows { folder: 0, root: "/music".into(), rows: (1..=260).map(cloud).collect() }], &Rules::default()).unwrap();
        let f = lib.find(&crate::search::words("cloud"), 250).unwrap();
        assert_eq!((f.tracks.len(), f.totals.tracks), (250, 260));
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
        drop(db);
        // Each kind counted in all, beyond the one shown: an album by its own name or by a track's title, an artist by
        // their name or an album's.
        let words = |q: &str| crate::search::words(q);
        let f = lib.find(&words("bjork"), 1).unwrap();
        assert_eq!((f.tracks.len(), f.totals.tracks, f.totals.albums, f.totals.artists), (1, 2, 1, 1));
        assert_eq!(lib.find(&words("hunter"), 1).unwrap().totals.albums, 1, "by a track's title");
        assert_eq!(lib.find(&words("homogenic"), 1).unwrap().totals.artists, 1, "by an album's name");
        assert_eq!(lib.find(&words("art"), 1).unwrap().totals.genres, 1);
        // The Library page's Tracks: every track without words, those the words name, none from a folder switched off.
        assert_eq!(lib.track_rows(&[], &[]).unwrap().len(), 3);
        let rows = lib.track_rows(&words("bjork"), &[]).unwrap();
        assert_eq!((rows.len(), rows[0].album.as_str()), (2, "Homogenic"));
        assert!(lib.track_rows(&words("bjork"), &[0]).unwrap().is_empty());
        // Several albums' tracks: the albums in the order asked for, each one's in its own; one asked twice once.
        let rows = lib.track_rows(&[], &[]).unwrap();
        let id_of = |album: &str| rows.iter().find(|r| r.album == album).unwrap().album_id.clone();
        let (homogenic, orb) = (id_of("Homogenic"), id_of("Adventures Beyond the Ultraworld"));
        let songs = lib.songs_of_albums(&[orb.clone(), homogenic.clone(), "nothing".into(), orb]).unwrap();
        assert_eq!(songs.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["Little Fluffy Clouds", "Hunter", "Jóga"]);
        assert_eq!(fts_query("a OR b").as_deref(), Some("(\"a\" OR \"a\"*) AND (\"OR\" OR \"OR\"*) AND (\"b\" OR \"b\"*)"), "quoted: a word, not an operator");
    }

    // ---- The trigram tables (0.12: one matcher, search in general).

    /// How many rows of a trigram table hold these letters.
    fn tri_found(lib: &Library, tri: &str, letters: &str) -> i64 {
        lib.db().query_row(&format!("SELECT count(*) FROM {tri} WHERE {tri} MATCH ?1"), [format!("\"{letters}\"")], |r| r.get(0)).unwrap()
    }

    /// Every name of the library found in its trigram table by its own letters, at its own row.
    fn tri_in_step(lib: &Library) {
        let db = lib.db();
        for (tri, table, column) in TRIGRAMS {
            let rows: Vec<(i64, String)> =
                db.prepare(&format!("SELECT n, {column} FROM {table}")).unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
            assert!(!rows.is_empty(), "{table}");
            for (n, name) in rows {
                let letters = cratarr_rules::matcher::letters(&name);
                if letters.chars().count() < 3 {
                    continue;
                }
                let at: i64 = db
                    .query_row(&format!("SELECT count(*) FROM {tri} WHERE {tri} MATCH ?1 AND rowid = ?2"), params![format!("\"{letters}\""), n], |r| r.get(0))
                    .unwrap();
                assert_eq!(at, 1, "{table} {name:?}");
            }
        }
    }

    #[test]
    fn cratarr_letters_is_the_matchers_letters() {
        let lib = Library::open(None).unwrap();
        let db = lib.db();
        for t in ["Hip-Hop", "Björk", "Sigur Rós", "ﬁre Ⅳ", "ＡＢＣ", "Lo-Fi Beats", "AC/DC", "Motörhead", "", "  "] {
            let got: String = db.query_row("SELECT cratarr_letters(?1)", [t], |r| r.get(0)).unwrap();
            assert_eq!(got, cratarr_rules::matcher::letters(t), "{t:?}");
        }
        let hip: String = db.query_row("SELECT cratarr_letters('Hip-Hop')", [], |r| r.get(0)).unwrap();
        assert_eq!(hip, "hiphop");
        let none: Option<String> = db.query_row("SELECT cratarr_letters(NULL)", [], |r| r.get(0)).unwrap();
        assert_eq!(none, None);
    }

    #[test]
    fn the_trigram_tables_keep_in_step_with_the_names() {
        let rules = Rules::default();
        let lib = Library::open(None).unwrap();
        lib.build(&sample(), &rules).unwrap();
        tri_in_step(&lib);
        assert_eq!(tri_found(&lib, "songs_tri_fts", "littlefluffyclouds"), 1);
        assert_eq!(tri_found(&lib, "albums_tri_fts", "adventuresbeyondtheultraworld"), 1);
        assert_eq!(tri_found(&lib, "artists_tri_fts", "theorb"), 1);
        // A title and an artist changed: the new letters in, the old out.
        let changed = with(sample(), |rows| {
            tag(&mut rows[2], "title", "Perpetual Dawn");
            tag(&mut rows[2], "artist", "Orbital");
        });
        lib.credit_in_tests(&changed, &rules);
        lib.update_rows(&with_records(&changed, &rules), &rules, &Covers::new()).unwrap();
        tri_in_step(&lib);
        assert_eq!((tri_found(&lib, "songs_tri_fts", "littlefluffyclouds"), tri_found(&lib, "songs_tri_fts", "perpetualdawn")), (0, 1));
        assert_eq!((tri_found(&lib, "artists_tri_fts", "theorb"), tri_found(&lib, "artists_tri_fts", "orbital")), (0, 1));
        assert_eq!(tri_found(&lib, "albums_tri_fts", "adventuresbeyondtheultraworld"), 1, "built again, once");
        // The file gone: its song, album and artist out.
        let gone = with(changed, |rows| {
            rows.remove(2);
        });
        lib.update_rows(&with_records(&gone, &rules), &rules, &Covers::new()).unwrap();
        tri_in_step(&lib);
        assert_eq!(tri_found(&lib, "songs_tri_fts", "perpetualdawn"), 0);
        assert_eq!(tri_found(&lib, "albums_tri_fts", "adventuresbeyondtheultraworld"), 0);
        assert_eq!(tri_found(&lib, "artists_tri_fts", "orbital"), 0);
        assert_eq!(tri_found(&lib, "artists_tri_fts", "bjork"), 1);
        // Built whole again: nothing left over.
        lib.build(&sample(), &rules).unwrap();
        tri_in_step(&lib);
        assert_eq!((tri_found(&lib, "songs_tri_fts", "littlefluffyclouds"), tri_found(&lib, "songs_tri_fts", "perpetualdawn")), (1, 0));
    }

    #[test]
    fn a_library_from_before_the_trigram_tables_gets_them_on_open() {
        let dir = std::env::temp_dir().join(format!("cratarr-trigram-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("library.sqlite");
        let _ = std::fs::remove_file(&file);
        {
            let lib = Library::open(Some(file.as_path())).unwrap();
            assert!(lib.refresh(&sample(), &Rules::default()).unwrap());
            lib.db().execute_batch("DROP TABLE songs_tri_fts; DROP TABLE albums_tri_fts; DROP TABLE artists_tri_fts;").unwrap();
        }
        let lib = Library::open(Some(file.as_path())).unwrap();
        assert!(!lib.refresh(&sample(), &Rules::default()).unwrap(), "filled where it stood, not built again");
        tri_in_step(&lib);
        assert_eq!(tri_found(&lib, "songs_tri_fts", "littlefluffyclouds"), 1);
        assert_eq!(tri_found(&lib, "albums_tri_fts", "homogenic"), 1);
        assert_eq!(tri_found(&lib, "artists_tri_fts", "bjork"), 1);
        drop(lib);
        // Opened again: made once, left as they are.
        let lib = Library::open(Some(file.as_path())).unwrap();
        assert_eq!(tri_found(&lib, "artists_tri_fts", "bjork"), 1);
        drop(lib);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_finds_names_however_spelled_or_spaced() {
        let t = |n: i64, artist: &str, album: &str, title: &str| {
            row(n, &format!("{artist}/{album}/{n:02} {title}.flac"), &[("title", title), ("artist", artist), ("albumartist", artist), ("album", album), ("tracknumber", "1")])
        };
        let folders = vec![FolderRows {
            folder: 0,
            root: "/music".into(),
            rows: vec![
                t(1, "Hip Hop Orchestra", "Big Band Nights", "Opening"),
                t(2, "Trip Hop Express", "Downtempo Sessions", "Night Train"),
                t(3, "Radiohead", "OK Computer", "Airbag"),
                t(4, "Night Owls", "Lo-Fi Beats", "Rainy Window"),
            ],
        }];
        let lib = Library::open(None).unwrap();
        lib.refresh(&folders, &Rules::default()).unwrap();
        let words = |q: &str| crate::search::words(q);
        let artists = |q: &str| lib.find(&words(q), 10).unwrap().artists.into_iter().map(|(a, _)| a.name).collect::<Vec<_>>();
        let albums = |q: &str| lib.find(&words(q), 10).unwrap().albums.into_iter().map(|(a, h)| (a.name, h.on)).collect::<Vec<_>>();
        // Separators don't count: the words run together find the name (no FTS term holds "hiphop").
        assert_eq!(artists("hiphop"), ["Hip Hop Orchestra"]);
        assert_eq!(lib.find(&words("hiphop"), 10).unwrap().totals.artists, 1);
        // Trip Hop Express is a candidate (it shares hip hop's trigrams) the matcher keeps out: below its floor.
        let tq = crate::search::trigram_query(&words("hiphop")).unwrap();
        let candidates: Vec<String> = lib
            .db()
            .prepare("SELECT a.name FROM artists_tri_fts f JOIN artists a ON a.n = f.rowid WHERE artists_tri_fts MATCH ?1")
            .unwrap()
            .query_map([&tq], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(candidates.iter().any(|c| c == "Trip Hop Express"), "{candidates:?}");
        assert_eq!(cratarr_rules::matcher::score("hiphop", "Trip Hop Express"), None);
        // A typo: the FTS's typo terms find it already, once; a wrong first letter only the trigrams.
        assert_eq!(artists("radiohed"), ["Radiohead"]);
        assert_eq!(artists("xadiohead"), ["Radiohead"]);
        // Spaced or run together, the album by its name.
        assert_eq!(albums("lo fi"), [("Lo-Fi Beats".to_string(), "name")]);
        assert_eq!(albums("lofi"), [("Lo-Fi Beats".to_string(), "name")]);
        let f = lib.find(&words("lofi"), 10).unwrap();
        assert_eq!((f.totals.albums, f.totals.artists, f.totals.tracks), (1, 0, 0));
        // A track by its title, run together.
        let f = lib.find(&words("rainywindow"), 10).unwrap();
        assert_eq!((f.tracks.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), f.totals.tracks), (vec!["Rainy Window"], 1));
        // Under three letters the trigram tables are not asked: what the FTS finds only.
        assert!(lib.find(&words("lf"), 10).unwrap().albums.is_empty());
    }

    /// A real catalogue, built into a library: `CRATARR_REAL_CATALOGUE=<copy of catalogue.sqlite> cargo test
    /// real_catalogue -- --ignored --nocapture`. Prints the time taken and what came out.
    #[test]
    #[ignore]
    fn real_catalogue() {
        let Ok(file) = std::env::var("CRATARR_REAL_CATALOGUE") else { return };
        let cat = crate::catalogue::Catalogue::open(Some(Path::new(&file)), vec!["/media/Music".into(), "/media/lidarr-music".into()]).unwrap();
        let lib = Library::open(None).unwrap();
        let t = std::time::Instant::now();
        assert!(lib.refresh_from(&cat, &Rules::default()).unwrap());
        let first = t.elapsed();
        assert!(!lib.refresh_from(&cat, &Rules::default()).unwrap(), "nothing changed");
        println!("read and built {:?}, checked again {:?}, (songs, albums, artists) {:?}", first, t.elapsed() - first, lib.counts().unwrap());
        // One album's files written by Cratarr (0.9.8): only its folder built again.
        let (folder, rel): (i64, String) = lib
            .db()
            .query_row("SELECT folder, rel FROM songs WHERE album_id = (SELECT id FROM albums ORDER BY song_count DESC LIMIT 1) LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        let path = cat.folders().get(folder).unwrap().join(&rel);
        let t = std::time::Instant::now();
        let n = lib.update_files(&cat, &Rules::default(), &[path]).unwrap();
        println!("one album's folder built again: {n} folders in {:?}", t.elapsed());
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        println!("memory: {}", status.lines().filter(|l| l.starts_with("VmHWM") || l.starts_with("VmRSS")).collect::<Vec<_>>().join(", "));
        let db = lib.db();
        let per_folder: Vec<(i64, i64)> =
            db.prepare("SELECT folder, count(*) FROM copies GROUP BY folder").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
        println!("albums per folder {per_folder:?}");
        let dirs: i64 = db.query_row("SELECT count(DISTINCT folder || '/' || dir) FROM copies", [], |r| r.get(0)).unwrap();
        println!("distinct album folders {dirs}");
        // Albums whose songs lie in more than one folder, and folders split into several albums: grouping by tags vs folders.
        let split: Vec<(String, i64)> = db
            .prepare("SELECT folder || '/' || substr(rel, 1, length(rel) - length(replace(rel, rtrim(rel, replace(rel, '/', '')), ''))) d, count(DISTINCT album_id) n FROM songs GROUP BY d HAVING n > 1 ORDER BY n DESC LIMIT 8")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        println!("folders split over several albums (first 8): {split:#?}");
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

/// What an album lacks of what the library requires (`Library::lacking`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lacks {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub cover: bool,
    pub year: bool,
    pub genre: bool,
    pub album_artist: bool,
    pub track_no: bool,
    pub musicbrainz: bool,
}

impl Lacks {
    pub fn any(&self) -> bool {
        self.cover || self.year || self.genre || self.album_artist || self.track_no || self.musicbrainz
    }

    /// What it lacks, in words (Incoming's marks).
    pub fn words(&self) -> Vec<&'static str> {
        [
            (self.cover, "cover"),
            (self.year, "year"),
            (self.genre, "genre"),
            (self.album_artist, "album artist"),
            (self.track_no, "track numbers"),
            (self.musicbrainz, "MusicBrainz id"),
        ]
        .into_iter()
        .filter_map(|(lacks, word)| lacks.then_some(word))
        .collect()
    }
}
