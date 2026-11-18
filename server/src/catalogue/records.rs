//! A record's references (decision 0100, step 7 of docs/records-and-copies.md): its id at each source that knows it, and the
//! tracklist each gives, beside the record's own row. A record had or not, one a discography names, the same rows.
//!
//! - **A link finds its subject.** A source's release, album page, artist or label is one subject's alone, so the record a pasted
//!   link or a file's tag names is found by it. A release group or a master is not: a deluxe edition and the standard one are one
//!   group and one master, and two records by the same-record rule, so every record of one is found (G1).
//! - **Kept as found, the hand first.** A reference is kept with when it was first found and how (`files`, `link`, `search`,
//!   `hand`): found again it stays as it was, but for a surer way, and a hand never gives way to a guess. A reference one subject
//!   holds alone goes to another by hand, or to a certain match where no hand holds it; a retired record holds none.
//! - **The tracklist that counts** is the record's `counting`: worked out from the lists and the files (`identity::counting`)
//!   until one is chosen by hand (0.14's "Use this"), which a worked-out one never replaces.
//!
//! The table is `"references"`, quoted in every statement: REFERENCES is SQLite's keyword, with no fallback to a name.

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use super::identity::RefTrack;
use super::{Catalogue, now};
use crate::serve::ids;

pub(super) const SCHEMA: &str = "
-- 0100: a record's, an artist's or a label's id at a source (`subject`: al-<record>, ar-<artist>, la-<label>): MusicBrainz's
-- release group or release, Discogs' master or release, a Bandcamp album page; an artist's or a label's at each. `how`: files,
-- link (another source's), search, hand. `confirmed`: a guess confirmed once (0.14). `read_at`: an artist's or a label's
-- listing at that source, read last. A release, an album, an artist or a label is one subject's alone; a release group or a
-- master is every edition's of it (G1).
CREATE TABLE IF NOT EXISTS \"references\" (
  subject TEXT NOT NULL, source TEXT NOT NULL, kind TEXT NOT NULL, value TEXT NOT NULL, at INTEGER NOT NULL, how TEXT NOT NULL,
  confirmed INTEGER NOT NULL DEFAULT 0, read_at INTEGER,
  PRIMARY KEY (source, kind, value, subject)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS references_of ON \"references\" (subject);
CREATE UNIQUE INDEX IF NOT EXISTS references_one ON \"references\" (source, kind, value) WHERE kind IN ('release', 'album', 'artist', 'label');
-- The tracklist a record's reference gives at a source (JSON, `identity::RefTrack`), and when it was read: what judges a folder
-- and what the record's page lists.
CREATE TABLE IF NOT EXISTS reference_tracks (
  record INTEGER NOT NULL, source TEXT NOT NULL, tracks TEXT NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (record, source)) WITHOUT ROWID;
-- X9: the record made for the first request by its names alone whose names several live albums of the artist share (X8), by
-- those names folded (`artist|title`): a later such request takes it, so two accounts asking for one name share one record.
CREATE TABLE IF NOT EXISTS shared_name_records (names TEXT PRIMARY KEY, record INTEGER NOT NULL) WITHOUT ROWID;
-- 0100: a label's id for life, served as `la-<id>`, kept when nothing names it any more, as an artist's (`catalogue::labels`).
-- `key`: one name, one row, by `ids::artist_key`.
CREATE TABLE IF NOT EXISTS labels (id INTEGER PRIMARY KEY AUTOINCREMENT, born INTEGER NOT NULL, name TEXT NOT NULL, key TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS labels_by_key ON labels (key);
-- A record's artists in the order its files credit them, the first its album's artist; and its labels, each with its
-- catalogue number ('' none). What an artist's and a label's pages list, had or not. `how`: `files` (the record pass's, which
-- each build replaces) or `listing` (a discography's or a label's list, which no build touches).
CREATE TABLE IF NOT EXISTS record_artists (
  record INTEGER NOT NULL, artist INTEGER NOT NULL, position INTEGER NOT NULL DEFAULT 0, how TEXT NOT NULL DEFAULT 'files',
  PRIMARY KEY (record, artist)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS record_artists_by_artist ON record_artists (artist, record);
CREATE TABLE IF NOT EXISTS record_labels (
  record INTEGER NOT NULL, label INTEGER NOT NULL, catno TEXT NOT NULL DEFAULT '', how TEXT NOT NULL DEFAULT 'files',
  PRIMARY KEY (record, label, catno)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS record_labels_by_label ON record_labels (label, record);
-- 0.17's step: a record whose certain match from 0.16 names a release whose list is still to be read (`references::read_lists`).
-- It counts nothing and waits off Health's references item until the list is read and held against its files.
-- `tries`: questions that failed for it; let go after a few (R7).
-- C50: a folder a fetcher placed for a record not had (`expect_record`): the album that arrives there, or in a folder under it,
-- is that record, its tracks given to it. By the folder's id, which is for life, and the album's folder in it; one record each;
-- forgotten a week after its last file landed, unclaimed.
CREATE TABLE IF NOT EXISTS expected (
  folder INTEGER NOT NULL, dir TEXT NOT NULL, record INTEGER NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (folder, dir)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS lists_to_read (record INTEGER PRIMARY KEY, release TEXT NOT NULL, at INTEGER NOT NULL, tries INTEGER NOT NULL DEFAULT 0);
-- RT01: what was forgotten and when, by its subject: `al-<record>` (a record not had nobody saw or opened for six months,
-- `Catalogue::forget_quiet`), `ar-<artist>` (an artist left with nothing, RT01b, `Catalogue::forget_quiet_artists`). Its id is
-- never given again (AUTOINCREMENT), and a link to it answers \"no longer known\".
CREATE TABLE IF NOT EXISTS forgotten (subject TEXT PRIMARY KEY, at INTEGER NOT NULL) WITHOUT ROWID;
-- RT01c: when the paced re-read last came to a library artist's list at a source, read or not (`Catalogue::reread_tried`): it
-- comes again a month later, so each comes once a month whatever came of it.
CREATE TABLE IF NOT EXISTS list_reread (artist INTEGER NOT NULL, source TEXT NOT NULL, at INTEGER NOT NULL, PRIMARY KEY (artist, source)) WITHOUT ROWID;
-- Z10: when an artist's last credit went (`record_artists`: a build's retag, the forgetting of their last record), noted where
-- it went: the forgetting counts their day from it, not from their birth. A row of one credited again means nothing.
CREATE TABLE IF NOT EXISTS artists_bare (artist INTEGER PRIMARY KEY, since INTEGER NOT NULL) WITHOUT ROWID;
-- Y12: the records of a name, by the names each row keeps folded (`names_of`); a rename makes them to be folded again.
CREATE INDEX IF NOT EXISTS records_by_names ON records (names);
DROP TRIGGER IF EXISTS records_names_unfolded;
CREATE TRIGGER records_names_unfolded AFTER UPDATE OF artist, title ON records WHEN OLD.artist IS NOT NEW.artist OR OLD.title IS NOT NEW.title
BEGIN UPDATE records SET names = NULL WHERE id = NEW.id; END;
";

/// A chain of merges longer than this is a loop: the record it began at answers none.
const MERGES: usize = 32;

/// The record `id` is now, its merges followed. None: no such id, or merges that go round.
pub(super) fn merged_end(db: &Connection, mut id: i64) -> Result<Option<i64>> {
    for _ in 0..MERGES {
        let into: Option<Option<i64>> = db.prepare_cached("SELECT merged_into FROM records WHERE id = ?1")?.query_row([id], |r| r.get(0)).optional()?;
        match into {
            None => return Ok(None),
            Some(Some(next)) if next != id => id = next,
            Some(_) => return Ok(Some(id)),
        }
    }
    Ok(None)
}

/// Whether a track holds the record: one had.
fn had(tx: &Connection, record: i64) -> Result<bool> {
    Ok(tx.prepare_cached("SELECT EXISTS (SELECT 1 FROM tracks WHERE record = ?1 AND retired_at IS NULL)")?.query_row([record], |r| r.get(0))?)
}

/// A record a list names (an artist's discography's row, a label's), before it has an id: what it is at its source and what
/// that source says it is elsewhere, and what a record not had shows (0100).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listing {
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub kind: Option<String>,
    pub released: Option<String>,
    pub genres: Vec<String>,
    pub cover: Option<String>,
    pub refs: Vec<Ref>,
    pub says: Vec<Ref>,
}

/// `Catalogue::keep_listing` in the caller's transaction, found at `at`. A record it makes, and one it finds that no artist is
/// credited on (Z3), is credited to `artist`. Each record it finds or makes is seen at `seen` (RT01), never earlier than it was,
/// Bandcamp's listings too (Z4); none for a list read before 0.17 (0.17's step), whose clock starts at the first forgetting.
pub(super) fn keep_listing_in(tx: &Connection, artist: Option<i64>, list: &[Listing], how: &str, at: i64, seen: Option<i64>) -> Result<Vec<i64>> {
    let mut out = Vec::with_capacity(list.len());
    for l in list {
        // A listing with no link to another source (`says`) and, as every listing, no tracks is found by its references or
        // not at all: `same_listing` has nothing else to go by, so the scan of every record of its name is skipped (R10).
        let found = match found_by_reference(tx, l)? {
            Some(r) => Some(r),
            None if l.says.is_empty() => None,
            None => found_by_listing(tx, l)?,
        };
        let record = match found {
            Some(r) => r,
            None => {
                let genres = (!l.genres.is_empty()).then(|| l.genres.join(&GENRES_APART.to_string()));
                tx.prepare_cached("INSERT INTO records (born, title, artist, year, kind, released, genres, cover) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")?
                    .execute(params![at, l.title, l.artist, l.year, l.kind, l.released, genres, l.cover])?;
                tx.last_insert_rowid()
            }
        };
        // Credited to the artist whose list it is: a record made now, and one found that no artist is credited on yet (Z3: opened
        // from a label's page or Beyond, `keep_listing(None, …)`). A credit by reference, never by a name; one credited stays so.
        if let Some(artist) = artist {
            tx.prepare_cached(
                "INSERT INTO record_artists (record, artist, position, how) SELECT ?1, ?2, 0, 'listing' WHERE NOT EXISTS (SELECT 1 FROM record_artists WHERE record = ?1)",
            )?
            .execute(params![record, artist])?;
        }
        for r in &l.refs {
            put_reference(tx, &ids::record(record), r, how, at, false)?;
        }
        // Seen by the list that names it, Bandcamp's too (Z4, the owner's rule: an artist page's reads refresh what they show),
        // though it lists only its search box's first answers (O4); a listing with no reference (a Want's) as well.
        if let Some(seen) = seen {
            tx.prepare_cached(SEEN)?.execute(params![record, seen])?;
        }
        out.push(record);
    }
    Ok(out)
}

/// The record a listing's references name: one a release or an album page names first, else the oldest of its release group
/// or master (G1: any of them is it, for a list that names the group), each as merged now.
pub(super) fn found_by_reference(tx: &Connection, l: &Listing) -> Result<Option<i64>> {
    let mut refs: Vec<&Ref> = l.refs.iter().collect();
    refs.sort_by_key(|r| !ONE_SUBJECT.contains(&r.kind.as_str()));
    for r in refs {
        let mut holders: Vec<i64> = Vec::new();
        for subject in subjects_in(tx, r)? {
            if let Some(id) = ids::record_number(&subject)
                && let Some(record) = merged_end(tx, id)?
            {
                holders.push(record);
            }
        }
        if let Some(record) = holders.into_iter().min() {
            return Ok(Some(record));
        }
    }
    Ok(None)
}

/// The record of a listing's name that `identity::same_listing` says it is, a link of one to the other's deciding, as a name
/// alone never does: the oldest where several are.
fn found_by_listing(tx: &Connection, l: &Listing) -> Result<Option<i64>> {
    use super::identity::{Known, fold, same_listing};
    let (artist, title) = (fold(&l.artist), fold(&l.title));
    let known = |refs: Vec<Ref>, says: Vec<Ref>| Known { refs, says, artist: artist.clone(), title: title.clone(), tracks: Vec::new() };
    let listing = known(l.refs.clone(), l.says.clone());
    let named: Vec<i64> = {
        let mut st = tx.prepare_cached("SELECT id, COALESCE(artist, ''), COALESCE(title, '') FROM records WHERE merged_into IS NULL ORDER BY id")?;
        let rows: Vec<(i64, String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        rows.into_iter().filter(|(_, a, t)| fold(a) == artist && fold(t) == title).map(|(id, ..)| id).collect()
    };
    for record in named {
        let refs: Vec<Ref> = {
            let mut st = tx.prepare_cached("SELECT source, kind, value FROM \"references\" WHERE subject = ?1")?;
            st.query_map([ids::record(record)], |r| Ok(Ref { source: r.get(0)?, kind: r.get(1)?, value: r.get(2)? }))?.collect::<rusqlite::Result<_>>()?
        };
        if same_listing(&listing, &known(refs, Vec::new())) {
            return Ok(Some(record));
        }
    }
    Ok(None)
}

/// `from` merged into `into` in the caller's transaction (G3): its references go to `into`, but where `into` holds one already,
/// kept the surer way, and confirmed where either was; its lists where `into` has none from that source; its count where `into`
/// has none, and its checks; its artists (after `into`'s) and labels; the facts `into` lacks. `from` answers through
/// `merged_into` from then on, and so does every record merged into it before. A record with copies keeps its id, and two with
/// copies are one only by their files: the record pass's, refused here. Each is followed to where it was merged first (R9), a
/// retired record takes no merge (it holds no release or page), and a reference `into` cannot take stops the merge: nothing of
/// `from` is lost on the way.
pub(super) fn merge_records_in(tx: &Connection, from: i64, into: i64) -> Result<()> {
    let (from, into) = (merged_end(tx, from)?.context("no such record")?, merged_end(tx, into)?.context("no such record")?);
    if from == into {
        return Ok(());
    }
    anyhow::ensure!(!had(tx, from)?, "a record with copies keeps its id: merge the other into it, and two with copies are one only by their files");
    anyhow::ensure!(!retired(tx, &ids::record(into))?, "a retired record takes no merge: it holds no release or page");
    let (subject, target) = (ids::record(from), ids::record(into));
    let refs: Vec<(Ref, String, i64, bool)> = {
        let mut st = tx.prepare("SELECT source, kind, value, how, at, confirmed FROM \"references\" WHERE subject = ?1")?;
        st.query_map([&subject], |r| Ok((Ref { source: r.get(0)?, kind: r.get(1)?, value: r.get(2)? }, r.get(3)?, r.get(4)?, r.get(5)?)))?.collect::<rusqlite::Result<_>>()?
    };
    tx.execute("DELETE FROM \"references\" WHERE subject = ?1", [&subject])?;
    for (r, how, at, confirmed) in &refs {
        match put_reference(tx, &target, r, how, *at, false)? {
            Put::Kept(_) => {}
            refused => anyhow::bail!("{} cannot take {} from {subject} ({refused:?})", target, named(&subject, r)),
        }
        if *confirmed {
            tx.execute("UPDATE \"references\" SET confirmed = 1 WHERE subject = ?1 AND source = ?2 AND kind = ?3 AND value = ?4", params![target, r.source, r.kind, r.value])?;
        }
    }
    tx.execute("INSERT OR IGNORE INTO reference_tracks (record, source, tracks, at) SELECT ?2, source, tracks, at FROM reference_tracks WHERE record = ?1", params![from, into])?;
    tx.execute("DELETE FROM reference_tracks WHERE record = ?1", [from])?;
    tx.execute(
        "UPDATE records SET
           counted_by_hand = CASE WHEN counting IS NULL AND counted_by_hand = 0 THEN (SELECT counted_by_hand FROM records WHERE id = ?1) ELSE counted_by_hand END,
           counting = COALESCE(counting, (SELECT counting FROM records WHERE id = ?1)),
           checked_by_hand = max(checked_by_hand, (SELECT checked_by_hand FROM records WHERE id = ?1)),
           title = COALESCE(title, (SELECT title FROM records WHERE id = ?1)), artist = COALESCE(artist, (SELECT artist FROM records WHERE id = ?1)),
           year = COALESCE(year, (SELECT year FROM records WHERE id = ?1)), kind = COALESCE(kind, (SELECT kind FROM records WHERE id = ?1)),
           released = COALESCE(released, (SELECT released FROM records WHERE id = ?1)), genres = COALESCE(genres, (SELECT genres FROM records WHERE id = ?1)),
           cover = COALESCE(cover, (SELECT cover FROM records WHERE id = ?1))
         WHERE id = ?2",
        params![from, into],
    )?;
    let after: i64 = tx.query_row("SELECT COALESCE(max(position) + 1, 0) FROM record_artists WHERE record = ?1", [into], |r| r.get(0))?;
    tx.execute("INSERT OR IGNORE INTO record_artists (record, artist, position, how) SELECT ?2, artist, position + ?3, how FROM record_artists WHERE record = ?1", params![from, into, after])?;
    tx.execute("DELETE FROM record_artists WHERE record = ?1", [from])?;
    tx.execute("INSERT OR IGNORE INTO record_labels (record, label, catno, how) SELECT ?2, label, catno, how FROM record_labels WHERE record = ?1", params![from, into])?;
    tx.execute("DELETE FROM record_labels WHERE record = ?1", [from])?;
    tx.execute("UPDATE records SET merged_into = ?2 WHERE id = ?1 OR merged_into = ?1", params![from, into])?;
    Ok(())
}

/// The kinds a source gives to one subject alone (G1): what `references_one` holds unique.
const ONE_SUBJECT: [&str; 4] = ["release", "album", "artist", "label"];

pub use super::identity::Ref;

/// What is kept beside a reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceRow {
    /// When it was first found.
    pub at: i64,
    /// `files`, `link`, `search` or `hand`.
    pub how: String,
    pub confirmed: bool,
    /// An artist's or a label's listing at this source, read last.
    pub read_at: Option<i64>,
}

/// A file of a record as `Catalogue::went_of` reads it (U7, T8): its catalogue folder, and when it came there, went (`gone_at`), or
/// went into a trash (its token's time); for a track retired since, where its last file lay and when that went, as kept then.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Went {
    pub folder: i64,
    pub came: Option<i64>,
    pub gone: Option<i64>,
    pub trashed: Option<i64>,
}

/// A record as requests and lists read it (`Catalogue::requested`, R16; `Catalogue::missing_of`, C53a): its row, its MusicBrainz
/// release groups (in value order), its Bandcamp album page, every reference it holds, and the record that holds its files where
/// it is had (R13).
#[derive(Debug, Clone, PartialEq)]
pub struct Requested {
    pub row: RecordRow,
    pub groups: Vec<String>,
    pub bandcamp: Option<String>,
    /// (source, kind, value).
    pub refs: Vec<(String, String, String)>,
    pub holder: Option<i64>,
    /// Read for requests alone (`Catalogue::requested`, W4): what its holder holds of it, as `holding_in` gives it.
    pub copies: Vec<(i64, usize)>,
    pub present: usize,
    pub list: Option<usize>,
}

/// What references name here (`Catalogue::by_reference`, C57): a record never by a name; for a row no record holds, the had record
/// its names point at (Z6), for display alone.
#[derive(Debug, Clone, Default)]
pub struct ByReference {
    /// Each album reference's record as merged now, in their order; none where no record holds it.
    pub records: Vec<Option<i64>>,
    /// Those records, read as the lists read them, by id.
    pub read: std::collections::HashMap<i64, Requested>,
    /// Each artist reference's artist, and whether the library has albums by them, in their order; none where no artist holds it.
    pub artists: Vec<Option<(i64, bool)>>,
    /// Each album reference no record holds: the live had record holding it through its names (`held_by_names_in`, Z6), in their
    /// order; none where a record holds it, or its names hold nothing.
    pub named: Vec<Option<i64>>,
}

/// A record as it stands, had or not.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordRow {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub kind: Option<String>,
    /// Its first release date as a source writes it, for a record not had: no file says it.
    pub released: Option<String>,
    pub genres: Vec<String>,
    /// A picture's address where no reference makes one.
    pub cover: Option<String>,
    /// The source whose tracklist counts, and whether that was chosen by hand.
    pub counting: Option<String>,
    pub counted_by_hand: bool,
    /// 0.14's "Checked by hand": its own facts are its reference.
    pub checked_by_hand: bool,
    /// The record it became where two proved one (G3); C47 follows it.
    pub merged_into: Option<i64>,
    pub retired_at: Option<i64>,
    /// Whether a track of it is there: a record not had has none.
    pub had: bool,
}

/// Genres as a column holds them: one string, joined by U+001F, as the tags' several values are.
const GENRES_APART: char = '\u{1f}';

/// How sure a way of finding a reference is: found again a surer way, the way is kept.
fn sureness(how: &str) -> u8 {
    match how {
        "hand" => 3,
        "files" => 2,
        "link" => 1,
        _ => 0,
    }
}

/// A record's row as `records` holds it: what `row_of` reads, in its order.
const RECORD_COLUMNS: &str = "id, COALESCE(title, ''), COALESCE(artist, ''), year, kind, released, genres, cover, counting, counted_by_hand, checked_by_hand, merged_into, retired_at,
   EXISTS (SELECT 1 FROM tracks t WHERE t.record = records.id AND t.retired_at IS NULL AND (EXISTS (SELECT 1 FROM files f WHERE f.track = t.id AND f.trashed IS NULL) OR NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id)))";

fn row_of(r: &rusqlite::Row) -> rusqlite::Result<RecordRow> {
    let genres: Option<String> = r.get(6)?;
    Ok(RecordRow {
        id: r.get(0)?,
        title: r.get(1)?,
        artist: r.get(2)?,
        year: r.get(3)?,
        kind: r.get(4)?,
        released: r.get(5)?,
        genres: genres.map(|g| g.split(GENRES_APART).filter(|g| !g.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
        cover: r.get(7)?,
        counting: r.get(8)?,
        counted_by_hand: r.get(9)?,
        checked_by_hand: r.get(10)?,
        merged_into: r.get(11)?,
        retired_at: r.get(12)?,
        had: r.get(13)?,
    })
}

fn record_in(db: &Connection, id: i64) -> Result<Option<RecordRow>> {
    Ok(db.prepare_cached(&format!("SELECT {RECORD_COLUMNS} FROM records WHERE id = ?1"))?.query_row([id], row_of).optional()?)
}

/// A MusicBrainz id's shape: what a tag must hold to name a release.
fn is_mbid(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// What a record's files name at MusicBrainz, of one kind (a release, a release group), each file by its first value: the
/// value most of them name, the lower where as many name two, folded to lower case; none where no file names one. The one
/// rule for what an album's files name: the record pass's (`files_named`, `files_references`), 0.17's step's, and the identity
/// an album is held by (`serve::library::album_copies`).
pub fn most_named<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for value in values {
        let value = value.trim().to_lowercase();
        if !value.is_empty() {
            *counts.entry(value).or_default() += 1;
        }
    }
    counts.into_iter().max_by(|(a, x), (b, y)| x.cmp(y).then(b.cmp(a))).map(|(value, _)| value)
}

/// What each record's files name at MusicBrainz, as their tags say now (`most_named`), all its copies' files together: (record,
/// `release` or `release-group`, id), the records in order. A value that is no MusicBrainz id names nothing. 0.17's step's, which
/// has no copies ranked; the first build after it keeps what each record's first-ranked copy names (`files_references`).
fn files_named(tx: &Connection) -> Result<Vec<(i64, &'static str, String)>> {
    let mut st = tx.prepare(
        "SELECT t.record, g.name, g.folder, g.rel, g.value FROM tags g
         JOIN files f ON f.folder = g.folder AND f.rel = g.rel
         JOIN tracks t ON t.id = f.track
         WHERE g.name IN ('musicbrainz_albumid', 'musicbrainz_releasegroupid') AND f.folder >= 0 AND f.gone_at IS NULL AND f.trashed IS NULL
           AND t.record IS NOT NULL AND t.retired_at IS NULL
         ORDER BY t.record, g.name, g.folder, g.rel, g.n",
    )?;
    let rows: Vec<(i64, String, i64, String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut firsts: std::collections::BTreeMap<(i64, &str), Vec<&str>> = std::collections::BTreeMap::new();
    let mut file = None;
    for (record, name, folder, rel, value) in &rows {
        let this = (*record, name.as_str(), *folder, rel.as_str());
        if file.replace(this) != Some(this) {
            firsts.entry((*record, name.as_str())).or_default().push(value.as_str());
        }
    }
    let mut out = Vec::new();
    for ((record, name), values) in firsts {
        if let Some(value) = most_named(values).filter(|v| is_mbid(v)) {
            out.push((record, if name == "musicbrainz_albumid" { "release" } else { "release-group" }, value));
        }
    }
    Ok(out)
}

/// Each record's copy that counts in a pass (`records[i]` is `albums[i]`'s): the first by kind, then the largest, then the first
/// listed, as `assign` keeps the record with (CR18). What its files name and credit is the record's: an Incoming copy nobody
/// has checked does not decide.
fn first_ranked<'a>(albums: &'a [super::AlbumCopy], records: &[Option<i64>]) -> std::collections::BTreeMap<i64, &'a super::AlbumCopy> {
    let key = |i: usize, a: &super::AlbumCopy| (a.rank, a.tracks.len(), std::cmp::Reverse(i));
    let mut first: std::collections::BTreeMap<i64, (usize, &super::AlbumCopy)> = std::collections::BTreeMap::new();
    for (i, (a, record)) in albums.iter().zip(records).enumerate() {
        let Some(record) = record else { continue };
        if first.get(record).is_none_or(|&(j, b)| key(i, a) > key(j, b)) {
            first.insert(*record, (i, a));
        }
    }
    first.into_iter().map(|(record, (_, a))| (record, a)).collect()
}

/// A record's MusicBrainz list once its files name another release than the one it was read for (R8, R17): gone, unless the count was chosen by
/// hand from it, and the count worked out again from the lists left and the files (`identity::counting`), unless chosen by
/// hand. With no files to hold a list against, nothing counts.
fn let_go_list(tx: &Connection, record: i64, files: &[super::identity::Listed]) -> Result<()> {
    let (counting, by_hand): (Option<String>, bool) = tx.query_row("SELECT counting, counted_by_hand FROM records WHERE id = ?1", [record], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if by_hand && counting.as_deref() == Some("musicbrainz") {
        return Ok(());
    }
    if tx.execute("DELETE FROM reference_tracks WHERE record = ?1 AND source = 'musicbrainz'", [record])? == 0 || by_hand {
        return Ok(());
    }
    let kept: Vec<(String, Vec<RefTrack>)> = {
        let mut st = tx.prepare_cached("SELECT source, tracks FROM reference_tracks WHERE record = ?1 ORDER BY source")?;
        let rows: Vec<(String, String)> = st.query_map([record], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        rows.into_iter().filter_map(|(source, tracks)| Some((source, serde_json::from_str(&tracks).ok()?))).collect()
    };
    let lists: Vec<(&str, &[RefTrack])> = kept.iter().map(|(s, l)| (s.as_str(), l.as_slice())).filter(|(_, l)| !l.is_empty()).collect();
    let worked = if files.is_empty() { None } else { super::identity::counting(None, &lists, files) };
    tx.execute("UPDATE records SET counting = ?2 WHERE id = ?1 AND counted_by_hand = 0", params![record, worked])?;
    Ok(())
}

/// A record's count worked out again against its files (R25, N6), unless chosen by hand, in the caller's transaction: what
/// `references::work_out_in` does, for a record an album has just joined. A record not had that a list counted before its album
/// came (Soulseek's search, `tracklist`, in the design's order) keeps that count only where the list agrees with the files that
/// came; else none counts until one is chosen, the owner's rule for a record with files (`identity::counting`).
pub(super) fn recount_in(tx: &Connection, record: i64, files: &[super::identity::Listed]) -> Result<()> {
    if files.is_empty() || tx.query_row("SELECT counted_by_hand FROM records WHERE id = ?1", [record], |r| r.get::<_, bool>(0))? {
        return Ok(());
    }
    let kept: Vec<(String, Vec<RefTrack>)> = {
        let mut st = tx.prepare_cached("SELECT source, tracks FROM reference_tracks WHERE record = ?1 ORDER BY source")?;
        let rows: Vec<(String, String)> = st.query_map([record], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        rows.into_iter().filter_map(|(source, tracks)| Some((source, serde_json::from_str(&tracks).ok()?))).collect()
    };
    let lists: Vec<(&str, &[RefTrack])> = kept.iter().map(|(s, l)| (s.as_str(), l.as_slice())).filter(|(_, l)| !l.is_empty()).collect();
    let worked = super::identity::counting(None, &lists, files);
    tx.execute("UPDATE records SET counting = ?2 WHERE id = ?1 AND counted_by_hand = 0", params![record, worked])?;
    Ok(())
}

/// What the records of a pass name at MusicBrainz now, each by its first-ranked copy (`first_ranked`: its release and release
/// group, `most_named` of that copy's files), kept as their `files` references in the record pass's transaction: a release or a
/// release group they named before and name no more is let go of (a retag), a release with its list and count (`let_go_list`),
/// and one they name now kept. Those kept already are left as they are, so a pass over a library as it was writes nothing; one
/// a search found becomes the files'. In the records' order: where one release's files make two records (G1, a bonus track,
/// another master), the older record keeps the release and the newer only the group, until a certain match or a link pasted
/// says which it is.
pub(super) fn files_references(tx: &Connection, albums: &[super::AlbumCopy], records: &[Option<i64>], at: i64) -> Result<()> {
    let first = first_ranked(albums, records);
    let named: Vec<(i64, &str, String)> = first
        .iter()
        .flat_map(|(record, a)| [("release", a.facts.release.clone()), ("release-group", a.group.clone())].into_iter().filter_map(move |(kind, v)| Some((*record, kind, v.filter(|v| is_mbid(v))?))))
        .collect();
    let now: std::collections::HashSet<(String, &str, &str)> = named.iter().map(|(record, kind, value)| (ids::record(*record), *kind, value.as_str())).collect();
    let held: Vec<(String, String, String)> = {
        let mut st = tx.prepare_cached("SELECT subject, kind, value FROM \"references\" WHERE source = 'musicbrainz' AND kind IN ('release', 'release-group') AND how = 'files'")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?
    };
    for (subject, kind, value) in &held {
        let Some((record, a)) = ids::record_number(subject).and_then(|record| Some((record, *first.get(&record)?))) else { continue };
        // Let go of on a retag only (R17, F8): the copy that counts names another of its kind. One that names none (the copy that
        // named it missing for a build, an untagged Incoming copy ranked first meanwhile) leaves it, and its list, as they are.
        let named_now = if kind == "release" { a.facts.release.as_deref() } else { a.group.as_deref() };
        let names_other = named_now.is_some_and(|v| is_mbid(v) && !v.eq_ignore_ascii_case(value));
        if names_other && !now.contains(&(subject.clone(), kind.as_str(), value.as_str())) {
            tx.prepare_cached("DELETE FROM \"references\" WHERE subject = ?1 AND source = 'musicbrainz' AND kind = ?2 AND value = ?3")?.execute(params![subject, kind, value])?;
            if kind == "release" {
                let_go_list(tx, record, &a.facts.tracks)?;
            }
        }
    }
    let known: std::collections::HashSet<(String, String, String)> = {
        let mut st = tx.prepare_cached("SELECT subject, kind, value FROM \"references\" WHERE source = 'musicbrainz' AND kind IN ('release', 'release-group') AND how IN ('files', 'hand')")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?
    };
    for (record, kind, value) in named {
        let subject = ids::record(record);
        if !known.contains(&(subject.clone(), kind.to_string(), value.clone())) {
            put_reference(tx, &subject, &Ref::new("musicbrainz", kind, &value), "files", at, false)?;
        }
    }
    Ok(())
}

/// Who each album's files credit and what labels they name, kept for its record in the record pass's transaction (C46): the
/// album artists in order (`record_artists`), each artist's MusicBrainz id as a `files` reference where the files give one
/// id for each, and the labels with their catalogue numbers (`record_labels`). A record of several copies goes by its
/// first-ranked copy (`first_ranked`, R8). What a record holds already is left alone, so a pass over a library as it was writes
/// nothing; what its files say now takes the place of what they said before: its title and artist too (R14, F6), as a request's
/// record and the had rule read them.
pub(super) fn record_credits(tx: &Connection, albums: &[super::AlbumCopy], records: &[Option<i64>], at: i64) -> Result<()> {
    let first = first_ranked(albums, records);
    let names: std::collections::BTreeSet<String> = first.values().flat_map(|a| a.artists.iter().cloned()).collect();
    let artists = super::artists::artists_in(tx, &names, at)?;
    let label_names: std::collections::BTreeSet<String> = first.values().flat_map(|a| a.labels.iter().map(|(l, _)| l.clone())).collect();
    let labels = super::labels::labels_in(tx, &label_names, at)?;
    // The build's own rows (`files`) alone: a list's (`listing`, a merge's) are no build's to replace (R9).
    let held_artists = grouped(tx, "SELECT record, artist FROM record_artists WHERE how = 'files' ORDER BY record, position")?;
    let held_labels: std::collections::HashMap<i64, Vec<(i64, String)>> = {
        let mut st = tx.prepare("SELECT record, label, catno FROM record_labels WHERE how = 'files' ORDER BY record, label, catno")?;
        let mut out: std::collections::HashMap<i64, Vec<(i64, String)>> = std::collections::HashMap::new();
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))? {
            let (record, label, catno) = row?;
            out.entry(record).or_default().push((label, catno));
        }
        out
    };
    // The artists a credit went from here (Z10), noted at the end where it was their last.
    let mut uncredited: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    for (record, a) in first {
        // Its names change on a retag only (R28, N11), as its list is let go of (R17): the copy that counts must rank at least as
        // high as the one that named it last (`named_rank`). A build without that copy (its library parked or out of reach, an
        // Incoming copy ranked first meanwhile) leaves them; a retag of it shows, as does a better copy (a Keep into Music).
        tx.prepare_cached(
            "UPDATE records SET title = ?2, artist = ?3, named_rank = ?4
             WHERE id = ?1 AND ?2 <> '' AND ?4 >= COALESCE(named_rank, 0) AND (title IS NOT ?2 OR artist IS NOT ?3 OR named_rank IS NOT ?4)",
        )?
        .execute(params![record, a.title, a.artist, a.rank])?;
        let mut ids: Vec<i64> = Vec::new();
        for name in &a.artists {
            if let Some(id) = artists.get(&ids::artist_key(name))
                && !ids.contains(id)
            {
                ids.push(*id);
            }
        }
        // Credited as its files say once it holds them (R17, F7): the artists a listing credited it (a request's names, a
        // discography's) give way wherever the files credit one; a listing's row of a label gives way to the files' own row of
        // that label. A label the files do not name stays, as a merge brought it (R9).
        if !ids.is_empty() {
            let mut gone = tx.prepare_cached("DELETE FROM record_artists WHERE record = ?1 AND how = 'listing' RETURNING artist")?;
            for artist in gone.query_map([record], |r| r.get::<_, i64>(0))? {
                uncredited.insert(artist?);
            }
        }
        if held_artists.get(&record).map_or(&[][..], Vec::as_slice) != ids.as_slice() {
            tx.prepare_cached("DELETE FROM record_artists WHERE record = ?1 AND how = 'files'")?.execute([record])?;
            uncredited.extend(held_artists.get(&record).into_iter().flatten());
            for (position, artist) in ids.iter().enumerate() {
                tx.prepare_cached(
                    "INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, ?3, 'files')
                     ON CONFLICT (record, artist) DO UPDATE SET position = excluded.position, how = 'files'",
                )?
                .execute(params![record, artist, position as i64])?;
            }
        }
        if a.artist_ids.len() == a.artists.len() {
            for (name, mbid) in a.artists.iter().zip(&a.artist_ids) {
                if let Some(id) = artists.get(&ids::artist_key(name))
                    && is_mbid(mbid)
                {
                    put_reference(tx, &ids::artist(*id), &Ref::new("musicbrainz", "artist", mbid), "files", at, false)?;
                }
            }
        }
        let mut named: Vec<(i64, String)> = a.labels.iter().filter_map(|(name, catno)| Some((*labels.get(&ids::artist_key(name))?, catno.clone()))).collect();
        named.sort();
        named.dedup();
        for (label, _) in &named {
            tx.prepare_cached("DELETE FROM record_labels WHERE record = ?1 AND label = ?2 AND how = 'listing'")?.execute(params![record, label])?;
        }
        if held_labels.get(&record).map_or(&[][..], Vec::as_slice) != named.as_slice() {
            tx.prepare_cached("DELETE FROM record_labels WHERE record = ?1 AND how = 'files'")?.execute([record])?;
            for (label, catno) in &named {
                tx.prepare_cached("INSERT INTO record_labels (record, label, catno, how) VALUES (?1, ?2, ?3, 'files') ON CONFLICT (record, label, catno) DO UPDATE SET how = 'files'")?
                    .execute(params![record, label, catno])?;
            }
        }
    }
    for artist in uncredited {
        tx.prepare_cached(LAST_CREDIT_WENT)?.execute(params![artist, at])?;
    }
    Ok(())
}

/// A two-column query's second column by its first, in the query's order.
fn grouped(tx: &Connection, sql: &str) -> Result<std::collections::HashMap<i64, Vec<i64>>> {
    let mut st = tx.prepare(sql)?;
    let mut out: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
    for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
        let (a, b) = row?;
        out.entry(a).or_default().push(b);
    }
    Ok(out)
}

/// What a reference write did: kept, with the subjects it was taken from (none, mostly); or refused, as another holds it
/// alone, or as a retired record holds no such reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Put {
    Kept(Vec<String>),
    Held,
    Retired,
}

impl Put {
    pub(super) fn kept(&self) -> bool {
        matches!(self, Put::Kept(_))
    }
}

/// Whether a subject is a record retired or merged into another: it keeps its choices and its groups and masters, and holds no
/// release, album page, artist or label, which name what is there now.
fn retired(tx: &Connection, subject: &str) -> Result<bool> {
    let Some(id) = ids::record_number(subject) else { return Ok(false) };
    let gone: Option<bool> = tx.prepare_cached("SELECT retired_at IS NOT NULL OR merged_into IS NOT NULL FROM records WHERE id = ?1")?.query_row([id], |r| r.get(0)).optional()?;
    Ok(gone.unwrap_or(false))
}

/// `set_reference` in the caller's transaction, found at `at`. One a subject holds alone (a release, an album page, an artist, a
/// label): a retired record is given none, and one it held goes to whoever names it now; held by another, it is taken by hand,
/// or where `takes` (a certain match), and never from a hand by anything else: a link pasted names what it names. An artist's id
/// held by `search` alone, by an artist credited on no had record (one opened from Beyond, Z2, or from a card, G7), gives way to
/// the files', and to the monthly read's (`search`, T7) for an artist credited on a had record; the records that artist's lists
/// made (credited `listing`) go with it to the taker (U4): an artist with nothing of their own is merged into them, left with
/// nothing, and forgotten a day later (Z10).
pub(super) fn put_reference(tx: &Connection, subject: &str, r: &Ref, how: &str, at: i64, takes: bool) -> Result<Put> {
    let mut taken = Vec::new();
    if ONE_SUBJECT.contains(&r.kind.as_str()) {
        if retired(tx, subject)? {
            return Ok(Put::Retired);
        }
        let holders: Vec<(String, String)> = {
            let mut st = tx.prepare_cached("SELECT subject, how FROM \"references\" WHERE source = ?1 AND kind = ?2 AND value = ?3 AND subject <> ?4 ORDER BY subject")?;
            st.query_map(params![r.source, r.kind, r.value, subject], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<_>>()?
        };
        // Decided over every holder before any lets go: a refusal leaves each where it was, though the caller commits. (The
        // schema holds one holder, `references_one`; the rule stands without counting on it.)
        let mut giving = Vec::new();
        for (holder, held_how) in &holders {
            // The monthly read takes only for an artist with a had record (T7): two of a name with none never pass it back and forth.
            let gives_way = r.kind == "artist" && held_how == "search" && (how == "files" || (how == "search" && !credited_on_none_had(tx, subject)?)) && credited_on_none_had(tx, holder)?;
            if !retired(tx, holder)? && ((held_how == "hand" && how != "hand") || (!takes && how != "hand" && !gives_way)) {
                return Ok(Put::Held);
            }
            if gives_way {
                giving.push(holder.clone());
            }
        }
        for (holder, _) in holders {
            tx.prepare_cached("DELETE FROM \"references\" WHERE source = ?1 AND kind = ?2 AND value = ?3 AND subject = ?4")?.execute(params![r.source, r.kind, r.value, holder])?;
            // The records its lists made go with the id (U4): read by it, they are the taker's to list, and its page and Discover's
            // missing list show them. One the taker is credited on already keeps that credit alone.
            if giving.contains(&holder)
                && let (Some(from), Some(to)) = (ids::artist_number(&holder), ids::artist_number(subject))
            {
                tx.prepare_cached("UPDATE OR IGNORE record_artists SET artist = ?2 WHERE artist = ?1 AND how = 'listing'")?.execute(params![from, to])?;
                tx.prepare_cached("DELETE FROM record_artists WHERE artist = ?1 AND how = 'listing'")?.execute([from])?;
                tx.prepare_cached(LAST_CREDIT_WENT)?.execute(params![from, at])?;
            }
            taken.push(holder);
        }
    }
    let had: Option<String> = tx
        .prepare_cached("SELECT how FROM \"references\" WHERE source = ?1 AND kind = ?2 AND value = ?3 AND subject = ?4")?
        .query_row(params![r.source, r.kind, r.value, subject], |r| r.get(0))
        .optional()?;
    match had {
        None => {
            tx.prepare_cached("INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?.execute(params![subject, r.source, r.kind, r.value, at, how])?;
        }
        Some(was) if sureness(how) > sureness(&was) => {
            tx.prepare_cached("UPDATE \"references\" SET how = ?5 WHERE source = ?1 AND kind = ?2 AND value = ?3 AND subject = ?4")?.execute(params![r.source, r.kind, r.value, subject, how])?;
        }
        Some(_) => {}
    }
    Ok(Put::Kept(taken))
}

/// Whether an artist (`ar-<n>`) is credited on no live record that holds live tracks (Z2): their id held by `search` alone gives
/// way to the files' (`put_reference`).
fn credited_on_none_had(tx: &Connection, subject: &str) -> Result<bool> {
    let Some(artist) = ids::artist_number(subject) else { return Ok(false) };
    let credited: bool = tx
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM record_artists a JOIN records r ON r.id = a.record WHERE a.artist = ?1 AND r.merged_into IS NULL AND r.retired_at IS NULL
               AND EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL))",
        )?
        .query_row([artist], |row| row.get(0))?;
    Ok(!credited)
}

fn subjects_in(db: &Connection, r: &Ref) -> Result<Vec<String>> {
    let mut st = db.prepare_cached("SELECT subject FROM \"references\" WHERE source = ?1 AND kind = ?2 AND value = ?3 ORDER BY subject")?;
    let rows = st.query_map(params![r.source, r.kind, r.value], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// ---- Had (R13): one rule for whether a record is had, a request's record's and the artist pages' (C53a)

/// The references that say two records are two where both hold one source's same kind with no value in common, whatever their
/// names say.
const IDENTIFYING: [(&str, &str); 5] = [("musicbrainz", "release-group"), ("musicbrainz", "release"), ("discogs", "master"), ("discogs", "release"), ("bandcamp", "album")];

/// Records by their folded artist and title, read once for all a caller asks after: the live ones, had or not (`holders_in`, Y1:
/// which are had it asks of the few it needs); the records not had, once a record pass (`wanted_record`).
pub(super) type FoldedNames = Option<std::collections::HashMap<(String, String), Vec<i64>>>;

/// Ids as a query holds them: numbers alone, so the text is safe.
fn id_list(ids: &[i64]) -> String {
    ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
}

/// Values as a query holds them, each quoted.
fn quoted_list(values: impl IntoIterator<Item = String>) -> String {
    values.into_iter().map(|v| format!("'{}'", v.replace('\'', "''"))).collect::<Vec<_>>().join(",")
}

/// Which of these records are live and hold live tracks, in one query.
fn live_had_of(db: &Connection, records: &[i64]) -> Result<std::collections::HashSet<i64>> {
    if records.is_empty() {
        return Ok(std::collections::HashSet::new());
    }
    let sql = format!(
        "SELECT r.id FROM records r WHERE r.id IN ({}) AND r.merged_into IS NULL AND r.retired_at IS NULL
         AND EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL AND (EXISTS (SELECT 1 FROM files f WHERE f.track = t.id AND f.trashed IS NULL) OR NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id)))",
        id_list(records)
    );
    let mut st = db.prepare(&sql)?;
    let rows = st.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// These records' references, by record, as (source, kind, value) in their order, in one query.
fn refs_of(db: &Connection, records: &[i64]) -> Result<std::collections::HashMap<i64, Vec<(String, String, String)>>> {
    let mut out: std::collections::HashMap<i64, Vec<(String, String, String)>> = std::collections::HashMap::new();
    if records.is_empty() {
        return Ok(out);
    }
    let sql = format!("SELECT subject, source, kind, value FROM \"references\" WHERE subject IN ({}) ORDER BY subject, source, kind, value", quoted_list(records.iter().map(|id| ids::record(*id))));
    let mut st = db.prepare(&sql)?;
    for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))? {
        let (subject, source, kind, value) = row?;
        if let Some(id) = ids::record_number(&subject) {
            out.entry(id).or_default().push((source, kind, value));
        }
    }
    Ok(out)
}

/// Whether a record is live (neither merged nor retired) and holds live tracks.
fn live_had(db: &Connection, record: i64) -> Result<bool> {
    let mut st = db.prepare_cached(
        "SELECT r.merged_into IS NULL AND r.retired_at IS NULL AND EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL AND (EXISTS (SELECT 1 FROM files f WHERE f.track = t.id AND f.trashed IS NULL) OR NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id))) FROM records r WHERE r.id = ?1",
    )?;
    Ok(st.query_row([record], |r| r.get(0)).optional()?.unwrap_or(false))
}

/// A subject's references: (source, kind, value).
fn refs_in(db: &Connection, subject: &str) -> Result<Vec<(String, String, String)>> {
    let mut st = db.prepare_cached("SELECT source, kind, value FROM \"references\" WHERE subject = ?1")?;
    let rows = st.query_map([subject], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Whether two records' references say they are two (`IDENTIFYING`): both hold one source's same kind, with no value in common.
fn contradicts(mine: &[(String, String, String)], theirs: &[(String, String, String)]) -> bool {
    fn values<'r>(refs: &'r [(String, String, String)], source: &str, kind: &str) -> Vec<&'r str> {
        refs.iter().filter(|(s, k, _)| s == source && k == kind).map(|(.., v)| v.as_str()).collect()
    }
    IDENTIFYING.iter().any(|(source, kind)| {
        let (a, b) = (values(mine, source, kind), values(theirs, source, kind));
        !a.is_empty() && !b.is_empty() && !a.iter().any(|v| b.contains(v))
    })
}

/// Whether a record of a name is an album of it as `albums_of` counts them (V3): had, or holding a MusicBrainz release or release
/// group. One not had holding neither (a Bandcamp or Discogs list's, a request's, a clash's) is none of its own: nothing tells it
/// apart from the others of its name, so it is never what makes a name several albums. Pure.
fn an_album(refs: &[(String, String, String)], had: bool) -> bool {
    had || refs.iter().any(|(s, k, _)| s == "musicbrainz" && (k == "release-group" || k == "release"))
}

/// What a record is as the had rule counts the albums of its name (U2, T10; RF1: one class for "had"). A had record is what its
/// album was built as; a record not had, what its list says, of the owner's rule only an album kind set apart from a single.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// A had record by the type its album was built with (RF1, `records.typed`: set by hand, an incomplete album's whole release, the
    /// rule), else, until a build typed it, by its own files with the rule (T10, `library::type_by_files`): a single.
    Single,
    /// The same, an EP.
    Ep,
    /// The same, any other type: an album, a broadcast.
    Album,
    /// A had record no build typed whose files the rule cannot place (a short one whose lengths are not read): of the class asked,
    /// either where the name holds an album of the other class too (`Albums::unsure`).
    Unknown,
    /// A record not had of kind album: set apart from a single by the rule.
    SaidAlbum,
    /// A record not had of kind single: one song in versions, an EP at most (the rule's point 5), so a single's or an EP's.
    SaidSingle,
    /// A record not had of no kind (a list that says none, a request's), of kind EP (MusicBrainz's EP covers a 2-track 12") or of
    /// any other: of either class.
    Either,
}

impl Class {
    /// A had record's, by the type its album was built with.
    fn built(kind: &str) -> Self {
        match kind.trim().to_ascii_lowercase().as_str() {
            "single" => Self::Single,
            "ep" => Self::Ep,
            _ => Self::Album,
        }
    }

    /// A record not had, by the kind its list says.
    fn said(kind: Option<&str>) -> Self {
        match kind.map(|k| k.trim().to_ascii_lowercase()).as_deref() {
            Some("album") => Self::SaidAlbum,
            Some("single") => Self::SaidSingle,
            _ => Self::Either,
        }
    }

    /// Its kind class (U2): a single's or not; none where it may be either.
    fn single(self) -> Option<bool> {
        match self {
            Self::Single | Self::SaidSingle => Some(true),
            Self::Ep | Self::Album | Self::SaidAlbum => Some(false),
            Self::Unknown | Self::Either => None,
        }
    }

    /// The kind classes a record not had of this class is held in by its names (RF1), in order: an album kind an album's alone; a
    /// single kind a single's, then an EP's (point 5); any other an album's, then a single's.
    fn asked(self) -> &'static [bool] {
        match self {
            Self::SaidAlbum => &[false],
            Self::SaidSingle => &[true, false],
            _ => &[false, true],
        }
    }

    /// Whether a record not had of this class may be held by a had one of class `holder`: a single kind never by an album (point 5:
    /// a Single said is an EP at most, however long).
    fn admits(self, holder: Class) -> bool {
        !(self == Self::SaidSingle && holder == Self::Album)
    }
}

/// A record as `albums_of` counts it: its id, its references, whether it is had, and its class (`classes_of`).
type Counted<'r> = (i64, &'r [(String, String, String)], bool, Class);

/// The albums of one name as a record of one kind class counts them (`albums_of`).
#[derive(Debug, Default, PartialEq)]
struct Albums {
    /// Each album's records.
    of: Vec<Vec<i64>>,
    /// Which of them (by place in `of`) say no kind where the name holds an album of the other class too: either class's, so
    /// each counts as two (`apart`).
    unsure: Vec<usize>,
}

impl Albums {
    /// How many albums of the name are not `r`'s own, one that may be of either class as two. No record's id is 0: `apart(0)`
    /// counts them all.
    fn apart(&self, r: i64) -> usize {
        self.of.iter().enumerate().filter(|(_, a)| !a.contains(&r)).map(|(i, _)| if self.unsure.contains(&i) { 2 } else { 1 }).sum()
    }

    /// The records of `h`'s album; none where `h` is in none (another class's, or no album of its own).
    fn with(&self, h: i64) -> &[i64] {
        self.of.iter().find(|a| a.contains(&h)).map_or(&[], Vec::as_slice)
    }
}

/// Records of one name as albums (X8, X9, V3, U2), as a record of the kind class `single` counts them:
/// - records holding one MusicBrainz release group or release are one album, its copies or editions; a had record holding
///   neither, one of its own (Y1: two untagged copies are two albums); one not had holding neither (a Bandcamp or Discogs list's,
///   a request's) none of its own (`an_album`, V3), left out;
/// - of one kind class (U2): an album its records' classes say is a single is no other album of an album's name, nor an album a
///   single's. A had record is what its album was built as (RF1, `classes_of`); a record not had of no kind or of kind EP may be
///   either, of the class asked whatever the name holds (RF1). An album of a had record no build typed and the rule cannot place
///   (its lengths not read yet) is of the class asked, but where the name holds an album of the other class too it may be either
///   (`unsure`);
/// - of two albums left, one had and one not, the one not had is the had one's where no record of it contradicts a had record of
///   it (U2): the had record already holds it by its names, as the had rule's names clause has it, so a list's record of the
///   name beside it is one album's, not a third. `contradicts` so says two only between a had record and one not had, never
///   between two had ones; a name holding three albums (Orbital's two self-titled albums beside an untagged copy) joins none.
///
/// Pure.
fn albums_of(records: &[Counted], single: bool) -> Albums {
    let mut albums: Vec<(std::collections::BTreeSet<(String, String)>, Vec<usize>)> = Vec::new();
    for (i, (_, refs, had, _)) in records.iter().enumerate() {
        if !an_album(refs, *had) {
            continue;
        }
        let mine: std::collections::BTreeSet<(String, String)> =
            refs.iter().filter(|(s, k, _)| s == "musicbrainz" && (k == "release-group" || k == "release")).map(|(_, k, v)| (k.clone(), v.to_ascii_lowercase())).collect();
        let (same, apart): (Vec<_>, Vec<_>) = albums.into_iter().partition(|(held, _)| !mine.is_disjoint(held));
        albums = apart;
        albums.push(same.into_iter().fold((mine, vec![i]), |(mut held, mut members), (theirs, of)| {
            held.extend(theirs);
            members.extend(of);
            (held, members)
        }));
    }
    // Each album's class as its records' classes say: none said, none known, either where only records not had say none (RF1);
    // both, the class asked.
    let class = |members: &[usize]| -> (Option<bool>, bool) {
        let known: Vec<bool> = members.iter().filter_map(|&i| records[i].3.single()).collect();
        let unknown = members.iter().any(|&i| records[i].3 == Class::Unknown);
        match (known.iter().any(|s| *s), known.iter().any(|s| !*s)) {
            (false, false) => (None, unknown),
            (true, false) => (Some(true), false),
            (false, true) => (Some(false), false),
            (true, true) => (Some(single), false),
        }
    };
    let classes: Vec<(Option<bool>, bool)> = albums.iter().map(|(_, m)| class(m)).collect();
    let other = classes.iter().any(|(c, _)| *c == Some(!single));
    let mut kept: Vec<(Vec<usize>, bool)> =
        albums.into_iter().zip(classes).filter(|(_, (c, _))| *c != Some(!single)).map(|((_, m), (_, unknown))| (m, other && unknown)).collect();
    let had_in = |members: &[usize]| members.iter().any(|&i| records[i].2);
    if let [(a, false), (b, false)] = &kept[..]
        && had_in(a) != had_in(b)
    {
        let (held, wanted) = if had_in(a) { (a, b) } else { (b, a) };
        if wanted.iter().all(|&w| held.iter().filter(|&&h| records[h].2).all(|&h| !contradicts(records[w].1, records[h].1))) {
            kept = vec![(held.iter().chain(wanted).copied().collect(), false)];
        }
    }
    Albums {
        unsure: kept.iter().enumerate().filter(|(_, (_, unsure))| *unsure).map(|(i, _)| i).collect(),
        of: kept.into_iter().map(|(members, _)| members.into_iter().map(|i| records[i].0).collect()).collect(),
    }
}

/// Each of these records' class (RF1, `Class`), `had` those of them that are (`live_had_of`): a had record by the type its album
/// was built with (`records.typed`, `Catalogue::keep_types`), whatever a list says of it; one no build typed yet by its own files,
/// with the release type rule itself (T10, `library::type_by_files`): its title, and its live tracks' lengths as their files read
/// them, unknown where the rule cannot place it (a short one whose lengths are not read yet). A record not had by the kind its list
/// says. In two queries.
fn classes_of(db: &Connection, records: &[i64], had: &std::collections::HashSet<i64>) -> Result<std::collections::HashMap<i64, Class>> {
    if records.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let mut st = db.prepare(&format!("SELECT id, kind, typed, COALESCE(title, '') FROM records WHERE id IN ({})", id_list(records)))?;
    let rows: Vec<(i64, Option<String>, Option<String>, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
    let lengths = lengths_of(db, &rows.iter().filter(|(id, _, typed, _)| had.contains(id) && typed.is_none()).map(|(id, ..)| *id).collect::<Vec<_>>())?;
    Ok(rows
        .into_iter()
        .map(|(id, kind, typed, title)| {
            let class = match (had.contains(&id), typed) {
                (true, Some(t)) => Class::built(&t),
                (true, None) => lengths.get(&id).and_then(|l| crate::serve::library::type_by_files(&title, l)).map_or(Class::Unknown, |t| Class::built(&t)),
                (false, _) => Class::said(kind.as_deref()),
            };
            (id, class)
        })
        .collect())
}

/// The had record of one name's records (`counted`, `r` among them; or no record's 0 for a list's row standing in) that holds `r` by
/// its names (U2, RF1): in the first kind class `r` is held in (`Class::asked`) where the name is no several albums besides its own
/// (`albums_of`), the oldest had record there that may hold it (`Class::admits`) whose album holds no record that contradicts it.
/// None where no class holds it. Pure.
fn held_by_names(counted: &[Counted], r: i64, asker: Class) -> Option<i64> {
    let mine = counted.iter().find(|c| c.0 == r).map_or(&[][..], |c| c.1);
    for &single in asker.asked() {
        let albums = albums_of(counted, single);
        if albums.apart(r) > 1 {
            continue;
        }
        // Held where nothing of the holder's album contradicts it: the had record, and a record its names joined to it (U2).
        let agrees = |h: i64| albums.with(h).iter().all(|m| *m == r || counted.iter().find(|c| c.0 == *m).is_none_or(|c| !contradicts(mine, c.1)));
        if let Some(h) = counted.iter().filter(|c| c.0 != r && c.2 && asker.admits(c.3)).map(|c| c.0).find(|h| !albums.with(*h).is_empty() && agrees(*h)) {
            return Some(h);
        }
    }
    None
}

/// The live tracks of these records where the record is live and had (each track as `live_had_of` has one hold its record), each
/// its length as its files read it (none not read yet), by record, in one query.
fn lengths_of(db: &Connection, records: &[i64]) -> Result<std::collections::HashMap<i64, Vec<Option<u64>>>> {
    let mut out: std::collections::HashMap<i64, Vec<Option<u64>>> = std::collections::HashMap::new();
    if records.is_empty() {
        return Ok(out);
    }
    let sql = format!(
        "SELECT t.record, (SELECT max(f.duration_ms) FROM files f WHERE f.track = t.id AND f.trashed IS NULL AND f.duration_ms > 0)
         FROM tracks t JOIN records r ON r.id = t.record
         WHERE t.record IN ({}) AND r.merged_into IS NULL AND r.retired_at IS NULL AND t.retired_at IS NULL
           AND (EXISTS (SELECT 1 FROM files f WHERE f.track = t.id AND f.trashed IS NULL) OR NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id))",
        id_list(records)
    );
    let mut st = db.prepare(&sql)?;
    for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)))? {
        let (record, ms) = row?;
        out.entry(record).or_default().push(ms.map(|m| m.max(0) as u64));
    }
    Ok(out)
}

/// Which record holds each record's files (R13, the one rule for "had": a request's record's, an artist's list's, C53a), each
/// record a merge's end; those had alone are in the answer, by record. The record itself where it holds live tracks; else the
/// oldest live, had record that holds its MusicBrainz release or release group (an album that arrived as a record of its own,
/// its group known since by its tags, a match or Lidarr); else the oldest live, had record of its artist with its title, both
/// folded, where no reference contradicts (`contradicts`), neither the holder's nor that of a record its album holds by names
/// (U2). Never by a name that is several albums' besides the record's own, records counted as X9 counts them (`albums_of`),
/// those not had only where they hold a MusicBrainz reference (`an_album`, V3), of its kind class, one not had the one had album
/// of the name holds by names in that album (U2), nor for a record made for a clash of names
/// (`shared_name_records`): which album the name means, nothing says, so a request of it stays Wanted (Y1). A retired record is
/// had the same way: by a live record of its group or of its names. For a whole list at once, in a few queries whatever its
/// length (C53a: an artist's page of hundreds of records asks once, not once a record); `names`, the live records by their
/// names, had or not, read once for every call that passes it.
fn holders_in(db: &Connection, records: &[i64], names: &mut FoldedNames) -> Result<std::collections::HashMap<i64, i64>> {
    use super::identity::fold;
    let mut out: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    let had = live_had_of(db, records)?;
    let rest: Vec<i64> = records.iter().copied().filter(|r| !had.contains(r)).collect();
    out.extend(had.into_iter().map(|r| (r, r)));
    if rest.is_empty() {
        return Ok(out);
    }
    let own = refs_of(db, &rest)?;
    let none: Vec<(String, String, String)> = Vec::new();
    let own_of = |r: i64| own.get(&r).unwrap_or(&none);
    // By a MusicBrainz release or release group a live, had record holds: the oldest of them.
    let values: std::collections::BTreeSet<String> = own.values().flatten().filter(|(s, k, _)| s == "musicbrainz" && (k == "release" || k == "release-group")).map(|(.., v)| v.clone()).collect();
    let mut holding: std::collections::HashMap<(String, String), Vec<i64>> = std::collections::HashMap::new();
    if !values.is_empty() {
        let sql = format!("SELECT subject, kind, value FROM \"references\" WHERE source = 'musicbrainz' AND kind IN ('release', 'release-group') AND value IN ({})", quoted_list(values));
        let rows: Vec<(String, String, String)> = db.prepare(&sql)?.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut ends: std::collections::HashMap<i64, Option<i64>> = std::collections::HashMap::new();
        for (subject, kind, value) in rows {
            let Some(id) = ids::record_number(&subject) else { continue };
            let end = match ends.get(&id) {
                Some(end) => *end,
                None => {
                    let end = merged_end(db, id)?;
                    ends.insert(id, end);
                    end
                }
            };
            if let Some(end) = end {
                holding.entry((kind, value)).or_default().push(end);
            }
        }
    }
    let candidates: Vec<i64> = holding.values().flatten().copied().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let live = live_had_of(db, &candidates)?;
    // A holder whose identifying references contradict the record's is another record (R26, N8): the standard edition, naming its
    // release, does not hold the files of the deluxe one asked for, though both hold their release group.
    let theirs_by_ref = refs_of(db, &candidates)?;
    let mut by_names: Vec<i64> = Vec::new();
    for &r in &rest {
        let agrees = |h: &i64| !contradicts(own_of(r), theirs_by_ref.get(h).map_or(&none[..], Vec::as_slice));
        let held = own_of(r).iter().filter(|(s, ..)| s == "musicbrainz").filter_map(|(_, k, v)| holding.get(&(k.clone(), v.clone()))).flatten().copied().filter(|h| *h != r && live.contains(h) && agrees(h)).min();
        match held {
            Some(h) => {
                out.insert(r, h);
            }
            None => by_names.push(r),
        }
    }
    // By its artist and title, folded, where nothing contradicts: the oldest had. Never a record made for a clash of names (X9, Y1),
    // as merged now: a merge points at its end (`merge_records_in`).
    if by_names.is_empty() {
        return Ok(out);
    }
    let named: Vec<(i64, String, String, bool)> = db
        .prepare(&format!(
            "SELECT r.id, COALESCE(r.artist, ''), COALESCE(r.title, ''), r.id IN (SELECT COALESCE(m.merged_into, m.id) FROM shared_name_records s JOIN records m ON m.id = s.record)
             FROM records r WHERE r.id IN ({})",
            id_list(&by_names)
        ))?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    if names.is_none() {
        let mut st = db.prepare_cached("SELECT id, COALESCE(artist, ''), COALESCE(title, '') FROM records WHERE merged_into IS NULL AND retired_at IS NULL ORDER BY id")?;
        let mut by_name: std::collections::HashMap<(String, String), Vec<i64>> = std::collections::HashMap::new();
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
            let (id, artist, title) = row?;
            by_name.entry((fold(&artist), fold(&title))).or_default().push(id);
        }
        *names = Some(by_name);
    }
    let Some(index) = names.as_ref() else { return Ok(out) };
    let keyed: Vec<(i64, (String, String))> =
        named.into_iter().filter(|(.., clashed)| !*clashed).map(|(id, artist, title, _)| (id, (fold(&artist), fold(&title)))).filter(|(_, (_, title))| !title.is_empty()).collect();
    // Every live record of those names, had or not: which are albums apart (X9), and which are had.
    let others: Vec<i64> = keyed.iter().filter_map(|(_, key)| index.get(key)).flatten().copied().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let theirs = refs_of(db, &others)?;
    let theirs_of = |h: i64| theirs.get(&h).map_or(&none[..], Vec::as_slice);
    let had_ones = live_had_of(db, &others)?;
    let classes = classes_of(db, &others.iter().copied().chain(keyed.iter().map(|(r, _)| *r)).collect::<Vec<_>>(), &had_ones)?;
    let class_of = |h: i64| classes.get(&h).copied().unwrap_or(Class::Either);
    for (r, key) in keyed {
        let same = index.get(&key).map(Vec::as_slice).unwrap_or_default();
        // Its name's albums of a kind class, its own among them (X9, U2): two besides its own, and its names cannot say which it is.
        // One not had that nothing tells apart is none (V3); one the had album holds by names is that album's (U2).
        let counted: Vec<Counted> =
            std::iter::once((r, own_of(r).as_slice(), false, class_of(r))).chain(same.iter().filter(|h| **h != r).map(|h| (*h, theirs_of(*h), had_ones.contains(h), class_of(*h)))).collect();
        if let Some(h) = held_by_names(&counted, r, class_of(r)) {
            out.insert(r, h);
        }
    }
    Ok(out)
}

/// The live had record each of these list rows (a reference, its artist and its title) is held by through its names, as the had
/// rule's names clause holds a record (`holders_in`): the oldest of its folded artist and title holding no reference that
/// contradicts the row's, nor does a record its album holds by names (U2), never where the name is several albums' besides the
/// row's own (Y1), albums of either class counted (RF1: a row says no kind). For display alone (Z6: Beyond's
/// `have`): nothing is made or tied. `names`, the live records by their names, read once for every call that passes it. In the
/// rows' order.
fn held_by_names_in(db: &Connection, rows: &[(Ref, &str, &str)], names: &mut FoldedNames) -> Result<Vec<Option<i64>>> {
    use super::identity::fold;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    if names.is_none() {
        let mut st = db.prepare_cached("SELECT id, COALESCE(artist, ''), COALESCE(title, '') FROM records WHERE merged_into IS NULL AND retired_at IS NULL ORDER BY id")?;
        let mut by_name: std::collections::HashMap<(String, String), Vec<i64>> = std::collections::HashMap::new();
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
            let (id, artist, title) = row?;
            by_name.entry((fold(&artist), fold(&title))).or_default().push(id);
        }
        *names = Some(by_name);
    }
    let Some(index) = names.as_ref() else { return Ok(vec![None; rows.len()]) };
    let keys: Vec<(String, String)> = rows.iter().map(|(_, artist, title)| (fold(artist), fold(title))).collect();
    let others: Vec<i64> = keys.iter().filter(|(_, title)| !title.is_empty()).filter_map(|key| index.get(key)).flatten().copied().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let theirs = refs_of(db, &others)?;
    let none: Vec<(String, String, String)> = Vec::new();
    let theirs_of = |h: i64| theirs.get(&h).map_or(&none[..], Vec::as_slice);
    let had = live_had_of(db, &others)?;
    let classes = classes_of(db, &others, &had)?;
    // The row stands in its name's albums as a record of its own would: no record's id is 0.
    const ROW: i64 = 0;
    Ok(rows
        .iter()
        .zip(&keys)
        .map(|((r, ..), key)| {
            if key.1.is_empty() {
                return None;
            }
            let same = index.get(key).map(Vec::as_slice).unwrap_or_default();
            let own = [(r.source.clone(), r.kind.clone(), r.value.clone())];
            let counted: Vec<Counted> = std::iter::once((ROW, &own[..], false, Class::Either))
                .chain(same.iter().map(|h| (*h, theirs_of(*h), had.contains(h), classes.get(h).copied().unwrap_or(Class::Either))))
                .collect();
            held_by_names(&counted, ROW, Class::Either)
        })
        .collect())
}

/// Records as the pages read them (R16, C53a), on the caller's connection: their rows, their references (a MusicBrainz release
/// group, a Bandcamp album page among them) in one query each, and who holds each one's files (`holders_in`). In id order.
pub(super) fn read_records_in(db: &Connection, records: &[i64], names: &mut FoldedNames) -> Result<Vec<Requested>> {
    if records.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<RecordRow> = db.prepare(&format!("SELECT {RECORD_COLUMNS} FROM records WHERE id IN ({}) ORDER BY id", id_list(records)))?.query_map([], row_of)?.collect::<rusqlite::Result<_>>()?;
    let mut refs = refs_of(db, records)?;
    let holders = holders_in(db, records, names)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let own = refs.remove(&row.id).unwrap_or_default();
            let groups = own.iter().filter(|(s, k, _)| s == "musicbrainz" && k == "release-group").map(|(.., v)| v.clone()).collect();
            let bandcamp = own.iter().find(|(s, k, _)| s == "bandcamp" && k == "album").map(|(.., v)| v.clone());
            Requested { holder: holders.get(&row.id).copied(), groups, bandcamp, refs: own, row, copies: Vec::new(), present: 0, list: None }
        })
        .collect())
}

/// What a record's holder holds of it, for its requests and fetchers (W4): the holder's present tracks in each catalogue folder (a
/// file of the track there, outside the trash: a copy in that library), all of them (a track no file holds counting as its own, as
/// the had rule has it), and how many tracks a whole copy holds: the record's counting list's, else its holder's, else the
/// MusicBrainz list kept for its holder (the release its files name, where no list counts: an album short of it). None: no list,
/// and any copy is whole.
fn holding_in(db: &Connection, record: i64, holder: i64) -> Result<(Vec<(i64, usize)>, usize, Option<usize>)> {
    let copies: Vec<(i64, usize)> = db
        .prepare_cached(
            "SELECT f.folder, count(DISTINCT t.id) FROM tracks t JOIN files f ON f.track = t.id
             WHERE t.record = ?1 AND t.retired_at IS NULL AND f.trashed IS NULL GROUP BY f.folder ORDER BY f.folder",
        )?
        .query_map([holder], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?
        .collect::<rusqlite::Result<_>>()?;
    let present: i64 = db
        .prepare_cached(concat!(
            "SELECT count(*) FROM tracks t WHERE t.record = ?1 AND t.retired_at IS NULL AND ",
            "(EXISTS (SELECT 1 FROM files f WHERE f.track = t.id AND f.trashed IS NULL) OR NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id))"
        ))?
        .query_row([holder], |r| r.get(0))?;
    let listed = |id: i64, source: &str| -> Result<Option<usize>> {
        let tracks: Option<String> = db.prepare_cached("SELECT tracks FROM reference_tracks WHERE record = ?1 AND source = ?2")?.query_row(params![id, source], |r| r.get(0)).optional()?;
        Ok(tracks.and_then(|t| serde_json::from_str::<Vec<serde_json::Value>>(&t).ok()).map(|l| l.len()).filter(|n| *n > 0))
    };
    let counted = |id: i64| -> Result<Option<usize>> {
        let source: Option<String> = db.prepare_cached("SELECT counting FROM records WHERE id = ?1")?.query_row([id], |r| r.get(0)).optional()?.flatten();
        match source {
            Some(source) => listed(id, &source),
            None => Ok(None),
        }
    };
    let list = match counted(record)? {
        Some(n) => Some(n),
        None => match counted(holder)? {
            Some(n) => Some(n),
            None => listed(holder, "musicbrainz")?,
        },
    };
    Ok((copies, present as usize, list))
}

/// Records not had as one list shows them (C53a, `rows` one artist's): a retired one gives way to a live record of its release
/// group (OQ16: that one is the group's record now); of the rest, an album once, by its title folded, where nothing contradicts:
/// a list naming it again (Bandcamp's album beside MusicBrainz's release group, with no link between the two) is no second row.
/// The live first, then the older.
fn once_each(db: &Connection, mut rows: Vec<Requested>) -> Result<Vec<Requested>> {
    use super::identity::fold;
    rows.sort_by_key(|q| (q.row.retired_at.is_some(), q.row.id));
    let mut kept: Vec<Requested> = Vec::new();
    for q in rows {
        if q.row.retired_at.is_some() && live_holds_group(db, &q)? {
            continue;
        }
        let title = fold(&q.row.title);
        if !title.is_empty() && kept.iter().any(|k| fold(&k.row.title) == title && !contradicts(&k.refs, &q.refs)) {
            continue;
        }
        kept.push(q);
    }
    Ok(kept)
}

/// Whether a live record other than this one holds one of its MusicBrainz release groups.
fn live_holds_group(db: &Connection, q: &Requested) -> Result<bool> {
    for g in &q.groups {
        for subject in subjects_in(db, &Ref::new("musicbrainz", "release-group", g))? {
            if let Some(id) = ids::record_number(&subject)
                && let Some(id) = merged_end(db, id)?
                && id != q.row.id
                && db.prepare_cached("SELECT retired_at IS NULL FROM records WHERE id = ?1")?.query_row([id], |r| r.get::<_, bool>(0))?
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// The record a release group names where a request asks for it (R13, F2): its oldest live, had holder; else its oldest live
/// one; a retired one only where no live record holds it (OQ16: a retired record stays its group's record not had).
/// The live record that holds a Bandcamp album page (W1), merges followed: a page is one record's alone (`put_reference`).
fn page_record_in(db: &Connection, page: &str) -> Result<Option<i64>> {
    for subject in subjects_in(db, &Ref::new("bandcamp", "album", page))? {
        if let Some(id) = ids::record_number(&subject)
            && let Some(id) = merged_end(db, id)?
            && db.prepare_cached("SELECT retired_at IS NULL FROM records WHERE id = ?1")?.query_row([id], |r| r.get::<_, bool>(0))?
        {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn group_record_in(tx: &Connection, group: &Ref) -> Result<Option<i64>> {
    let mut holders: Vec<(bool, bool, i64)> = Vec::new();
    for subject in subjects_in(tx, group)? {
        if let Some(id) = ids::record_number(&subject)
            && let Some(id) = merged_end(tx, id)?
        {
            let retired: bool = tx.prepare_cached("SELECT retired_at IS NOT NULL FROM records WHERE id = ?1")?.query_row([id], |r| r.get(0))?;
            holders.push((retired, !live_had(tx, id)?, id));
        }
    }
    Ok(holders.into_iter().min().map(|(.., id)| id))
}

impl Catalogue {
    /// The record a Want's release group names, else the one its Bandcamp page does (W1, as `request_record_in` takes them first),
    /// on a reader: whether the Want names a record already, so its album need not be read to name one. Blocking.
    pub fn record_named_by(&self, group: Option<&str>, page: Option<&str>) -> Result<Option<i64>> {
        let read = |db: &Connection| -> Result<Option<i64>> {
            if let Some(g) = group.map(str::to_ascii_lowercase).filter(|g| is_mbid(g))
                && let Some(record) = group_record_in(db, &Ref::new("musicbrainz", "release-group", &g))?
            {
                return Ok(Some(record));
            }
            match page {
                Some(page) => page_record_in(db, page),
                None => Ok(None),
            }
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// These records as requests read them (R16, F5), on one reader connection and never under the writer's lock, which a build
    /// holds for its whole record pass: each followed through its merges, then read as the pages read records (`read_records_in`:
    /// rows and references in one query each, holders by the had rule in a few). By the id asked; one not known is left out.
    /// Blocking.
    pub fn requested(&self, records: &[i64]) -> Result<std::collections::HashMap<i64, Requested>> {
        let read = |db: &Connection| -> Result<std::collections::HashMap<i64, Requested>> {
            let mut ends: Vec<(i64, i64)> = Vec::new();
            for &id in records {
                if let Some(end) = merged_end(db, id)? {
                    ends.push((id, end));
                }
            }
            let wanted: Vec<i64> = ends.iter().map(|(_, end)| *end).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            let mut names = None;
            let mut by_id: std::collections::HashMap<i64, Requested> = read_records_in(db, &wanted, &mut names)?.into_iter().map(|q| (q.row.id, q)).collect();
            // What each holder holds of it (W4): partly, in Incoming alone, or whole in a library.
            for q in by_id.values_mut() {
                if let Some(holder) = q.holder {
                    (q.copies, q.present, q.list) = holding_in(db, q.row.id, holder)?;
                }
            }
            Ok(ends.into_iter().filter_map(|(id, end)| Some((id, by_id.get(&end)?.clone()))).collect())
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// An artist's records not had, as their page lists them (C53a): every record credited to them, of the kinds albums and EPs
    /// or of none yet (a list that gives no type, Q32), that no record holds the files of (the one had rule, `holders_in`: by
    /// references first, then by names where nothing contradicts); a retired one as not had, with when it went (OQ16), unless a
    /// live record holds its release group; each album once (`once_each`). On one reader, never the writer's lock. Blocking.
    pub fn missing_of(&self, artist: i64) -> Result<Vec<Requested>> {
        let read = |db: &Connection| -> Result<Vec<Requested>> {
            let records: Vec<i64> = db
                .prepare_cached(
                    "SELECT DISTINCT a.record FROM record_artists a JOIN records r ON r.id = a.record
                     WHERE a.artist = ?1 AND r.merged_into IS NULL AND (r.kind IS NULL OR r.kind IN ('album', 'ep')) ORDER BY a.record",
                )?
                .query_map([artist], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let mut names = None;
            once_each(db, read_records_in(db, &records, &mut names)?.into_iter().filter(|q| q.holder.is_none()).collect())
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// Every record not had of the artists the library has albums by (C53b, Discover's missing list), but those with no list of
    /// their own (Z1: "Various Artists", MusicBrainz's "[unknown]"), each under the album artist it credits first, as their pages
    /// list them (`missing_of`'s rule: by the one had rule, a retired record per OQ16, each album once); and how many of those
    /// artists have a list read anywhere. On one reader, never the writer's lock; the rows in one query. Blocking.
    pub fn missing_everywhere(&self) -> Result<(Vec<(i64, Requested)>, usize)> {
        let read = |db: &Connection| -> Result<(Vec<(i64, Requested)>, usize)> {
            // The artists with albums here (one credited first on a live record that holds live tracks), but those with no list of
            // their own (`library_artists_in`, Z1).
            const HERE: &str = LIBRARY_ARTISTS;
            let here = library_artists_in(db)?;
            let rows: Vec<(i64, i64)> = db
                .prepare_cached(&format!(
                    "SELECT a.artist, a.record FROM record_artists a JOIN records r ON r.id = a.record
                     WHERE a.position = 0 AND r.merged_into IS NULL AND (r.kind IS NULL OR r.kind IN ('album', 'ep'))
                       AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL AND (EXISTS (SELECT 1 FROM files f WHERE f.track = t.id AND f.trashed IS NULL) OR NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id)))
                       AND a.artist IN ({HERE})
                     ORDER BY a.artist, a.record"
                ))?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let rows: Vec<(i64, i64)> = rows.into_iter().filter(|(artist, _)| here.binary_search(artist).is_ok()).collect();
            let read_lists: i64 = if here.is_empty() {
                0
            } else {
                let sql = format!("SELECT count(DISTINCT subject) FROM \"references\" WHERE kind = 'artist' AND read_at IS NOT NULL AND subject IN ({})", quoted_list(here.iter().map(|a| ids::artist(*a))));
                db.query_row(&sql, [], |r| r.get(0))?
            };
            let records: Vec<i64> = rows.iter().map(|(_, record)| *record).collect();
            let mut names = None;
            let mut read: std::collections::HashMap<i64, Requested> = read_records_in(db, &records, &mut names)?.into_iter().filter(|q| q.holder.is_none()).map(|q| (q.row.id, q)).collect();
            let mut by_artist: std::collections::BTreeMap<i64, Vec<Requested>> = std::collections::BTreeMap::new();
            for (artist, record) in rows {
                if let Some(q) = read.remove(&record) {
                    by_artist.entry(artist).or_default().push(q);
                }
            }
            let mut out = Vec::new();
            for (artist, rows) in by_artist {
                out.extend(once_each(db, rows)?.into_iter().map(|q| (artist, q)));
            }
            Ok((out, read_lists as usize))
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// An artist as their page reads them (C53a): their name, and what each source knows them by, with when their list there was
    /// read last (`read_at`). None: no such artist. On a reader. Blocking.
    pub fn artist_known(&self, artist: i64) -> Result<Option<(String, Vec<(Ref, ReferenceRow)>)>> {
        let read = |db: &Connection| -> Result<Option<(String, Vec<(Ref, ReferenceRow)>)>> {
            let Some(name) = db.prepare_cached("SELECT name FROM artists WHERE id = ?1")?.query_row([artist], |r| r.get::<_, String>(0)).optional()? else { return Ok(None) };
            let mut st = db.prepare_cached("SELECT source, kind, value, at, how, confirmed, read_at FROM \"references\" WHERE subject = ?1 ORDER BY source, kind, value")?;
            let refs = st
                .query_map([ids::artist(artist)], |r| Ok((Ref { source: r.get(0)?, kind: r.get(1)?, value: r.get(2)? }, ReferenceRow { at: r.get(3)?, how: r.get(4)?, confirmed: r.get(5)?, read_at: r.get(6)? })))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(Some((name, refs)))
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// An artist's list at a source read now (C53a): what that source knows them by dated, so it is read again only once old.
    /// Blocking.
    pub fn artist_list_read(&self, artist: i64, source: &str) -> Result<()> {
        self.db().execute(
            "UPDATE \"references\" SET read_at = max(COALESCE(read_at, 0), ?3) WHERE subject = ?1 AND source = ?2 AND kind = 'artist'",
            params![ids::artist(artist), source, now()],
        )?;
        Ok(())
    }

    /// A record by its id for life: had or not, retired or not, and where it was merged into another (G3), that one, so an id
    /// any page or list keeps answers for good. None: no such id. Blocking.
    pub fn record(&self, id: i64) -> Result<Option<RecordRow>> {
        let read = |db: &Connection| -> Result<Option<RecordRow>> {
            Ok(match merged_end(db, id)? {
                Some(id) => record_in(db, id)?,
                None => None,
            })
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// Records a list names, kept (C47): each found by its references first, else by `identity::same_listing` among the
    /// records of its name, else a record not had of its own, credited to `artist` where the list is one artist's, as is one
    /// found that no artist is credited on (Z3). Its references are kept, found `how`. Their ids, in the list's order. Blocking.
    pub fn keep_listing(&self, artist: Option<i64>, list: &[Listing], how: &str) -> Result<Vec<i64>> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let at = now();
        let kept = keep_listing_in(&tx, artist, list, how, at, Some(at))?;
        tx.commit()?;
        Ok(kept)
    }

    /// What references name here (C57: Beyond's rows), on one reader; nothing made or tied. Each album reference's record as merged
    /// now, as a row opened finds it (`found_by_reference`), never by a name, and those records read as the lists read them (the one
    /// had rule's holder); for one no record holds, the had record its artist and title point at, as the had rule's names clause
    /// holds a record (`held_by_names_in`, Z6), the names read once for all; each artist reference's artist, and whether the
    /// library has albums by them (`LIBRARY_ARTISTS`). Each album as (its reference, its artist, its title). Blocking.
    pub fn by_reference(&self, albums: &[(Ref, &str, &str)], artists: &[Ref]) -> Result<ByReference> {
        let read = |db: &Connection| -> Result<ByReference> {
            let mut records = Vec::with_capacity(albums.len());
            for (r, ..) in albums {
                records.push(found_by_reference(db, &Listing { refs: vec![r.clone()], ..Listing::default() })?);
            }
            let wanted: Vec<i64> = records.iter().flatten().copied().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            let mut names = None;
            let read: std::collections::HashMap<i64, Requested> = read_records_in(db, &wanted, &mut names)?.into_iter().map(|q| (q.row.id, q)).collect();
            let unheld: Vec<usize> = (0..albums.len()).filter(|&i| records[i].is_none()).collect();
            let rows: Vec<(Ref, &str, &str)> = unheld.iter().map(|&i| albums[i].clone()).collect();
            let mut named = vec![None; albums.len()];
            for (i, held) in unheld.into_iter().zip(held_by_names_in(db, &rows, &mut names)?) {
                named[i] = held;
            }
            let mut people = Vec::with_capacity(artists.len());
            for r in artists {
                let held = subjects_in(db, r)?.iter().find_map(|s| ids::artist_number(s));
                people.push(match held {
                    Some(artist) => Some((artist, db.prepare_cached(&format!("SELECT ?1 IN ({LIBRARY_ARTISTS})"))?.query_row([artist], |row| row.get::<_, bool>(0))?)),
                    None => None,
                });
            }
            Ok(ByReference { records, read, artists: people, named })
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// Two records found to be one (G3, OQ13 (a)): the one with copies keeps its id, else the older; the other answers through
    /// `merged_into` for good. The id kept. Blocking.
    #[cfg_attr(not(test), allow(dead_code))] // a tracklist read that shows two records are one (C51a)
    pub fn merge(&self, a: i64, b: i64) -> Result<i64> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let (a, b) = (merged_end(&tx, a)?.context("no such record")?, merged_end(&tx, b)?.context("no such record")?);
        // Into the one that holds tracks, else the live one (R17: `merge_records_in` refuses a retired record as the end), else the
        // older.
        let key = |id: i64| -> Result<(bool, bool, i64)> {
            let retired: bool = tx.query_row("SELECT retired_at IS NOT NULL FROM records WHERE id = ?1", [id], |r| r.get(0))?;
            Ok((!had(&tx, id)?, retired, id))
        };
        let into = if key(a)? <= key(b)? { a } else { b };
        merge_records_in(&tx, if into == a { b } else { a }, into)?;
        tx.commit()?;
        Ok(into)
    }

    /// `from` merged into `into`, as `merge` decides and does it (`merge_records_in`). Blocking.
    #[cfg_attr(not(test), allow(dead_code))] // a tracklist read that shows two records are one (C51a)
    pub fn merge_records(&self, from: i64, into: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        merge_records_in(&tx, from, into)?;
        tx.commit()?;
        Ok(())
    }

    /// What a subject is known by at each source, by source and kind. Blocking.
    pub fn references_of(&self, subject: &str) -> Result<Vec<(Ref, ReferenceRow)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT source, kind, value, at, how, confirmed, read_at FROM \"references\" WHERE subject = ?1 ORDER BY source, kind, value")?;
        let rows = st.query_map([subject], |r| {
            Ok((Ref { source: r.get(0)?, kind: r.get(1)?, value: r.get(2)? }, ReferenceRow { at: r.get(3)?, how: r.get(4)?, confirmed: r.get(5)?, read_at: r.get(6)? }))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A subject's reference, kept, and whether it is the subject's now. Found again, it stays as first found, but for a surer way
    /// (`files` over `link` over `search`, the hand over all). One a source gives to one subject alone (a release, an album page, an
    /// artist, a label) that another subject holds stays there, unless this is by hand: a link pasted names what it names, and the
    /// other lets go of it. Blocking.
    pub fn set_reference(&self, subject: &str, r: &Ref, how: &str) -> Result<bool> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let kept = put_reference(&tx, subject, r, how, now(), how == "hand")?.kept();
        tx.commit()?;
        Ok(kept)
    }

    /// `set_reference`, taking a reference another subject holds alone by its files or a guess: a certain match's release, as
    /// MusicBrainz matched the record's own tracks to it (where one release's files make two records, G1). A link pasted by
    /// hand stays where it is. Blocking.
    pub fn take_reference(&self, subject: &str, r: &Ref, how: &str) -> Result<bool> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let kept = put_reference(&tx, subject, r, how, now(), true)?.kept();
        tx.commit()?;
        Ok(kept)
    }

    /// Tests elsewhere: the catalogue's connection, to set what no call sets (a record's release date).
    #[cfg(test)]
    pub fn db_for_tests(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db()
    }

    /// How these records' files went (U7, T8): each file of their tracks, where it lies and when it came there, went (`gone_at`) or
    /// went into a trash (its token's time); and each of their tracks retired since, where its last file lay and when that went, as
    /// kept as it retired (`was_folder`, `was_gone`; else when it retired). A file gone a while holds its record for its day, then
    /// retires: what is read after is the track's. By record; a record with neither is not in the answer. On a reader. Blocking.
    pub fn went_of(&self, records: &[i64]) -> Result<std::collections::HashMap<i64, Vec<Went>>> {
        let read = |db: &Connection| -> Result<std::collections::HashMap<i64, Vec<Went>>> {
            let mut out: std::collections::HashMap<i64, Vec<Went>> = std::collections::HashMap::new();
            if records.is_empty() {
                return Ok(out);
            }
            let ids = id_list(records);
            let mut st = db.prepare(&format!("SELECT t.record, f.folder, f.came, f.gone_at, f.trashed FROM files f JOIN tracks t ON t.id = f.track WHERE t.record IN ({ids})"))?;
            for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, Option<String>>(4)?)))? {
                let (record, folder, came, gone, token) = row?;
                // A trash token begins with its time (`trash::token`).
                let trashed = token.map(|t| t.split('/').next().and_then(|t| t.parse().ok()).unwrap_or(0));
                out.entry(record).or_default().push(Went { folder, came, gone, trashed });
            }
            let mut st = db.prepare(&format!(
                "SELECT record, was_folder, COALESCE(was_gone, retired_at) FROM tracks WHERE record IN ({ids}) AND retired_at IS NOT NULL AND was_folder IS NOT NULL"
            ))?;
            for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))? {
                let (record, folder, at) = row?;
                out.entry(record).or_default().push(Went { folder, came: None, gone: Some(at), trashed: None });
            }
            Ok(out)
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// Tests elsewhere: one statement on the catalogue, as a test sets a shape no action makes (a file rewritten in place).
    #[cfg(test)]
    pub fn execute_for_tests(&self, sql: &str) {
        self.db().execute_batch(sql).unwrap();
    }

    /// Tests elsewhere: a copy of the record in catalogue folder `folder`, a file there behind each of its tracks (`copy-<record>/…`).
    #[cfg(test)]
    pub fn copy_for_tests(&self, record: i64, folder: i64) {
        self.db()
            .execute("INSERT INTO files (folder, rel, size, mtime, track) SELECT ?2, 'copy-' || ?1 || '/' || id || '.flac', 1, 1, id FROM tracks WHERE record = ?1", params![record, folder])
            .unwrap();
    }

    /// The other live records of each of these records' names (folded as the had rule folds them, `names_of`) with a present file in
    /// one of `folders`, by record, the oldest first: what an Incoming copy of a record kept nowhere else may be a copy of all the
    /// same, split from it only by missing tags (RF2). By the names each row keeps (Y12), a row not folded yet folded here and never
    /// written; on a reader, never the writer's lock: the Library and Health ask it of every copy waiting. Blocking.
    pub fn editions_of(&self, records: &[i64], folders: &[i64]) -> Result<std::collections::HashMap<i64, Vec<i64>>> {
        use super::identity::fold;
        if records.is_empty() || folders.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let read = |db: &Connection| -> Result<std::collections::HashMap<i64, Vec<i64>>> {
            let asked: Vec<(i64, String)> = db
                .prepare(&format!("SELECT id, COALESCE(artist, ''), COALESCE(title, '') FROM records WHERE id IN ({})", id_list(records)))?
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .into_iter()
                .filter(|(_, artist, title)| !fold(artist).is_empty() && !fold(title).is_empty())
                .map(|(id, artist, title)| (id, names_of(&artist, &title)))
                .collect();
            let keys: std::collections::BTreeSet<String> = asked.iter().map(|(_, names)| names.clone()).collect();
            let mut named: std::collections::HashMap<String, Vec<i64>> = std::collections::HashMap::new();
            if !keys.is_empty() {
                let sql = format!(
                    "SELECT id, names, COALESCE(artist, ''), COALESCE(title, '') FROM records WHERE (names IN ({}) OR names IS NULL) AND merged_into IS NULL AND retired_at IS NULL ORDER BY id",
                    quoted_list(keys.iter().cloned())
                );
                for row in db.prepare(&sql)?.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))? {
                    let (id, names, artist, title) = row?;
                    let names = names.unwrap_or_else(|| names_of(&artist, &title));
                    if keys.contains(&names) {
                        named.entry(names).or_default().push(id);
                    }
                }
            }
            let mut there = db.prepare(&format!(
                "SELECT EXISTS (SELECT 1 FROM tracks t JOIN files f ON f.track = t.id WHERE t.record = ?1 AND t.retired_at IS NULL AND {} AND f.folder IN ({}))",
                super::PRESENT_F,
                id_list(folders)
            ))?;
            let mut out = std::collections::HashMap::new();
            for (id, names) in asked {
                let mut others = Vec::new();
                for &other in named.get(&names).into_iter().flatten() {
                    if other != id && there.query_row([other], |r| r.get::<_, bool>(0))? {
                        others.push(other);
                    }
                }
                if !others.is_empty() {
                    out.insert(id, others);
                }
            }
            Ok(out)
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// Tests elsewhere: the type a build kept on a record (RF1, `keep_types`).
    #[cfg(test)]
    pub fn typed_for_tests(&self, record: i64) -> Option<String> {
        self.db().query_row("SELECT typed FROM records WHERE id = ?1", [record], |r| r.get(0)).unwrap()
    }

    /// Tests elsewhere: a record of this title with `tracks` tracks on it (none: a record not had), no file behind them. Its id.
    #[cfg(test)]
    pub fn record_for_tests(&self, title: &str, tracks: usize) -> i64 {
        let db = self.db();
        db.execute("INSERT INTO records (born, title, artist) VALUES (1, ?1, 'Glass Harbour')", [title]).unwrap();
        let record = db.last_insert_rowid();
        for n in 1..=tracks as i64 {
            db.execute("INSERT INTO tracks (born, record, disc, number) VALUES (1, ?1, 1, ?2)", params![record, n]).unwrap();
        }
        record
    }

    /// A subject's references of `source` found by guessing, confirmed once (0.14's "That's it"): they stay guesses, said to
    /// be right. How many. Blocking.
    pub fn confirm_reference(&self, subject: &str, source: &str) -> Result<usize> {
        Ok(self.db().execute("UPDATE \"references\" SET confirmed = 1 WHERE subject = ?1 AND source = ?2", params![subject, source])?)
    }

    /// 0.14's "Checked by hand": its own facts are the record's reference. Blocking.
    pub fn set_checked_by_hand(&self, record: i64, checked: bool) -> Result<()> {
        self.db().execute("UPDATE records SET checked_by_hand = ?2 WHERE id = ?1", params![record, checked])?;
        Ok(())
    }

    /// The records no reference speaks for (Health's references item), in one query: had, with none counting and not checked
    /// by hand, and no release its files name nor a link pasted. While the lists are read (`lists_read`: MusicBrainz switched
    /// on), a record whose certain match from 0.16 waits for its list (`lists_to_read`) waits off it: what 0.16 left off the
    /// item stays off until the list is read, and goes on it only where the list does not agree. With MusicBrainz off no list
    /// comes, and it is on the item (R7). Blocking.
    pub fn unreferenced(&self, lists_read: bool) -> Result<Vec<i64>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT r.id FROM records r
             WHERE r.retired_at IS NULL AND r.merged_into IS NULL AND r.counting IS NULL AND r.checked_by_hand = 0
               AND EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL AND (EXISTS (SELECT 1 FROM files f WHERE f.track = t.id AND f.trashed IS NULL) OR NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id)))
               AND NOT (?1 AND EXISTS (SELECT 1 FROM lists_to_read w WHERE w.record = r.id))
               AND NOT EXISTS (SELECT 1 FROM \"references\" f WHERE f.subject = 'al-' || r.id
                 AND (f.how = 'hand' OR (f.how = 'files' AND f.source = 'musicbrainz' AND f.kind = 'release')))
             ORDER BY r.id",
        )?;
        let rows = st.query_map([lists_read], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The subjects a reference names: one, for a release, an album page, an artist or a label; for a release group or a master,
    /// every record of it (G1). None: no subject known by it. Blocking.
    pub fn subject_of(&self, r: &Ref) -> Result<Vec<String>> {
        subjects_in(&self.db(), r)
    }

    /// Each source's tracklist of a record, as kept, by source. On a reader. Blocking.
    pub fn reference_tracks(&self, record: i64) -> Result<Vec<(String, Vec<RefTrack>)>> {
        let read = |db: &Connection| -> Result<Vec<(String, Vec<RefTrack>)>> {
            let mut st = db.prepare_cached("SELECT source, tracks FROM reference_tracks WHERE record = ?1 ORDER BY source")?;
            let rows: Vec<(String, String)> = st.query_map([record], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            rows.into_iter().map(|(source, tracks)| Ok((source, serde_json::from_str(&tracks)?))).collect()
        };
        // Z11: a record's page, which reads them, never waits on the writer.
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// Records waiting for their certain match's list (0.17's step, `lists_to_read`), those tried least first, then the oldest:
    /// (record, release), up to `n`. Blocking.
    pub fn lists_to_read(&self, n: usize) -> Result<Vec<(i64, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT record, release FROM lists_to_read ORDER BY tries, record LIMIT ?1")?;
        let rows = st.query_map([n as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A record's list read (or none to read): it waits no more. Blocking.
    pub fn list_read(&self, record: i64) -> Result<()> {
        self.db().execute("DELETE FROM lists_to_read WHERE record = ?1", [record])?;
        Ok(())
    }

    /// A question for a record's list failed: counted, and the record let go of at the `most`th, so a release that always
    /// fails lets the rest through and goes on Health's item. Whether it was let go of. Blocking.
    pub fn list_failed(&self, record: i64, most: i64) -> Result<bool> {
        let db = self.db();
        db.execute("UPDATE lists_to_read SET tries = tries + 1 WHERE record = ?1", [record])?;
        Ok(db.execute("DELETE FROM lists_to_read WHERE record = ?1 AND tries >= ?2", params![record, most])? > 0)
    }

    /// Tests elsewhere: a record waiting for its release's list, as 0.17's step leaves a certain match.
    #[cfg(test)]
    pub fn wait_for_list_for_tests(&self, record: i64, release: &str) {
        self.db().execute("INSERT INTO lists_to_read (record, release, at) VALUES (?1, ?2, 1)", params![record, release]).unwrap();
    }

    /// A source's tracklist of a record, in place of what it gave before. Blocking.
    pub fn set_reference_tracks(&self, record: i64, source: &str, tracks: &[RefTrack]) -> Result<()> {
        self.db().execute(
            "INSERT OR REPLACE INTO reference_tracks (record, source, tracks, at) VALUES (?1, ?2, ?3, ?4)",
            params![record, source, serde_json::to_string(tracks)?, now()],
        )?;
        Ok(())
    }

    /// A source's tracklist of a record let go of (D18): a link pasted by hand names another list than the one kept, read by the
    /// link when it is asked for (`references::tracklist`). Blocking.
    pub fn forget_reference_tracks(&self, record: i64, source: &str) -> Result<()> {
        self.db().execute("DELETE FROM reference_tracks WHERE record = ?1 AND source = ?2", params![record, source])?;
        Ok(())
    }

    /// Which source's tracklist counts for a record (none: none does). By hand, it is chosen, and taken back with none, so it is
    /// worked out again. Worked out, it never takes the place of one chosen by hand: the hand comes first (0.14). Whether it was
    /// written. Blocking.
    pub fn set_counting(&self, record: i64, source: Option<&str>, by_hand: bool) -> Result<bool> {
        let changed = if by_hand {
            self.db().execute("UPDATE records SET counting = ?2, counted_by_hand = ?3 WHERE id = ?1", params![record, source, source.is_some()])?
        } else {
            self.db().execute("UPDATE records SET counting = ?2 WHERE id = ?1 AND counted_by_hand = 0", params![record, source])?
        };
        Ok(changed > 0)
    }
}

// ---- 0.17's step (C44, `app::step_017`): what 0.16 knew of each record's references, into these tables once

/// 0.17's step's parts, in the order they run, as who keeps a release two records name goes by it (G1): a link pasted by hand;
/// a certain match, MusicBrainz having matched the record's own tracks to the release; what the files name, the older record
/// first; a likely match; MusicBrainz's verified Discogs links, before a Discogs search by names, which can find another
/// record's; the discographies (C48b), records not had found by the release groups the parts before kept; Bandcamp's pages,
/// a record not had's among them; 0.14's choices, which confirm what the parts before wrote; the requests (C49a), each given
/// its record. No part counts: a certain match's list is read after (`lists_to_read`), and counts where it agrees with the
/// files. A new part goes at the end, so a data folder that ran the parts before runs it in the same order as a new one.
pub const PARTS_017: [&str; 10] = ["hand", "certain", "files", "likely", "links", "discogs", "discography", "pages", "chosen", "requests"];

/// A part's mark in `meta`, holding its report: there, the part is done. One a part, so a data folder that ran an earlier
/// version of the step, with fewer parts, runs the ones it lacks.
fn mark_017(part: &str) -> String {
    format!("017:{part}")
}

/// A part's mark there: the part done.
fn marked_017(db: &Connection, mark: &str) -> rusqlite::Result<bool> {
    Ok(db.query_row("SELECT 1 FROM meta WHERE key = ?1", [mark], |_| Ok(())).optional()?.is_some())
}

/// The user data's table a part reads and reports: its rows, carried or listed by why not. `likely` and `chosen` read again
/// what `certain` and `hand` report.
fn table_017(part: &str) -> Option<&'static str> {
    Some(match part {
        "hand" => "record_references",
        "certain" => "album_matches",
        "links" => "mb_releases",
        "discogs" => "discogs_facts: albums",
        "discography" => "discography",
        "pages" => "bandcamp_found",
        "requests" => "request",
        _ => return None,
    })
}

/// The report's lists of references refused or taken, in every part that writes them.
const HELD: &str = "references: held by another record";
const RETIRED: &str = "references: a retired record's";
const TAKEN: &str = "references: taken from another record";
/// A link pasted that names nothing a reference can be (0.16 kept any link of the three sites).
const UNLINKED: &str = "links pasted: naming no reference";
/// A source confirmed in 0.14 of which the record holds no reference.
const UNCONFIRMED: &str = "confirmations: no reference";
/// A Bandcamp page kept by names that more than one record has.
const SEVERAL: &str = "bandcamp_found: several records";
/// A request's Bandcamp page the pages part found no record for: the requests part makes the record and carries it (R19).
const BY_REQUESTS: &str = "bandcamp_found: a request's, its record made by the requests part";

/// What 0.17's step carries, as the user data kept it (`app::step_017` reads it and makes these of it).
#[derive(Debug, Clone, Default)]
pub struct Carry {
    pub chosen: Vec<CarriedChoice>,
    pub matches: Vec<CarriedMatch>,
    pub discogs: Vec<CarriedDiscogs>,
    pub pages: Vec<CarriedPage>,
    pub links: Vec<CarriedLinks>,
    pub discographies: Vec<CarriedDiscography>,
    pub requests: Vec<CarriedRequest>,
    /// Each table's rows as read: the report's before and after alike, as nothing is taken out of it.
    pub read: Vec<(&'static str, u64)>,
    /// Rows read that carry nothing, by why ("<table>: <why>": nothing found, a miss, not read, ...): their keys, which the
    /// report lists under the part reading that table.
    pub noted: Vec<(&'static str, Vec<String>)>,
}

/// 0.14's choices of one album (`record_references`): the source chosen, "Checked by hand", each link pasted (as what it
/// names; `unlinked`, one that names nothing a reference can be), the sources whose guesses were confirmed.
#[derive(Debug, Clone, Default)]
pub struct CarriedChoice {
    pub album: String,
    pub counting: Option<String>,
    pub checked: bool,
    pub links: Vec<Ref>,
    pub unlinked: Vec<String>,
    pub confirmed: Vec<String>,
}

/// Matching's find (`album_matches`): its release and release group, certain or likely, and when.
#[derive(Debug, Clone)]
pub struct CarriedMatch {
    pub album: String,
    pub at: i64,
    pub release: String,
    pub group: Option<String>,
    pub certain: bool,
}

/// What Discogs said of an album (`discogs_facts`, `album:`): its master and its main release, and when.
#[derive(Debug, Clone)]
pub struct CarriedDiscogs {
    pub album: String,
    pub at: i64,
    pub master: Option<u64>,
    pub release: Option<u64>,
}

/// A page found on Bandcamp (`bandcamp_found`) by what it was kept by: a release group, else `key(artist)|key(album)`.
#[derive(Debug, Clone)]
pub struct CarriedPage {
    pub key: String,
    pub at: i64,
    pub page: String,
}

/// A request with no record yet (`request`, C49a): its id, its key (its release group, else its names' as Discover keyed them,
/// which `bandcamp_found` keeps a page by), its release group, its names as asked for, its year and cover, and when it was
/// asked.
#[derive(Debug, Clone, Default)]
pub struct CarriedRequest {
    pub id: i64,
    pub key: String,
    pub group: Option<String>,
    pub album: String,
    pub artist: String,
    pub year: Option<i32>,
    pub cover: Option<String>,
    pub at: i64,
}

/// What an album asked for is (C49a): its release group, its names (as `bandcamp::label_release` sorts a label's out), the label
/// that named it so, its year and cover, and a Bandcamp album page where one was found for it by its names.
#[derive(Debug, Clone, Default)]
pub struct Wanted {
    pub group: Option<String>,
    pub artist: String,
    /// Its credit split as the record pass splits a file's (W6: the library's rules), the first its artist; none, the credit whole.
    pub artists: Vec<String>,
    pub title: String,
    pub label: Option<String>,
    pub year: Option<i32>,
    pub cover: Option<String>,
    pub page: Option<String>,
}

/// The record an album asked for is, in the caller's transaction (C49a): the record its release group names (`group_record_in`:
/// the oldest live one, had first; a retired one only where no live record holds it, OQ16), else the live record that holds its
/// Bandcamp page (W1, merges followed), else the live record had of its names
/// as the import matcher finds an album (R14), else, with no group, the oldest live record not had of its names nothing contradicts,
/// else a new record not had, credited to its artist (`listing`). Its label, where a label named it, credited too (`listing`);
/// its Bandcamp album page, where one was found for it (by its names, or by its release group, R19), a reference (`search`), so
/// Soulseek can judge by Bandcamp's list (C51b). A request with a release group takes a record by its names only where nothing
/// the record holds contradicts that group (R21, N1): "Album (Remixes)" asked by its group is not the owned "Album", nor another
/// self-titled album. A name several live albums of the artist share, had or not had (X8: Orbital's two self-titled albums, Peter
/// Gabriel's four), finds none by its names: the request takes its own record. Records holding one MusicBrainz release group or
/// release are one album's copies or editions, no clash (X9); a later request by those names alone takes the record made for the
/// first, so two askers share one. What it came to (`RequestRecord`).
pub(super) fn request_record_in(tx: &Connection, w: &Wanted, at: i64) -> Result<RequestRecord> {
    use super::identity::fold;
    let group = w.group.as_deref().map(str::to_ascii_lowercase).filter(|g| is_mbid(g)).map(|g| Ref::new("musicbrainz", "release-group", &g));
    let by_group = match &group {
        Some(g) => group_record_in(tx, g)?,
        None => None,
    };
    // Else the live record that holds its Bandcamp page (W1): the page a card knows names the record its list showed.
    let by_page = match (by_group, w.page.as_deref()) {
        (None, Some(page)) => page_record_in(tx, page)?,
        _ => None,
    };
    // What it was asked by, which a record found by its names must not say is another's (R21, W2): its release group and its
    // Bandcamp page.
    let asked: Vec<(String, String, String)> =
        group.iter().map(|g| (g.source.clone(), g.kind.clone(), g.value.clone())).chain(w.page.iter().map(|p| ("bandcamp".to_string(), "album".to_string(), p.clone()))).collect();
    let agrees = |record: i64| -> Result<bool> { Ok(asked.is_empty() || !contradicts(&asked, &refs_in(tx, &ids::record(record))?)) };
    // Had by its names (W2, B1): the had rule's own, its artist and title folded alike (`identity::fold`, which keeps what is in
    // brackets), never loosely: "Tide Tables (Remixes)" is not the owned "Tide Tables". Never by a title alone, whoever's it is
    // (R14). Of one name, the oldest that nothing it was asked by contradicts.
    let by_names = || -> Result<Option<i64>> {
        let (artist, title) = (fold(&w.artist), fold(&w.title));
        if title.is_empty() {
            return Ok(None);
        }
        let mut st = tx.prepare_cached(
            "SELECT r.id, COALESCE(r.artist, ''), COALESCE(r.title, '') FROM records r
             WHERE r.merged_into IS NULL AND r.retired_at IS NULL AND EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL AND (EXISTS (SELECT 1 FROM files f WHERE f.track = t.id AND f.trashed IS NULL) OR NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id)))
             ORDER BY r.id",
        )?;
        let rows: Vec<(i64, String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        for (id, ..) in rows.into_iter().filter(|(_, a, t)| fold(a) == artist && fold(t) == title) {
            if agrees(id)? {
                return Ok(Some(id));
            }
        }
        Ok(None)
    };
    // With no release group, the oldest live record not had of its names (folded) that nothing contradicts (R14, F3): what
    // another request of the album made, so two requests of one album share its record, at the step and after.
    let not_had_by_names = || -> Result<Option<i64>> {
        let (artist, title) = (fold(&w.artist), fold(&w.title));
        if title.is_empty() {
            return Ok(None);
        }
        let mut st = tx.prepare_cached(
            "SELECT r.id, COALESCE(r.artist, ''), COALESCE(r.title, '') FROM records r
             WHERE r.merged_into IS NULL AND r.retired_at IS NULL AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL AND (EXISTS (SELECT 1 FROM files f WHERE f.track = t.id AND f.trashed IS NULL) OR NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id)))
             ORDER BY r.id",
        )?;
        let rows: Vec<(i64, String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        let mine: Vec<(String, String, String)> = w.page.iter().map(|p| ("bandcamp".to_string(), "album".to_string(), p.clone())).collect();
        for (id, ..) in rows.into_iter().filter(|(_, a, t)| fold(a) == artist && fold(t) == title) {
            if !contradicts(&mine, &refs_in(tx, &ids::record(id))?) {
                return Ok(Some(id));
            }
        }
        Ok(None)
    };
    // Its names several live albums of the artist share, had or not had (X8): exact folded names cannot say which. Records holding
    // one MusicBrainz release group or release are one album, its copies or editions (X9, `albums_of`, as the had rule counts them,
    // Y1); a had record holding neither, one of its own; one not had holding neither (a list's, a request's), none (V3); one not
    // had that the one had album holds by names, that album's; of an album's kind class, as a Want says no kind (U2).
    let shared_names = || -> Result<bool> {
        if fold(&w.title).is_empty() {
            return Ok(false);
        }
        // By the names each record keeps (Y12), not a fold of every live record.
        let named = live_named_in(tx, &names_of(&w.artist, &w.title))?;
        if named.len() < 2 {
            return Ok(false);
        }
        let (refs, had, none) = (refs_of(tx, &named)?, live_had_of(tx, &named)?, Vec::new());
        let classes = classes_of(tx, &named, &had)?;
        let counted: Vec<Counted> =
            named.iter().map(|id| (*id, refs.get(id).unwrap_or(&none).as_slice(), had.contains(id), classes.get(id).copied().unwrap_or(Class::Either))).collect();
        Ok(albums_of(&counted, false).apart(0) > 1)
    };
    let shared = by_group.or(by_page).is_none() && shared_names()?;
    // A request by its names alone whose names clash takes the record made for the first such request, while it is live (X9): two
    // accounts asking for "Orbital" share one record, not one each. One asked by its group or page takes its own, as X8 has it.
    let alone = group.is_none() && w.page.is_none();
    let names_key = names_of(&w.artist, &w.title);
    // Which, and whether it is an owned record found by its names, onto which nothing it was asked by is written (W2, B2).
    let (found, by_names, owned) = match by_group.or(by_page) {
        Some(record) => (Some(record), false, false),
        None if shared => (if alone { made_for_names_in(tx, &names_key)? } else { None }, false, false),
        None => match by_names()? {
            Some(record) => (Some(record), true, true),
            None if group.is_none() => {
                let record = not_had_by_names()?;
                (record, record.is_some(), false)
            }
            None => (None, false, false),
        },
    };
    let (record, made) = match found {
        Some(record) => (record, false),
        None => {
            // Credited as the record pass credits its files (W6): each artist its credit names, split by the library's rules, the first
            // its artist; a credit not split, whole.
            let names: Vec<String> = if w.artists.is_empty() { vec![w.artist.trim().to_string()] } else { w.artists.iter().map(|a| a.trim().to_string()).collect() };
            let names: Vec<String> = names.into_iter().filter(|n| !n.is_empty()).collect();
            let credited = super::artists::artists_in(tx, &names.iter().cloned().collect(), at)?;
            let artist = names.first().map(|n| credited[&ids::artist_key(n)]);
            let listing = Listing { title: w.title.clone(), artist: w.artist.clone(), year: w.year, cover: w.cover.clone(), refs: group.clone().into_iter().collect(), ..Listing::default() };
            let record = keep_listing_in(tx, artist, &[listing], "link", at, Some(at))?[0];
            for (position, name) in names.iter().enumerate().skip(1) {
                tx.prepare_cached("INSERT OR IGNORE INTO record_artists (record, artist, position, how) VALUES (?1, ?2, ?3, 'listing')")?.execute(params![record, credited[&ids::artist_key(name)], position as i64])?;
            }
            // The first request by these names alone to meet their clash: the next takes this record (X9).
            if shared && alone {
                tx.prepare_cached("INSERT OR REPLACE INTO shared_name_records (names, record) VALUES (?1, ?2)")?.execute(params![names_key, record])?;
            }
            (record, true)
        }
    };
    if let Some(label) = w.label.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        let id = super::labels::labels_in(tx, &std::collections::BTreeSet::from([label.to_string()]), at)?[&ids::artist_key(label)];
        tx.prepare_cached("INSERT OR IGNORE INTO record_labels (record, label, catno, how) VALUES (?1, ?2, '', 'listing')")?.execute(params![record, id])?;
    }
    let paged = match w.page.as_deref() {
        // Never onto an owned record found by its names (W2, B2): the page there would make another album had.
        Some(_) if owned => None,
        Some(page) => {
            let (r, subject) = (Ref::new("bandcamp", "album", page), ids::record(record));
            if subjects_in(tx, &r)?.contains(&subject) { None } else { Some(put_reference(tx, &subject, &r, "search", at, false)?.kept()) }
        }
        None => None,
    };
    Ok(RequestRecord { record, made, by_names, paged, shared })
}

/// A record's names as the clash of names compares them (X8, X9), and as its row keeps them (`records.names`, Y12): its artist and
/// title, each folded (`identity::fold`), as `artist|title`.
fn names_of(artist: &str, title: &str) -> String {
    use super::identity::fold;
    format!("{}|{}", fold(artist), fold(title))
}

/// The live records of `names` (`names_of`), oldest first, by the names each row keeps (Y12: an index, where X8's clash folded every
/// live record at each Want); a row not folded since it was made or renamed (`names` NULL) folded first. In the caller's transaction.
fn live_named_in(tx: &Connection, names: &str) -> Result<Vec<i64>> {
    let unfolded: Vec<(i64, String, String)> = tx
        .prepare_cached("SELECT id, COALESCE(artist, ''), COALESCE(title, '') FROM records WHERE names IS NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut keep = tx.prepare_cached("UPDATE records SET names = ?2 WHERE id = ?1")?;
    for (id, artist, title) in unfolded {
        keep.execute(params![id, names_of(&artist, &title)])?;
    }
    let mut st = tx.prepare_cached("SELECT id FROM records WHERE names = ?1 AND merged_into IS NULL AND retired_at IS NULL ORDER BY id")?;
    Ok(st.query_map([names], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

/// The record made for the first request by `names` alone that met a clash of names (X9), as merged now; none where it is retired
/// or gone.
fn made_for_names_in(tx: &Connection, names: &str) -> Result<Option<i64>> {
    let Some(record) = tx.prepare_cached("SELECT record FROM shared_name_records WHERE names = ?1")?.query_row([names], |r| r.get::<_, i64>(0)).optional()? else {
        return Ok(None);
    };
    let Some(end) = merged_end(tx, record)? else { return Ok(None) };
    let retired: Option<i64> = tx.prepare_cached("SELECT retired_at FROM records WHERE id = ?1")?.query_row([end], |r| r.get(0))?;
    Ok(retired.is_none().then_some(end))
}

/// What `request_record_in` came to: the record, whether it was made now, whether it was found by its names (R21: each listed in
/// the step's report), and what became of the page: put on it now (true), held by another record (false), or none (no page, or
/// the record held it already); and whether its names were shared by several records, so none was taken by them (X8: listed too).
pub(super) struct RequestRecord {
    pub record: i64,
    pub made: bool,
    pub by_names: bool,
    pub paged: Option<bool>,
    pub shared: bool,
}

/// How long a folder expected for a record waits for its album (`expect_record`).
const EXPECTED_SECS: i64 = 7 * 86_400;

/// The folders expected for records (C50), by (folder, the album's folder in it), those older than `EXPECTED_SECS` let go of
/// first, in the record pass's transaction. An album's is [`expected_of`].
pub(super) fn expected_in(tx: &Connection, now: i64) -> Result<std::collections::HashMap<(i64, String), i64>> {
    tx.execute("DELETE FROM expected WHERE at < ?1", [now - EXPECTED_SECS])?;
    let mut st = tx.prepare_cached("SELECT folder, dir, record FROM expected")?;
    let rows = st.query_map([], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The expectation an album's files are under (C51b, F9): its folder's expected directory, the files' own or any above them, so
/// the discs of a download (`CD1`, `CD2`) join the record expected for the download's folder. The first file's first. The
/// folder's own root is never one: a fetcher places a record in a directory of its own.
pub(super) fn expected_of(expected: &std::collections::HashMap<(i64, String), i64>, files: &[(i64, String)]) -> Option<((i64, String), i64)> {
    files.iter().find_map(|(folder, rel)| {
        let mut dir = super::dir_of(rel);
        while !dir.is_empty() {
            if let Some((key, record)) = expected.get_key_value(&(*folder, dir.to_string())) {
                return Some((key.clone(), *record));
            }
            dir = super::dir_of(dir);
        }
        None
    })
}

/// The record not had a new album is (C50), where it is one, held in turn: the record expected in its folder (a fetcher placed
/// it there for it, `expect_record`), else a record not had its files' release or release group names, else one of its names
/// whose kept tracklist agrees with its files (`identity::agrees`: names alone never join two records). Merges are followed, and
/// the oldest is taken where several are. Never a retired record: under OQ16 a retired record stays its group's record not
/// had, but an album that arrives is new tracks, and makes a record of its own, as before 0.17.
pub(super) fn wanted_record(tx: &Connection, a: &super::AlbumCopy, expected: Option<i64>, not_had: &mut FoldedNames) -> Result<Option<i64>> {
    use super::identity::{agrees, fold};
    // A record that can take an album's tracks: live, not retired, with none of its own.
    let open = |record: i64| -> Result<Option<i64>> {
        let Some(id) = merged_end(tx, record)? else { return Ok(None) };
        let free: bool = tx.query_row(
            "SELECT r.retired_at IS NULL AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL AND (EXISTS (SELECT 1 FROM files f WHERE f.track = t.id AND f.trashed IS NULL) OR NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id))) FROM records r WHERE r.id = ?1",
            [id],
            |r| r.get(0),
        )?;
        Ok(free.then_some(id))
    };
    if let Some(record) = expected
        && let Some(id) = open(record)?
    {
        return Ok(Some(id));
    }
    for (kind, value) in [("release", a.facts.release.as_deref()), ("release-group", a.group.as_deref())] {
        let Some(value) = value.filter(|v| is_mbid(v)) else { continue };
        let mut found: Vec<i64> = Vec::new();
        for subject in subjects_in(tx, &Ref::new("musicbrainz", kind, value))? {
            if let Some(record) = ids::record_number(&subject)
                && let Some(id) = open(record)?
            {
                found.push(id);
            }
        }
        if let Some(id) = found.into_iter().min() {
            return Ok(Some(id));
        }
    }
    // The records not had by their folded names, read once a pass (R16, F17); one an album took meanwhile is no longer open.
    if not_had.is_none() {
        let mut st = tx.prepare_cached(
            "SELECT r.id, COALESCE(r.artist, ''), COALESCE(r.title, '') FROM records r
             WHERE r.merged_into IS NULL AND r.retired_at IS NULL AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id) ORDER BY r.id",
        )?;
        let mut by_name: std::collections::HashMap<(String, String), Vec<i64>> = std::collections::HashMap::new();
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
            let (id, artist, title) = row?;
            by_name.entry((fold(&artist), fold(&title))).or_default().push(id);
        }
        *not_had = Some(by_name);
    }
    let named: Vec<i64> = if a.facts.title.is_empty() { Vec::new() } else { not_had.as_ref().and_then(|n| n.get(&(a.facts.artist.clone(), a.facts.title.clone()))).cloned().unwrap_or_default() };
    for id in named {
        if open(id)? != Some(id) {
            continue;
        }
        let lists: Vec<String> = {
            let mut st = tx.prepare_cached("SELECT tracks FROM reference_tracks WHERE record = ?1 ORDER BY source")?;
            st.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
        };
        if lists.iter().filter_map(|l| serde_json::from_str::<Vec<RefTrack>>(l).ok()).any(|l| agrees(&l, &a.facts.tracks)) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// Where 0.17's step keeps the record it gave each request (`requests`), in the part's transaction: the user data's own
/// (`request.record`) is written from it at every start where it is still empty, so a step cut off between the two gives it
/// at the next.
const REQUEST_RECORDS: &str = "requests_017";

/// One artist's discography (`discography`, C48b): their MusicBrainz id, when it was read, its release groups as listings, and
/// the names they may go by where nothing here names them yet: (name, how found), a related artist's first, then a match's.
#[derive(Debug, Clone, Default)]
pub struct CarriedDiscography {
    pub mbid: String,
    pub at: i64,
    pub listings: Vec<Listing>,
    pub names: Vec<(String, &'static str)>,
}

/// The words that join two artists in one credit ("A & B", "A feat. B"), lower case: a credit with one is no one artist's
/// name, and the artists it names are what lies between them (`credited_apart`).
pub const JOINS: [&str; 11] = [" & ", ", ", " feat. ", " feat ", " featuring ", " ft. ", " ft ", " with ", " x ", " vs. ", " vs "];

/// The artists a credit names, apart: split at the words that join two (`JOINS`), each as written. One artist's credit is
/// itself; a band whose name holds such a word ("Earth, Wind & Fire") splits too, so a caller pairs the parts with something
/// that counts them.
pub fn credited_apart(credit: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = credit.trim();
    loop {
        // ASCII lower case keeps every byte where it was, so a place found in it is the same place in `rest`.
        let lower = rest.to_ascii_lowercase();
        let Some((at, len)) = JOINS.iter().filter_map(|j| lower.find(j).map(|at| (at, j.len()))).min() else { break };
        out.push(rest[..at].trim().to_string());
        rest = rest[at + len..].trim_start();
    }
    out.push(rest.trim().to_string());
    out.retain(|n| !n.is_empty());
    out
}

/// What the files call each MusicBrainz artist id (R6): each file's album artists (the `albumartists` it lists, else its
/// `albumartist` apart, `credited_apart`) paired in order with its album artist ids, where there are as many of each, as the
/// record pass pairs them (`record_credits`); for each id, the name most files pair with it, the lower where as many. A
/// collaboration names each of its artists by their own id, never the two names together, and a file whose names and ids do
/// not pair names no one.
fn files_artists(tx: &Connection) -> Result<std::collections::HashMap<String, String>> {
    let mut st = tx.prepare(
        "SELECT g.folder, g.rel, g.name, g.value FROM tags g JOIN files f ON f.folder = g.folder AND f.rel = g.rel
         WHERE g.name IN ('albumartist', 'albumartists', 'musicbrainz_albumartistid') AND f.folder >= 0 AND f.gone_at IS NULL AND f.trashed IS NULL
         ORDER BY g.folder, g.rel, g.n",
    )?;
    let rows: Vec<(i64, String, String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut said: std::collections::BTreeMap<String, std::collections::BTreeMap<String, usize>> = std::collections::BTreeMap::new();
    let mut pair = |artist: Option<&str>, artists: &[&str], ids: &[String]| {
        let names: Vec<String> = if artists.is_empty() { artist.map(credited_apart).unwrap_or_default() } else { artists.iter().map(|a| a.trim().to_string()).collect() };
        if names.len() == ids.len() {
            for (id, name) in ids.iter().zip(names) {
                if is_mbid(id) {
                    *said.entry(id.clone()).or_default().entry(name).or_default() += 1;
                }
            }
        }
    };
    let mut i = 0;
    while i < rows.len() {
        let file = (rows[i].0, rows[i].1.as_str());
        let (mut artist, mut artists, mut ids): (Option<&str>, Vec<&str>, Vec<String>) = (None, Vec::new(), Vec::new());
        while i < rows.len() && (rows[i].0, rows[i].1.as_str()) == file {
            let (_, _, name, value) = &rows[i];
            match name.as_str() {
                "albumartist" => artist = artist.or(Some(value.as_str())),
                "albumartists" => artists.push(value.as_str()),
                _ => ids.extend(value.split([';', '/', ',']).map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty())),
            }
            i += 1;
        }
        pair(artist, &artists, &ids);
    }
    Ok(said.into_iter().filter_map(|(id, names)| Some((id, names.into_iter().max_by(|(a, x), (b, y)| x.cmp(y).then(b.cmp(a)))?.0))).collect())
}

/// A discography whose artist's name is held by an artist with another MusicBrainz id (N13): two artists of one name, which
/// nothing tells apart yet.
const SHARED_NAME: &str = "discography: a name two ids share";

/// MusicBrainz's verified Discogs links of a release (`mb_releases`), and when it was read.
#[derive(Debug, Clone)]
pub struct CarriedLinks {
    pub release: String,
    pub at: i64,
    pub discogs: Vec<Ref>,
}

/// One part's writes in its transaction, and what its report says of them.
struct Carrying<'t> {
    tx: &'t Connection,
    /// References another record holds alone, not taken: "<subject> <source>:<kind>:<value>".
    refused: Vec<String>,
    /// Those a retired record would have held: "<subject> <source>:<kind>:<value>".
    retired: Vec<String>,
    /// Those taken from another record: "<subject> <source>:<kind>:<value> from <other>".
    taken: Vec<String>,
    /// What found no record or carried nothing, by the report's list.
    unmatched: std::collections::BTreeMap<&'static str, Vec<String>>,
    /// The rows of the part's table carried.
    moved: u64,
    /// The report's tables beside the part's own: (name, rows, carried).
    tables: Vec<(&'static str, u64, u64)>,
    /// What the report counts beside its lists.
    counts: Vec<(&'static str, u64)>,
    /// What the report lists whole beside its samples (the requests part's).
    lists: Vec<(&'static str, Vec<String>)>,
}

impl Carrying<'_> {
    /// The record an album id names, one of the catalogue's, retired or not.
    fn found(&self, album: &str) -> Result<Option<i64>> {
        let Some(id) = ids::record_number(album) else { return Ok(None) };
        Ok(self.tx.prepare_cached("SELECT id FROM records WHERE id = ?1")?.query_row([id], |r| r.get(0)).optional()?)
    }

    /// `found`; none listed under `table`.
    fn record(&mut self, table: &'static str, album: &str) -> Result<Option<i64>> {
        let found = self.found(album)?;
        if found.is_none() {
            self.unmatched.entry(table).or_default().push(album.to_string());
        }
        Ok(found)
    }

    /// `put_reference`, its refusals and takes listed.
    fn put(&mut self, record: i64, r: &Ref, how: &str, at: i64, takes: bool) -> Result<bool> {
        let subject = ids::record(record);
        Ok(match put_reference(self.tx, &subject, r, how, at, takes)? {
            Put::Kept(taken) => {
                self.taken.extend(taken.into_iter().map(|from| format!("{} from {from}", named(&subject, r))));
                true
            }
            Put::Held => {
                self.refused.push(named(&subject, r));
                false
            }
            Put::Retired => {
                self.retired.push(named(&subject, r));
                false
            }
        })
    }

    /// The live records a reference names, by their numbers: a retired one holds no release or page.
    fn holders(&self, r: &Ref) -> Result<Vec<i64>> {
        let mut out = Vec::new();
        for subject in subjects_in(self.tx, r)? {
            if let Some(id) = ids::record_number(&subject)
                && !retired(self.tx, &subject)?
            {
                out.push(id);
            }
        }
        Ok(out)
    }

    /// 1. A link pasted by hand names what it names, whoever held it but another hand: two of 0.16's on one release go by the
    ///    records' age, the older keeping it (R11), and one pasted in 0.17 since a step that ran before stays where it was put.
    fn hand(&mut self, c: &Carry, now: i64) -> Result<()> {
        let (mut links, mut put) = (0, 0);
        self.unmatched.entry(UNLINKED).or_default();
        let mut chosen: Vec<(i64, &CarriedChoice)> = Vec::new();
        for choice in &c.chosen {
            if let Some(record) = self.record("record_references", &choice.album)? {
                chosen.push((record, choice));
            }
        }
        chosen.sort_by_key(|(record, _)| *record);
        for (record, choice) in chosen {
            self.moved += 1;
            for link in &choice.links {
                let subject = ids::record(record);
                let by_hand: Option<String> = self
                    .tx
                    .prepare_cached("SELECT subject FROM \"references\" WHERE source = ?1 AND kind = ?2 AND value = ?3 AND subject <> ?4 AND how = 'hand' LIMIT 1")?
                    .query_row(params![link.source, link.kind, link.value, subject], |r| r.get(0))
                    .optional()?;
                if by_hand.is_some_and(|other| ONE_SUBJECT.contains(&link.kind.as_str()) && !retired(self.tx, &other).unwrap_or(false)) {
                    self.refused.push(named(&subject, link));
                    continue;
                }
                self.put(record, link, "hand", now, true)?;
            }
            links += (choice.links.len() + choice.unlinked.len()) as u64;
            put += choice.links.len() as u64;
            self.unmatched.entry(UNLINKED).or_default().extend(choice.unlinked.iter().map(|url| format!("{} {url}", choice.album)));
        }
        self.tables.push(("links pasted", links, put));
        Ok(())
    }

    /// Matching's finds of the records there are, the older record first (the lower id, as ids are given in turn); `listed`,
    /// those of no record listed and the rest counted carried.
    fn matched<'c>(&mut self, c: &'c Carry, listed: bool) -> Result<Vec<(i64, &'c CarriedMatch)>> {
        let mut out = Vec::new();
        for m in &c.matches {
            let record = if listed { self.record("album_matches", &m.album)? } else { self.found(&m.album)? };
            if let Some(record) = record {
                out.push((record, m));
            }
        }
        if listed {
            self.moved = out.len() as u64;
        }
        out.sort_by_key(|(record, _)| *record);
        Ok(out)
    }

    /// 2. and 4. Certain matches, or likely ones: each its release and release group, the files' word where they name it too.
    /// A certain match takes its release from what the files or a guess gave another record; two on one release, the older
    /// record keeps it, and one refused it (a hand's, a retired record) lets the next try. A record a certain match's release
    /// is now waits for its list (`lists_to_read`): 0.16 kept none, and the match counts nothing until it agrees.
    fn matches(&mut self, c: &Carry, certain: bool) -> Result<()> {
        let mut waiting = 0;
        let files = files_named(self.tx)?;
        let say: std::collections::HashSet<(i64, &str, &str)> = files.iter().map(|(r, k, v)| (*r, *k, v.as_str())).collect();
        let mut kept: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (record, m) in self.matched(c, certain)?.into_iter().filter(|(_, m)| m.certain == certain) {
            let how = |kind: &str, value: &str| if say.contains(&(record, kind, value)) { "files" } else { "search" };
            let release = Ref::new("musicbrainz", "release", &m.release);
            if kept.contains(m.release.as_str()) {
                self.refused.push(named_ref(record, &release));
            } else if self.put(record, &release, how("release", &m.release), m.at, m.certain)? && m.certain {
                kept.insert(&m.release);
                waiting += self.tx.execute(
                    "INSERT OR IGNORE INTO lists_to_read (record, release, at) SELECT ?1, ?2, ?3
                     WHERE NOT EXISTS (SELECT 1 FROM reference_tracks WHERE record = ?1 AND source = 'musicbrainz')",
                    params![record, m.release, m.at],
                )? as u64;
            }
            if let Some(group) = &m.group {
                self.put(record, &Ref::new("musicbrainz", "release-group", group), how("release-group", group), m.at, false)?;
            }
        }
        if certain {
            self.counts = vec![("lists_to_read", waiting)];
        }
        Ok(())
    }

    /// 3. What the files name, the older record first.
    fn files(&mut self, now: i64) -> Result<()> {
        let files = files_named(self.tx)?;
        let mut put = 0;
        for (record, kind, value) in &files {
            put += u64::from(self.put(*record, &Ref::new("musicbrainz", kind, value), "files", now, false)?);
        }
        self.tables.push(("files: musicbrainz", files.len() as u64, put));
        Ok(())
    }

    /// 5. MusicBrainz's verified links: the Discogs master and release of the records its release is, as the parts above
    ///    left them.
    fn links(&mut self, c: &Carry) -> Result<()> {
        for l in &c.links {
            let holders = self.holders(&Ref::new("musicbrainz", "release", &l.release))?;
            if holders.is_empty() {
                self.unmatched.entry("mb_releases").or_default().push(l.release.clone());
                continue;
            }
            for record in holders {
                for r in &l.discogs {
                    self.put(record, r, "link", l.at, false)?;
                }
            }
            self.moved += 1;
        }
        Ok(())
    }

    /// 6. Discogs' facts: its master and main release, found by the record's names or MusicBrainz's link (`search`): what a
    ///    verified link gave a record stays its.
    fn discogs(&mut self, c: &Carry) -> Result<()> {
        for d in &c.discogs {
            let Some(record) = self.record("discogs_facts: albums", &d.album)? else { continue };
            for (kind, id) in [("master", d.master), ("release", d.release)] {
                if let Some(id) = id {
                    self.put(record, &Ref::new("discogs", kind, &id.to_string()), "search", d.at, false)?;
                }
            }
            self.moved += 1;
        }
        Ok(())
    }

    /// 7. The discographies (C48b): each artist's albums and EPs as MusicBrainz listed them become records (`keep_listing_in`):
    ///    one a record holds already by its release group found again, another a record not had, credited to the artist and
    ///    found `link`. The artist is named by their MusicBrainz reference, else as their files call them, else by a related
    ///    artist's name or a match's credit, which then keeps that id as their reference; read when the discography was.
    ///    A name its artist holds another MusicBrainz id for is two artists of one name: listed, and no records made for it,
    ///    until same-name artists are told apart.
    fn discography(&mut self, c: &Carry) -> Result<()> {
        let files = files_artists(self.tx)?;
        self.unmatched.entry(SHARED_NAME).or_default();
        let records = |tx: &Connection| -> Result<u64> { Ok(tx.query_row("SELECT count(*) FROM records", [], |r| r.get::<_, i64>(0))? as u64) };
        let (before, mut found, mut listed) = (records(self.tx)?, 0, 0);
        for d in &c.discographies {
            let reference = Ref::new("musicbrainz", "artist", &d.mbid);
            let known = subjects_in(self.tx, &reference)?.iter().find_map(|s| s.strip_prefix(ids::ARTIST)?.parse::<i64>().ok());
            let artist = match known {
                Some(id) => id,
                None => {
                    let Some((name, how)) = files.get(&d.mbid).map(|name| (name.clone(), "files")).or_else(|| d.names.first().map(|(n, how)| (n.clone(), *how))) else {
                        self.unmatched.entry("discography").or_default().push(d.mbid.clone());
                        continue;
                    };
                    let id = super::artists::artists_in(self.tx, &std::collections::BTreeSet::from([name.clone()]), d.at)?[&ids::artist_key(&name)];
                    let other: Option<String> = self
                        .tx
                        .prepare_cached("SELECT value FROM \"references\" WHERE subject = ?1 AND source = 'musicbrainz' AND kind = 'artist' AND value <> ?2 LIMIT 1")?
                        .query_row(params![ids::artist(id), d.mbid], |r| r.get(0))
                        .optional()?;
                    if other.is_some() {
                        self.unmatched.entry(SHARED_NAME).or_default().push(format!("{} {name}", d.mbid));
                        continue;
                    }
                    if put_reference(self.tx, &ids::artist(id), &reference, how, d.at, false)? == Put::Held {
                        self.refused.push(named(&ids::artist(id), &reference));
                    }
                    id
                }
            };
            self.moved += 1;
            let name: String = self.tx.query_row("SELECT name FROM artists WHERE id = ?1", [artist], |r| r.get(0))?;
            let listings: Vec<Listing> = d.listings.iter().map(|l| Listing { artist: name.clone(), ..l.clone() }).collect();
            let made = records(self.tx)?;
            // Seen by nobody since 0.17 (RT01): its clock starts at the first forgetting, not at 0.16's read.
            let kept = keep_listing_in(self.tx, Some(artist), &listings, "link", d.at, None)?;
            found += kept.len() as u64 - (records(self.tx)? - made);
            listed += kept.len() as u64;
            self.tx.execute(
                "UPDATE \"references\" SET read_at = max(COALESCE(read_at, 0), ?2) WHERE subject = ?1 AND source = 'musicbrainz' AND kind = 'artist'",
                params![ids::artist(artist), d.at],
            )?;
        }
        self.counts = vec![("records_not_had", records(self.tx)? - before), ("records_found", found), ("listed", listed)];
        Ok(())
    }

    /// 8. Bandcamp's pages: where exactly one live record is what a page was kept by, its release group or its names. By names,
    ///    the records had come first: 0.16 kept a page by names for an album it had, and a discography's record not had of the
    ///    same names (C48b) is another edition, which MusicBrainz lists apart.
    fn pages(&mut self, c: &Carry) -> Result<()> {
        let mut names: std::collections::HashMap<String, (Vec<i64>, Vec<i64>)> = std::collections::HashMap::new();
        {
            let mut st = self.tx.prepare(
                "SELECT r.id, COALESCE(r.artist, ''), COALESCE(r.title, ''), EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL AND (EXISTS (SELECT 1 FROM files f WHERE f.track = t.id AND f.trashed IS NULL) OR NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id)))
                 FROM records r WHERE r.merged_into IS NULL AND r.retired_at IS NULL ORDER BY r.id",
            )?;
            for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, bool>(3)?)))? {
                let (id, artist, title, had) = row?;
                let (all, had_ones) = names.entry(format!("{}|{}", cratarr_rules::names::key(&artist), cratarr_rules::names::key(&title))).or_default();
                all.push(id);
                if had {
                    had_ones.push(id);
                }
            }
        }
        self.unmatched.entry(SEVERAL).or_default();
        let requested: std::collections::HashSet<&str> = c.requests.iter().map(|r| r.key.as_str()).collect();
        for p in &c.pages {
            let records = if is_mbid(&p.key) {
                self.holders(&Ref::new("musicbrainz", "release-group", &p.key))?
            } else {
                names.get(&p.key).map(|(all, had)| if had.is_empty() { all.clone() } else { had.clone() }).unwrap_or_default()
            };
            match records[..] {
                [record] => {
                    self.put(record, &Ref::new("bandcamp", "album", &p.page), "search", p.at, false)?;
                    self.moved += 1;
                }
                // A request's, its record made by the requests part after, which carries it (R19, F13): listed so, not as left over.
                [] if requested.contains(p.key.as_str()) => self.unmatched.entry(BY_REQUESTS).or_default().push(p.key.clone()),
                [] => self.unmatched.entry("bandcamp_found").or_default().push(p.key.clone()),
                _ => self.unmatched.entry(SEVERAL).or_default().push(p.key.clone()),
            }
        }
        Ok(())
    }

    /// 10. The requests (C49a): each its record (`request_record_in`), its names as a label's account named them sorted out
    ///     (S12, `bandcamp::label_release`), and where it has no release group, the Bandcamp page `bandcamp_found` kept by its
    ///     names: a page the pages part found no record for, as the record is made only here.
    fn requests(&mut self, c: &Carry) -> Result<()> {
        // By key, a release group's lower-cased as references keep it.
        let lower = |key: &str| if is_mbid(key) { key.to_ascii_lowercase() } else { key.to_string() };
        let pages: std::collections::HashMap<String, &str> = c.pages.iter().map(|p| (lower(&p.key), p.page.as_str())).collect();
        let mut given: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
        let (mut made, mut paged) = (0, 0);
        let (mut made_for, mut found_by_names, mut look_alikes, mut shared_names) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut covers_dropped = Vec::new();
        // The owned albums the import matcher finds loosely (W2, B1): no request takes one any more, exact folded names alone do;
        // each request one would have been, listed whole for the rehearsal to check.
        let owned: Vec<(i64, String, String)> = self
            .tx
            .prepare(
                "SELECT r.id, COALESCE(r.artist, ''), COALESCE(r.title, '') FROM records r
                 WHERE r.merged_into IS NULL AND r.retired_at IS NULL AND EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL AND (EXISTS (SELECT 1 FROM files f WHERE f.track = t.id AND f.trashed IS NULL) OR NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id)))
                 ORDER BY r.id DESC",
            )?
            .query_map([], |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut loosely = crate::app::import::Matcher::default();
        for (id, artist, title) in &owned {
            loosely.add_album(&id.to_string(), title, Some(artist));
        }
        for r in &c.requests {
            let named = crate::bandcamp::label_release(&r.artist, &r.album, None);
            // Its page as `bandcamp_found` kept it, by its names or by its release group (R19, F13): where its record is made only
            // here, the pages part found none to give it to. Counted where it is put now; one another record holds is reported so.
            let page = pages.get(&lower(&r.key)).or_else(|| r.group.as_deref().and_then(|g| pages.get(&lower(g)))).map(|p| p.to_string());
            // A 0.16 Want's cover that fails W6's check, any address then, is dropped (O3), each listed.
            let cover = r.cover.clone().filter(|c| {
                let kept = crate::app::requests::cover_kept(c);
                if !kept {
                    covers_dropped.push(format!("{} {c}", r.id));
                }
                kept
            });
            let w = Wanted { group: r.group.clone(), artist: named.artist, artists: Vec::new(), title: named.title, label: named.label, year: r.year, cover, page };
            let RequestRecord { record, made: new, by_names, paged: mut page, shared } = request_record_in(self.tx, &w, r.at)?;
            if shared {
                shared_names.push(format!("{} {} - {}", r.id, r.artist, r.album));
            }
            // Else by a release group its record holds (R22, the owner's GROUCH request: 0.16 found its page by the group its card
            // knew, and it was asked for by Bandcamp's names).
            let mut put = w.page.clone();
            if put.is_none() {
                let groups: Vec<String> = refs_in(self.tx, &ids::record(record))?.into_iter().filter(|(s, k, _)| s == "musicbrainz" && k == "release-group").map(|(.., v)| v).collect();
                if let Some(found) = groups.iter().find_map(|g| pages.get(g)) {
                    let (bc, subject) = (Ref::new("bandcamp", "album", found), ids::record(record));
                    if !subjects_in(self.tx, &bc)?.contains(&subject) {
                        page = Some(put_reference(self.tx, &subject, &bc, "search", r.at, false)?.kept());
                        put = Some(found.to_string());
                    }
                }
            }
            made += u64::from(new);
            if by_names {
                let (artist, title): (String, String) = self.tx.query_row("SELECT COALESCE(artist, ''), COALESCE(title, '') FROM records WHERE id = ?1", [record], |x| Ok((x.get(0)?, x.get(1)?)))?;
                found_by_names.push(format!("{} {} - {} -> {artist} - {title}", r.id, r.artist, r.album));
            }
            if let Some(alike) = loosely.find_album_by(&w.title, &w.artist).and_then(|id| id.parse::<i64>().ok()).filter(|id| *id != record)
                && let Some((_, artist, title)) = owned.iter().find(|(id, ..)| *id == alike)
            {
                look_alikes.push(format!("{} {} - {} ~ {artist} - {title}", r.id, r.artist, r.album));
            }
            paged += u64::from(page == Some(true));
            if page == Some(false) {
                self.refused.push(format!("{} bandcamp:album:{}", ids::record(record), put.as_deref().unwrap_or_default()));
            }
            if new {
                made_for.push(format!("{} {} - {}", r.id, w.artist, w.title));
            }
            given.insert(r.id, record);
            self.moved += 1;
        }
        self.tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![REQUEST_RECORDS, serde_json::to_string(&given)?])?;
        self.counts = vec![("records_made_for_requests", made), ("pages_for_requests", paged)];
        // Each, whole, for a rehearsal to check (R14, F6): a request given a record not had where 0.16 judged it by names.
        self.lists = vec![
            ("records_made_for", made_for),
            ("requests_found_by_names", found_by_names),
            ("requests_beside_owned_look_alikes", look_alikes),
            ("names_shared_by_several_records", shared_names),
            ("want_covers_dropped", covers_dropped),
        ];
        Ok(())
    }

    /// 9. 0.14's choices: the source chosen, by hand; checked by hand; its guesses confirmed, each source of which the record
    ///    holds no reference listed.
    fn chosen(&mut self, c: &Carry) -> Result<()> {
        let (mut by_hand, mut checked, mut confirmed, mut asked, mut found) = (0, 0, 0, 0, 0);
        self.unmatched.entry(UNCONFIRMED).or_default();
        for choice in &c.chosen {
            let Some(record) = self.found(&choice.album)? else { continue };
            // A count chosen by hand since (the step run again) stands: 0.14's choice is the older (R11).
            if let Some(source) = &choice.counting {
                by_hand += self.tx.execute("UPDATE records SET counting = ?2, counted_by_hand = 1 WHERE id = ?1 AND counted_by_hand = 0", params![record, source])? as u64;
            }
            if choice.checked {
                checked += self.tx.execute("UPDATE records SET checked_by_hand = 1 WHERE id = ?1", [record])? as u64;
            }
            for source in &choice.confirmed {
                asked += 1;
                match self.tx.execute("UPDATE \"references\" SET confirmed = 1 WHERE subject = ?1 AND source = ?2", params![ids::record(record), source])? as u64 {
                    0 => self.unmatched.entry(UNCONFIRMED).or_default().push(format!("{} {source}", choice.album)),
                    n => {
                        found += 1;
                        confirmed += n;
                    }
                }
            }
        }
        self.tables.push(("confirmations", asked, found));
        self.counts = vec![("counted_by_hand", by_hand), ("checked_by_hand", checked), ("confirmed", confirmed)];
        Ok(())
    }
}

/// A reference as the report names it: "<subject> <source>:<kind>:<value>".
fn named(subject: &str, r: &Ref) -> String {
    format!("{subject} {}:{}:{}", r.source, r.kind, r.value)
}

fn named_ref(record: i64, r: &Ref) -> String {
    named(&ids::record(record), r)
}

/// One part of 0.17's step in the caller's transaction: its writes, and its report.
fn carry_part(tx: &Connection, c: &Carry, part: &str, now: i64) -> Result<serde_json::Value> {
    let count = || -> Result<u64> { Ok(tx.query_row("SELECT count(*) FROM \"references\"", [], |r| r.get::<_, i64>(0))? as u64) };
    let before = count()?;
    let mut w = Carrying { tx, refused: Vec::new(), retired: Vec::new(), taken: Vec::new(), unmatched: std::collections::BTreeMap::new(), moved: 0, tables: Vec::new(), counts: Vec::new(), lists: Vec::new() };
    match part {
        "hand" => w.hand(c, now)?,
        "certain" => w.matches(c, true)?,
        "files" => w.files(now)?,
        "likely" => w.matches(c, false)?,
        "links" => w.links(c)?,
        "discogs" => w.discogs(c)?,
        "discography" => w.discography(c)?,
        "pages" => w.pages(c)?,
        "chosen" => w.chosen(c)?,
        "requests" => w.requests(c)?,
        _ => anyhow::bail!("0.17's step has no part {part}"),
    }
    let mut report = crate::moved::Report::default();
    if let Some(table) = table_017(part) {
        let rows = c.read.iter().find(|(t, _)| *t == table).map_or(0, |(_, n)| *n);
        report.table(table, rows, rows, w.moved);
        report.unmatched(table, w.unmatched.remove(table).unwrap_or_default());
        for (what, keys) in c.noted.iter().filter(|(what, _)| what.strip_prefix(table).is_some_and(|why| why.starts_with(": "))) {
            report.unmatched(what, keys.clone());
        }
    }
    for (name, rows, moved) in std::mem::take(&mut w.tables) {
        report.table(name, rows, rows, moved);
    }
    for (what, keys) in std::mem::take(&mut w.unmatched) {
        report.unmatched(what, keys);
    }
    let after = count()?;
    report.table("references", before, after, after.saturating_sub(before));
    report.differed(HELD, std::mem::take(&mut w.refused));
    report.differed(RETIRED, std::mem::take(&mut w.retired));
    report.differed(TAKEN, std::mem::take(&mut w.taken));
    let mut report = serde_json::to_value(&report)?;
    if let Some(fields) = report.as_object_mut() {
        for (field, n) in w.counts {
            fields.insert(field.into(), n.into());
        }
        for (field, items) in w.lists {
            fields.insert(field.into(), items.into());
        }
    }
    Ok(report)
}

impl Catalogue {
    /// The album a fetcher placed in `dir` of catalogue folder `folder` is `record` (C50, `wanted_record`), said only once the
    /// fetcher's gate has accepted its files: when the record pass meets it as a new album, in `dir` or a folder under it (a
    /// download's discs), its tracks are given to that record, a record not had, and the expectation is used up. Forgotten a
    /// week after it was last said: a fetcher says it again once its last file has landed (F9). One record per folder: another
    /// record's expectation, still waiting, is never taken over (logged; false). Blocking.
    pub fn expect_record(&self, folder: i64, dir: &str, record: i64) -> Result<bool> {
        let (dir, at) = (dir.trim_matches('/'), now());
        let kept = self.db().execute(
            "INSERT INTO expected (folder, dir, record, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (folder, dir) DO UPDATE SET record = excluded.record, at = excluded.at WHERE expected.record = excluded.record OR expected.at < ?5",
            params![folder, dir, record, at, at - EXPECTED_SECS],
        )? > 0;
        if !kept {
            tracing::warn!(target: "library", folder, dir, record, "a folder another record is expected in: not expected for this one too");
        }
        Ok(kept)
    }

    /// The record the album a fetcher placed in `dir` (as Cratarr sees it) went to, once a record pass has read it (X4): the one
    /// most of its present tracks are on, followed through merges, of the files that came at `since` or after (Y10: an album that came
    /// to the folder before the placing is not its). None before then. Blocking.
    pub fn record_placed_in(&self, dir: &std::path::Path, since: i64) -> Result<Option<i64>> {
        let Some((folder, rel)) = self.place_of_path(dir) else { return Ok(None) };
        let db = self.db();
        let found: Option<i64> = db
            .prepare_cached(&format!(
                "SELECT t.record FROM files f JOIN tracks t ON t.id = f.track
                 WHERE f.folder = ?1 AND substr(f.rel, 1, length(?2) + 1) = ?2 || '/' AND COALESCE(f.came, 0) >= ?3 AND {} AND t.retired_at IS NULL
                   AND t.record IS NOT NULL
                 GROUP BY t.record ORDER BY count(*) DESC, t.record LIMIT 1",
                super::PRESENT_F
            ))?
            .query_row(params![folder, rel, since], |r| r.get(0))
            .optional()?;
        match found {
            Some(record) => merged_end(&db, record),
            None => Ok(None),
        }
    }

    /// The records 0.17's step gave the requests (C49a), by request id, for the user data's `request.record`. Blocking.
    pub fn request_records_017(&self) -> Result<Vec<(i64, i64)>> {
        let Some(text) = self.meta(REQUEST_RECORDS) else { return Ok(Vec::new()) };
        match serde_json::from_str::<std::collections::BTreeMap<i64, i64>>(&text) {
            Ok(given) => Ok(given.into_iter().collect()),
            // Unreadable (R19, F12, as the marks are read): left out, and the start goes on; a request it would have given one is
            // given its record again (`step_017`).
            Err(e) => {
                tracing::warn!(target: "library", reason = %e, "0.17's step: the records it gave the requests cannot be read; left out");
                Ok(Vec::new())
            }
        }
    }

    /// The record an album asked for is, found or made (`request_record_in`, C49a), opened now in the same transaction (Z5): the
    /// forgetting gathers what keeps a record, the requests among them, before its own transaction, so a record a Want names
    /// meanwhile is kept by its opening. Blocking.
    pub fn record_for_request(&self, w: &Wanted) -> Result<i64> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let at = now();
        let record = request_record_in(&tx, w, at)?.record;
        tx.execute(OPENED, params![record, at])?;
        tx.commit()?;
        Ok(record)
    }

    /// 0.17's step's parts not done (no mark), in order. Blocking.
    pub fn references_to_carry(&self) -> Result<Vec<&'static str>> {
        let db = self.db();
        let mut st = db.prepare("SELECT key FROM meta WHERE key GLOB '017:*'")?;
        let marked: std::collections::HashSet<String> = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(PARTS_017.into_iter().filter(|part| !marked.contains(&mark_017(part))).collect())
    }

    /// 0.17's step's report, of the parts done: their marks' reports as one, in the parts' order (`moved::Report::merge`), the
    /// counts summed. None: no part done. Blocking.
    pub fn references_carried(&self) -> Result<Option<serde_json::Value>> {
        let mut merged: Option<(crate::moved::Report, serde_json::Map<String, serde_json::Value>)> = None;
        for part in PARTS_017 {
            let Some(text) = self.meta(&mark_017(part)) else { continue };
            // A mark whose report cannot be read is a part done with its report missing (R11): the part stays done, and the
            // start goes on.
            let read = serde_json::from_str::<serde_json::Value>(&text).and_then(|v| Ok((serde_json::from_value::<crate::moved::Report>(v.clone())?, v)));
            let (report, v) = match read {
                Ok(read) => read,
                Err(e) => {
                    tracing::warn!(target: "library", part, reason = %e, "0.17's step: a part's report cannot be read; left out of the migration report");
                    continue;
                }
            };
            let (all, counts) = merged.get_or_insert_with(Default::default);
            all.merge(report);
            for (field, n) in v.as_object().into_iter().flatten().filter(|(k, _)| !matches!(k.as_str(), "tables" | "unmatched" | "differed")) {
                // A list (`Carrying::lists`) whole, its parts' one after another; a count summed.
                if let Some(items) = n.as_array() {
                    let all = counts.entry(field.clone()).or_insert_with(|| serde_json::Value::Array(Vec::new()));
                    if let Some(all) = all.as_array_mut() {
                        all.extend(items.iter().cloned());
                    }
                    continue;
                }
                let sum = counts.get(field).and_then(serde_json::Value::as_u64).unwrap_or(0) + n.as_u64().unwrap_or(0);
                counts.insert(field.clone(), sum.into());
            }
        }
        let Some((report, counts)) = merged else { return Ok(None) };
        let mut report = serde_json::to_value(&report)?;
        if let Some(fields) = report.as_object_mut() {
            fields.extend(counts);
        }
        Ok(Some(report))
    }

    /// 0.17's step on the catalogue's side: each part not done, in turn, in a transaction of its own whose last write is its
    /// mark, holding the part's report. Each begins IMMEDIATE, so no writer comes between what it reads and what it writes, and
    /// finds its mark inside it. Each says it begins as it begins (W1), a part done before saying nothing, so the log shows how
    /// far the step is. `after` hears each part done, inside its transaction: an error from it leaves that part unwritten, the
    /// parts before it done, and names that part as its failure would. The parts run now. Blocking.
    pub fn carry_references(&self, c: &Carry, after: &dyn Fn(&str) -> Result<()>) -> Result<Vec<&'static str>> {
        let mut db = self.db();
        let mut ran = Vec::new();
        for part in PARTS_017 {
            let mark = mark_017(part);
            // A part that fails names itself (X7), one that cannot begin too (W5): the catalogue held by another writer.
            let named = || format!("0.17's step, part {part}");
            if marked_017(&db, &mark).with_context(named)? {
                continue;
            }
            tracing::info!(target: "library", "0.17's step, part {part}");
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).with_context(named)?;
            if marked_017(&tx, &mark).with_context(named)? {
                continue;
            }
            let report = carry_part(&tx, c, part, now()).with_context(named)?;
            after(part).with_context(named)?;
            tx.execute("INSERT INTO meta (key, value) VALUES (?1, ?2)", params![mark, report.to_string()]).with_context(named)?;
            tx.commit().with_context(named)?;
            ran.push(part);
        }
        Ok(ran)
    }

    /// Whether 0.17's step's `part` is done (its mark), a part outside `PARTS_017` too (`CREDITS_017`). Blocking.
    pub fn part_done_017(&self, part: &str) -> Result<bool> {
        Ok(marked_017(&self.db(), &mark_017(part))?)
    }

    /// 0.17's step's part `credits` (P3, W3) on the catalogue's side: what `albums`' files name and credit kept for `records` (one
    /// for each album, in its order; none for an album whose tracks hold none), as a build's record pass keeps them where no album
    /// needs a record (`keep_files_names`: `files_references`, `record_credits`), in one IMMEDIATE transaction whose last write is
    /// the part's mark, holding how many records the files credit: a part cut off wrote nothing. No record is given, made, moved or
    /// merged. `after` hears the part done, inside it (what the tests cut it by). None: done before; else how many records the
    /// files credit. Blocking.
    pub fn credit_files_017(&self, albums: &[super::AlbumCopy], records: &[Option<i64>], after: &dyn Fn() -> Result<()>) -> Result<Option<u64>> {
        let mark = mark_017(CREDITS_017);
        let mut db = self.db();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if marked_017(&tx, &mark)? {
            return Ok(None);
        }
        let at = now();
        files_references(&tx, albums, records, at)?;
        record_credits(&tx, albums, records, at)?;
        let credited: i64 = tx.query_row("SELECT count(DISTINCT record) FROM record_artists WHERE how = 'files'", [], |r| r.get(0))?;
        after()?;
        tx.execute("INSERT INTO meta (key, value) VALUES (?1, ?2)", params![mark, serde_json::json!({ "records": credited }).to_string()])?;
        tx.commit()?;
        Ok(Some(credited.max(0) as u64))
    }
}

/// 0.17's step's part after `band_wave` (P3, W3, outside `PARTS_017` as `band_wave` is): the credits of the records you have,
/// kept as a build keeps them (`Catalogue::credit_files_017`), marked `017:credits`.
pub const CREDITS_017: &str = "credits";

// ---- RT01: a record not had that nobody sees, forgotten

/// How long a record not had is kept that no list showed and nobody opened (RT01, the owner's six months).
const KEPT_UNSEEN: i64 = 183 * 86_400;
/// When the forgetting first ran (`meta`): the clock of a record not seen since 0.17, so nothing from before goes until six
/// months after the first pass.
const RETENTION_FROM: &str = "retention_from";
/// A record seen at `?2`, never earlier than it was seen before (RT01).
const SEEN: &str = "UPDATE records SET seen_at = max(COALESCE(seen_at, 0), ?2) WHERE id = ?1";
/// A record opened at `?2`, never earlier than it was opened before (RT01): its page (C54), a Want (Z5).
const OPENED: &str = "UPDATE records SET opened_at = max(COALESCE(opened_at, 0), ?2) WHERE id = ?1";
/// A record not had that nothing keeps (RT01), as `n`: no track, ever, and nothing pointing at it: no merge either way, not
/// retired ("Was in Music until…"), no count or check chosen by hand, no reference pasted by hand nor confirmed, no folder
/// expected for it, no link from before 0.16, no request's shared name (X9). A request's and a Soulseek item's are left out by
/// the caller (`protected`).
const UNKEPT: &str = "n.merged_into IS NULL
       AND n.id NOT IN (SELECT merged_into FROM records WHERE merged_into IS NOT NULL)
       AND n.retired_at IS NULL
       AND n.counted_by_hand = 0 AND n.checked_by_hand = 0
       AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = n.id)
       AND NOT EXISTS (SELECT 1 FROM \"references\" f WHERE f.subject = 'al-' || n.id AND f.how = 'hand')
       AND NOT EXISTS (SELECT 1 FROM \"references\" c WHERE c.subject = 'al-' || n.id AND c.confirmed = 1)
       AND n.id NOT IN (SELECT record FROM expected)
       AND n.id NOT IN (SELECT record FROM album_moved)
       AND n.id NOT IN (SELECT record FROM shared_name_records)";
/// The records the forgetting takes of those nothing keeps (`UNKEPT`, RT01), `?1` the first pass (`RETENTION_FROM`), `?2` six
/// months ago: neither seen nor opened since `?2`. One never seen (from before 0.17, or kept by 0.17's step from its lists) counts
/// from when it was made or from the first pass, the later.
const FORGETTABLE: &str = "COALESCE(n.seen_at, max(n.born, ?1)) < ?2
       AND COALESCE(n.opened_at, 0) < ?2";
/// How long the kept lists of a record not had that nothing keeps stay unopened (RT01b): read again when they are asked for.
const LISTS_KEPT: i64 = 30 * 86_400;
/// The kept lists the forgetting lets go of (RT01b), `?1` thirty days ago: each read before then, of a record nothing keeps
/// (`UNKEPT`) not opened since.
const LISTS_FORGETTABLE: &str = "k.at < ?1 AND COALESCE(n.opened_at, 0) < ?1";
/// How long an artist is left with nothing before they are forgotten (RT01b), from when they were given their id or, later, when
/// their last credit went (Z10, `artists_bare`): a build or a heart may be about to credit one just named, and an Undo of a retag
/// to credit one again.
const ARTIST_NEW: i64 = 86_400;
/// The artists the forgetting takes (RT01b), `?1` a day ago: credited on no record, named by no reference by hand nor
/// confirmed, no id from before 0.16 went to them, given theirs, and their last credit gone (Z10), at `?1` or before. One a track
/// credits, or the user data or a store names, is the caller's (`forget_quiet_artists`'s `protected`).
const ARTISTS_FORGETTABLE: &str = "SELECT a.id FROM artists a
     WHERE max(a.born, COALESCE((SELECT b.since FROM artists_bare b WHERE b.artist = a.id), 0)) <= ?1
       AND NOT EXISTS (SELECT 1 FROM record_artists ra WHERE ra.artist = a.id)
       AND NOT EXISTS (SELECT 1 FROM \"references\" f WHERE f.subject = 'ar-' || a.id AND f.how = 'hand')
       AND NOT EXISTS (SELECT 1 FROM \"references\" c WHERE c.subject = 'ar-' || a.id AND c.confirmed = 1)
       AND a.id NOT IN (SELECT id FROM artist_moved)
     ORDER BY a.id";
/// An artist's last credit gone at `?2` (Z10), where no record credits them now: their day's grace counts from then.
const LAST_CREDIT_WENT: &str =
    "INSERT OR REPLACE INTO artists_bare (artist, since) SELECT ?1, ?2 WHERE NOT EXISTS (SELECT 1 FROM record_artists WHERE artist = ?1)";
/// A subject forgotten at `?2`, its tombstone (RT01).
const FORGOTTEN: &str = "INSERT OR REPLACE INTO forgotten (subject, at) VALUES (?1, ?2)";
/// The artists with albums here: each credited first on a live record that holds live tracks. Discover's missing list is theirs
/// (C53b), and the paced re-read comes to them (RT01c), but for those with no list of their own (`library_artists_in`, Z1).
const LIBRARY_ARTISTS: &str = "SELECT b.artist FROM record_artists b JOIN records h ON h.id = b.record
    WHERE b.position = 0 AND h.merged_into IS NULL AND h.retired_at IS NULL AND EXISTS (SELECT 1 FROM tracks u WHERE u.record = h.id AND u.retired_at IS NULL)";
/// The lists the paced re-read reads, in its order (RT01c): Bandcamp's is read only when a page is opened.
const REREAD_SOURCES: [&str; 2] = ["musicbrainz", "discogs"];
/// How long a library artist's list stays read before the paced re-read comes to it again (RT01c): a month, as a page's.
const REREAD_EVERY: i64 = 30 * 86_400;
/// A day's re-read comes to one in this many of the library's artists, rounded up (RT01c): each once a month, spread over it.
const REREAD_DAYS: usize = 30;
/// MusicBrainz's "Various Artists" and "[unknown]": no list of their own (Z1).
const LISTLESS_MBIDS: [&str; 2] = ["89ad4ac3-39f7-470e-963a-56509c546377", "125ec42a-7229-4250-afc5-e057484327fe"];

/// Whether an artist has no list of their own (Z1, 0.16's rule carried: Discover never read one): a compilation's album artist,
/// "Various Artists", by their name (`crates::various`) or by the id MusicBrainz knows them by (`refs`), and MusicBrainz's
/// "[unknown]". Their list is never read, and their records not had are listed nowhere.
pub fn listless<'r>(name: &str, refs: impl IntoIterator<Item = &'r Ref>) -> bool {
    crate::crates::various(name) || refs.into_iter().any(|r| r.source == "musicbrainz" && r.kind == "artist" && LISTLESS_MBIDS.contains(&r.value.as_str()))
}

/// The artists with albums here (`LIBRARY_ARTISTS`) but those with no list of their own (`listless`, Z1), in id order: whom the
/// paced re-read comes to (RT01c) and whose records not had Discover's missing list lists (C53b).
fn library_artists_in(db: &Connection) -> Result<Vec<i64>> {
    let named: Vec<(i64, String)> = db
        .prepare_cached(&format!("SELECT DISTINCT l.artist, COALESCE(a.name, '') FROM ({LIBRARY_ARTISTS}) l LEFT JOIN artists a ON a.id = l.artist ORDER BY l.artist"))?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    // Who MusicBrainz knows by an id of no list: a few at most.
    let subjects: Vec<String> = db
        .prepare_cached("SELECT subject FROM \"references\" WHERE source = 'musicbrainz' AND kind = 'artist' AND value IN (?1, ?2)")?
        .query_map(LISTLESS_MBIDS, |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let held: std::collections::HashSet<i64> = subjects.iter().filter_map(|s| ids::artist_number(s)).collect();
    Ok(named.into_iter().filter(|(id, name)| !crate::crates::various(name) && !held.contains(id)).map(|(id, _)| id).collect())
}

impl Catalogue {
    /// An artist's records only Bandcamp knows, or no source, seen at `at` (RT01), never earlier than they were: where a list of
    /// theirs other than Bandcamp's is read (`discover::read_one`), as Bandcamp's own lists only its search box's first answers
    /// (O4); and where the paced re-read finds that neither MusicBrainz nor Discogs knows a library artist (Z4), as it never reads
    /// Bandcamp's. Blocking.
    pub fn bandcamp_only_seen(&self, artist: i64, at: i64) -> Result<()> {
        self.db().execute(
            "UPDATE records SET seen_at = max(COALESCE(seen_at, 0), ?2)
             WHERE id IN (SELECT record FROM record_artists WHERE artist = ?1)
               AND NOT EXISTS (SELECT 1 FROM \"references\" f WHERE f.subject = 'al-' || records.id AND f.source <> 'bandcamp')",
            params![artist, at],
        )?;
        Ok(())
    }

    /// A record's page opened at `at` (RT01, C54): kept six months from then, and its kept lists thirty days. Never earlier than it
    /// was opened before. Never waits on the writer (Z11): while it is busy (a build's record pass holds it for minutes) the mark is
    /// skipped, which only shortens the keep by this one visit. Whether it was marked. Blocking.
    pub fn opened(&self, record: i64, at: i64) -> Result<bool> {
        let db = match self.db.try_lock() {
            Ok(db) => db,
            Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return Ok(false),
        };
        db.execute(OPENED, params![record, at])?;
        Ok(true)
    }

    /// When a subject was forgotten (RT01): its tombstone, by which a link to it answers "no longer known". None: never
    /// forgotten. On a reader. Blocking.
    pub fn forgotten_at(&self, subject: &str) -> Result<Option<i64>> {
        let read = |db: &Connection| -> Result<Option<i64>> {
            Ok(db.prepare_cached("SELECT at FROM forgotten WHERE subject = ?1")?.query_row([subject], |r| r.get(0)).optional()?)
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// The records not had that nothing keeps, forgotten at `now` (RT01), in one transaction: those `FORGETTABLE` names, but
    /// for `protected` (every request's record, whatever its status, and Soulseek's items', gathered outside the catalogue).
    /// Each goes with its references, credits and lists, and leaves a tombstone (`forgotten`); its id is never given again. A
    /// shared name (X9) whose record is gone, however it went, goes too. The first pass starts the clock of what was not seen
    /// since 0.17 (`RETENTION_FROM`). The ids forgotten. Blocking.
    pub fn forget_quiet(&self, now: i64, protected: &std::collections::BTreeSet<i64>) -> Result<Vec<i64>> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute("INSERT OR IGNORE INTO meta (key, value) VALUES (?1, ?2)", params![RETENTION_FROM, now.to_string()])?;
        // One that cannot be read counts as now: nothing goes by it.
        let from: String = tx.query_row("SELECT value FROM meta WHERE key = ?1", [RETENTION_FROM], |r| r.get(0))?;
        let from: i64 = from.parse().unwrap_or(now);
        let gone: Vec<i64> = {
            let mut st = tx.prepare_cached(&format!("SELECT n.id FROM records n WHERE {UNKEPT} AND {FORGETTABLE} ORDER BY n.id"))?;
            let all: Vec<i64> = st.query_map(params![from, now - KEPT_UNSEEN], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            all.into_iter().filter(|record| !protected.contains(record)).collect()
        };
        // The artists a credit went from here (Z10), noted where it was their last.
        let mut uncredited: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
        for &record in &gone {
            let subject = ids::record(record);
            tx.prepare_cached("DELETE FROM \"references\" WHERE subject = ?1")?.execute([&subject])?;
            let mut credits = tx.prepare_cached("DELETE FROM record_artists WHERE record = ?1 RETURNING artist")?;
            for artist in credits.query_map([record], |r| r.get::<_, i64>(0))? {
                uncredited.insert(artist?);
            }
            for table in ["record_labels", "reference_tracks", "lists_to_read"] {
                tx.prepare_cached(&format!("DELETE FROM {table} WHERE record = ?1"))?.execute([record])?;
            }
            tx.prepare_cached("DELETE FROM records WHERE id = ?1")?.execute([record])?;
            tx.prepare_cached(FORGOTTEN)?.execute(params![subject, now])?;
        }
        for artist in uncredited {
            tx.prepare_cached(LAST_CREDIT_WENT)?.execute(params![artist, now])?;
        }
        tx.execute("DELETE FROM shared_name_records WHERE record NOT IN (SELECT id FROM records)", [])?;
        tx.commit()?;
        Ok(gone)
    }

    /// The kept lists of records not had that nothing keeps (RT01b, `UNKEPT`), let go of at `now` where read thirty days before
    /// and the record not opened since, but for `protected` (a request's record and a Soulseek item's: they are judged by
    /// them). A list let go of is read again when it is asked for (`references::tracklist`); the record's count stays, so its
    /// source is read. How many lists. Blocking.
    pub fn forget_quiet_lists(&self, now: i64, protected: &std::collections::BTreeSet<i64>) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let gone: Vec<(i64, String)> = {
            let sql = format!("SELECT k.record, k.source FROM reference_tracks k JOIN records n ON n.id = k.record WHERE {LISTS_FORGETTABLE} AND {UNKEPT}");
            let mut st = tx.prepare_cached(&sql)?;
            let all: Vec<(i64, String)> = st.query_map([now - LISTS_KEPT], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            all.into_iter().filter(|(record, _)| !protected.contains(record)).collect()
        };
        for (record, source) in &gone {
            tx.prepare_cached("DELETE FROM reference_tracks WHERE record = ?1 AND source = ?2")?.execute(params![record, source])?;
        }
        tx.commit()?;
        Ok(gone.len())
    }

    /// The artists left with nothing, forgotten at `now` (RT01b), in one transaction: those `ARTISTS_FORGETTABLE` names (a day
    /// after their last credit went, Z10), but for `protected` (a track's credit, what the user data or a store names, gathered
    /// outside the catalogue). Each goes with their references and leaves a tombstone (`forgotten`, `ar-<n>`); their id is never
    /// given again, and a name credited again is given a new one. Their number among the artists of their name stays theirs (C55,
    /// `artist_numbers`), so it is never given again either. The ids forgotten. Blocking.
    pub fn forget_quiet_artists(&self, now: i64, protected: &std::collections::BTreeSet<i64>) -> Result<Vec<i64>> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        // When the last credit went of one credited again means nothing: let go of (Z10).
        tx.execute("DELETE FROM artists_bare WHERE artist IN (SELECT artist FROM record_artists)", [])?;
        let gone: Vec<i64> = {
            let mut st = tx.prepare_cached(ARTISTS_FORGETTABLE)?;
            let all: Vec<i64> = st.query_map([now - ARTIST_NEW], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            all.into_iter().filter(|artist| !protected.contains(artist)).collect()
        };
        for &artist in &gone {
            let subject = ids::artist(artist);
            tx.prepare_cached("DELETE FROM \"references\" WHERE subject = ?1")?.execute([&subject])?;
            tx.prepare_cached("DELETE FROM list_reread WHERE artist = ?1")?.execute([artist])?;
            tx.prepare_cached("DELETE FROM artists_bare WHERE artist = ?1")?.execute([artist])?;
            tx.prepare_cached("DELETE FROM artists WHERE id = ?1")?.execute([artist])?;
            tx.prepare_cached(FORGOTTEN)?.execute(params![subject, now])?;
        }
        tx.commit()?;
        Ok(gone)
    }

    /// The library artists the paced re-read comes to at `now` (RT01c): of the artists with albums here but those with no list of
    /// their own (`library_artists_in`, Z1), a thirtieth (rounded up), those whose MusicBrainz or Discogs list was read, or come to,
    /// longest ago first, never first of all; each with its sources (`REREAD_SOURCES`) a month old or never read. On a reader.
    /// Blocking.
    pub fn rereads_due(&self, now: i64) -> Result<Vec<(i64, Vec<&'static str>)>> {
        let read = |db: &Connection| -> Result<Vec<(i64, Vec<&'static str>)>> {
            let artists = library_artists_in(db)?;
            // Its list there read last (`artist_list_read`), else the re-read's last coming to it, 0 for neither.
            let mut last = db.prepare_cached(
                "SELECT max(COALESCE((SELECT max(read_at) FROM \"references\" WHERE subject = ?1 AND source = ?2 AND kind = 'artist'), 0),
                            COALESCE((SELECT at FROM list_reread WHERE artist = ?3 AND source = ?2), 0))",
            )?;
            let mut due: Vec<(i64, i64, Vec<&'static str>)> = Vec::new();
            for &artist in &artists {
                let (mut oldest, mut sources) = (i64::MAX, Vec::new());
                for source in REREAD_SOURCES {
                    let at: i64 = last.query_row(params![ids::artist(artist), source, artist], |r| r.get(0))?;
                    if at < now - REREAD_EVERY {
                        oldest = oldest.min(at);
                        sources.push(source);
                    }
                }
                if !sources.is_empty() {
                    due.push((oldest, artist, sources));
                }
            }
            due.sort_by_key(|(oldest, artist, _)| (*oldest, *artist));
            due.truncate(artists.len().div_ceil(REREAD_DAYS));
            Ok(due.into_iter().map(|(_, artist, sources)| (artist, sources)).collect())
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// The paced re-read came to an artist's list at `source` at `at` (RT01c), read or not: it comes again a month later. Never
    /// earlier than it came before. Blocking.
    pub fn reread_tried(&self, artist: i64, source: &str, at: i64) -> Result<()> {
        self.db().execute(
            "INSERT INTO list_reread (artist, source, at) VALUES (?1, ?2, ?3) ON CONFLICT (artist, source) DO UPDATE SET at = max(at, excluded.at)",
            params![artist, source, at],
        )?;
        Ok(())
    }

    /// An artist's records not had seen at `at` (RT01c), never earlier than they were: where the paced re-read could not read
    /// their list (it failed, or its source is switched off, or the background paused), so an outage, a switch or a pause never
    /// makes an album forgotten. Blocking.
    pub fn artist_records_seen(&self, artist: i64, at: i64) -> Result<()> {
        self.db().execute(
            "UPDATE records SET seen_at = max(COALESCE(seen_at, 0), ?2)
             WHERE id IN (SELECT record FROM record_artists WHERE artist = ?1) AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = records.id)",
            params![artist, at],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R4: what an album's files name is what most of them name, folded to lower case, the lower of two named as often; a
    /// blank names nothing. Not the first file's: its tag may be the odd one out.
    #[test]
    fn what_the_files_name_is_what_most_of_them_name() {
        let (a, b) = ("00000000-0000-4000-8000-000000000001", "ABCDEF00-0000-4000-8000-000000000002");
        assert_eq!(most_named([a, b, b]).as_deref(), Some("abcdef00-0000-4000-8000-000000000002"));
        assert_eq!(most_named([b, a]).as_deref(), Some(a), "as often: the lower");
        assert_eq!(most_named(["", " "]), None);
    }

    /// R17 (R9's rest): two records not had merge into the live one: a retired one, older or not, is never the end.
    #[test]
    fn a_merge_goes_into_the_live_record() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (gone, live) = (cat.record_for_tests("Undertow", 0), cat.record_for_tests("Undertow (asked for)", 0));
        cat.db().execute("UPDATE records SET retired_at = 5 WHERE id = ?1", [gone]).unwrap();
        assert_eq!(cat.merge(gone, live).unwrap(), live);
    }

    /// R14 (F6), W2 (B1): a Want for an album the library has finds it by its own names, folded as the had rule folds them, never by
    /// looser ones: "Tide Tables (Deluxe Edition)" is another album than the owned "Tide Tables".
    #[test]
    fn a_request_for_an_album_had_by_a_loose_name_is_another() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let had = cat.record_for_tests("Tide Tables", 2);
        assert_ne!(crate::app::requests::record_of_album(&cat, None, "The Glass Harbour", "Tide Tables (Deluxe Edition)", None, None).unwrap(), had);
        assert_eq!(crate::app::requests::record_of_album(&cat, None, "GLASS HARBOUR", "Tide Tables", None, None).unwrap(), had, "its own names, folded");
    }

    /// A record not had of `artist`'s, as their list keeps one: an album, titled `title`, holding `refs`.
    fn credited(cat: &Catalogue, artist: i64, title: &str, refs: &[(&str, &str, &str)]) -> i64 {
        let l = Listing { title: title.into(), artist: "Glass Harbour".into(), kind: Some("album".into()), refs: refs.iter().map(|(s, k, v)| Ref::new(s, k, v)).collect(), ..Listing::default() };
        cat.keep_listing(Some(artist), &[l], "link").unwrap()[0]
    }

    fn missing_titles(cat: &Catalogue, artist: i64) -> Vec<String> {
        cat.missing_of(artist).unwrap().into_iter().map(|q| q.row.title).collect()
    }

    /// C53a: a record not had whose release group a live, had record holds is had by it, so it is not on its artist's list.
    #[test]
    fn have_by_references() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let artist = cat.artist_for("Glass Harbour").unwrap();
        let (held, other) = ("00000000-0000-4000-8000-000000000160", "00000000-0000-4000-8000-000000000161");
        credited(&cat, artist, "Tide Tables (Deluxe)", &[("musicbrainz", "release-group", held)]);
        credited(&cat, artist, "Undertow", &[("musicbrainz", "release-group", other)]);
        assert_eq!(missing_titles(&cat, artist), ["Tide Tables (Deluxe)", "Undertow"]);
        let had = cat.record_for_tests("Tide Tables", 2);
        assert!(cat.set_reference(&ids::record(had), &Ref::new("musicbrainz", "release-group", held), "files").unwrap());
        assert_eq!(missing_titles(&cat, artist), ["Undertow"]);
    }

    /// Z3: a record made at open and credited to nobody (a label's page, Beyond: `keep_listing(None, …)`) joins the artist whose
    /// list later names it by its reference: credited to them, and on their list. One credited already takes no other list's.
    #[test]
    fn a_record_opened_from_a_label_joins_its_artist_when_their_list_names_it() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let [theirs, other] = ["Kite Theory", "Noor Field"].map(|n| cat.artist_for(n).unwrap());
        let group = [("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000165")];
        let label_row = Listing { title: "Night Walks".into(), kind: Some("album".into()), refs: vec![Ref::new(group[0].0, group[0].1, group[0].2)], ..Listing::default() };
        let opened = cat.keep_listing(None, &[label_row], "link").unwrap()[0];
        assert!(cat.record_artists(opened).unwrap().is_empty(), "opened from a label's page: credited to nobody");
        assert_eq!(credited(&cat, theirs, "Night Walks", &group), opened, "their list finds it by its reference");
        assert_eq!(cat.record_artists(opened).unwrap(), [theirs], "and credits them");
        assert_eq!(missing_titles(&cat, theirs), ["Night Walks"], "on their list");
        assert_eq!(credited(&cat, other, "Night Walks", &group), opened);
        assert_eq!(cat.record_artists(opened).unwrap(), [theirs], "credited already: no other list's");
    }

    /// C53a: a record not had of its artist's title, folded, that a live, had record has too is had by it where no reference
    /// contradicts; where two Bandcamp pages differ they are two records, and it stays on the list.
    #[test]
    fn have_by_names_where_nothing_contradicts() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let artist = cat.artist_for("Glass Harbour").unwrap();
        credited(&cat, artist, "Tide Tables", &[("discogs", "master", "3010")]);
        credited(&cat, artist, "Two Rooms", &[("bandcamp", "album", "https://glassharbour.bandcamp.com/album/two-rooms")]);
        cat.record_for_tests("TIDE TABLES", 2);
        let live = cat.record_for_tests("Two Rooms", 2);
        assert!(cat.set_reference(&ids::record(live), &Ref::new("bandcamp", "album", "https://glassharbour.bandcamp.com/album/two-rooms-live"), "link").unwrap());
        assert_eq!(missing_titles(&cat, artist), ["Two Rooms"]);
    }

    /// C53a, OQ16: a retired record is on its artist's list as not had, with when it went, and once: a list naming its release group
    /// again keeps to it; a live record of its group takes its place.
    #[test]
    fn a_retired_record_is_listed_once() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let artist = cat.artist_for("Glass Harbour").unwrap();
        let group = "00000000-0000-4000-8000-000000000162";
        let gone = credited(&cat, artist, "Partial Shore", &[("musicbrainz", "release-group", group)]);
        cat.db().execute("UPDATE records SET retired_at = 1790000000 WHERE id = ?1", [gone]).unwrap();
        assert_eq!(credited(&cat, artist, "Partial Shore", &[("musicbrainz", "release-group", group)]), gone, "listed again: the same record");
        let listed: Vec<(i64, Option<i64>)> = cat.missing_of(artist).unwrap().iter().map(|q| (q.row.id, q.row.retired_at)).collect();
        assert_eq!(listed, [(gone, Some(1790000000))]);
        let live = cat.record_for_tests("Partial Shore (Remaster)", 0);
        assert!(cat.set_reference(&ids::record(live), &Ref::new("musicbrainz", "release-group", group), "link").unwrap());
        cat.db().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'listing')", params![live, artist]).unwrap();
        assert_eq!(cat.missing_of(artist).unwrap().iter().map(|q| q.row.id).collect::<Vec<_>>(), [live], "the live record of its group in its place");
    }

    /// C53b: Discover's missing list is one query on one reader, never the writer's lock: while a build holds it, the records not
    /// had of every artist with albums here are read at once; an artist with none here has no row.
    #[test]
    fn discover_missing_is_one_query() {
        let dir = std::env::temp_dir().join(format!("cratarr-missing-everywhere-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let cat = std::sync::Arc::new(Catalogue::open(Some(&dir.join("catalogue.sqlite")), Vec::new()).unwrap());
        let (here, elsewhere) = (cat.artist_for("Glass Harbour").unwrap(), cat.artist_for("Orbit Lane").unwrap());
        let had = cat.record_for_tests("Tide Tables", 2);
        cat.db().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'files')", params![had, here]).unwrap();
        credited(&cat, here, "Undertow", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000163")]);
        credited(&cat, elsewhere, "Quiet Lanes", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000164")]);
        let opened = cat.readers_for_tests();
        let build = cat.writer_for_tests();
        let (tx, rx) = std::sync::mpsc::channel();
        let c = cat.clone();
        std::thread::spawn(move || tx.send(c.missing_everywhere().unwrap()).ok());
        let (rows, _) = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("answered while a build holds the writer");
        drop(build);
        assert_eq!(rows.iter().map(|(artist, q)| (*artist, q.row.title.as_str())).collect::<Vec<_>>(), [(here, "Undertow")]);
        assert_eq!(cat.readers_for_tests() - opened, 1, "one reader for every artist");
        drop(cat);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// R21 (N1), W2 (B1): a Want for "Tide Tables (Remixes)" by its own release group is not the owned "Tide Tables", whose files
    /// name another group, though the loose names agree: it is a record of its own, holding the group asked for. Nor is a Want by
    /// its names only: the owned album is found by its own names, folded, never loosely.
    #[test]
    fn a_want_for_a_remix_album_is_not_the_owned_album_of_its_name() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (owned_group, asked) = ("00000000-0000-4000-8000-000000000200", "00000000-0000-4000-8000-000000000201");
        let owned = cat.record_for_tests("Tide Tables", 2);
        assert!(cat.set_reference(&ids::record(owned), &Ref::new("musicbrainz", "release-group", owned_group), "files").unwrap());
        let wanted = crate::app::requests::record_of_album(&cat, Some(asked), "Glass Harbour", "Tide Tables (Remixes)", None, None).unwrap();
        assert_ne!(wanted, owned);
        assert!(cat.references_of(&ids::record(wanted)).unwrap().iter().any(|(r, _)| r.value == asked));
        let untagged = crate::app::requests::record_of_album(&cat, None, "Glass Harbour", "Tide Tables (Deluxe)", None, None).unwrap();
        assert_ne!(untagged, owned, "by its names only, not the owned album its loose names match (W2)");
        assert_eq!(crate::app::requests::record_of_album(&cat, None, "Glass Harbour", "TIDE TABLES", None, None).unwrap(), owned, "its own names, folded");
    }

    /// W6 (H): a Want credited to two artists credits each, as the record pass credits a file's, the first its artist; not one
    /// artist named "A / B" for life.
    #[test]
    fn a_want_credited_to_two_artists_credits_each() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let wanted = Wanted { artist: "Noor Field / Kite Theory".into(), artists: vec!["Noor Field".into(), "Kite Theory".into()], title: "Together".into(), ..Wanted::default() };
        let record = cat.record_for_request(&wanted).unwrap();
        let names: Vec<String> = cat
            .db()
            .prepare("SELECT a.name FROM record_artists ra JOIN artists a ON a.id = ra.artist WHERE ra.record = ?1 ORDER BY ra.position")
            .unwrap()
            .query_map([record], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(names, ["Noor Field", "Kite Theory"]);
    }

    /// W4 (D1): a copy whose files are all in the trash makes no request had: the album deleted (rejected) is not "In the library"
    /// for the trash's days.
    #[test]
    fn a_copy_in_the_trash_does_not_make_a_request_had() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let owned = cat.record_for_tests("Tide Tables", 2);
        cat.copy_for_tests(owned, 1);
        let holder = || cat.requested(&[owned]).unwrap()[&owned].holder;
        assert_eq!(holder(), Some(owned));
        cat.mark_trashed(1, &format!("copy-{owned}"), "t1").unwrap();
        assert_eq!(holder(), None, "its only copy in the trash");
    }

    /// W2: a Want whose Bandcamp page is another than the one an owned album of its names holds is not that album: its page says
    /// they are two.
    #[test]
    fn a_want_whose_page_is_another_is_not_the_owned_album_of_its_name() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let owned = cat.record_for_tests("Two Rooms", 2);
        assert!(cat.set_reference(&ids::record(owned), &Ref::new("bandcamp", "album", "https://glassharbour.bandcamp.com/album/two-rooms"), "link").unwrap());
        let wanted = Wanted { artist: "Glass Harbour".into(), title: "Two Rooms".into(), page: Some("https://glassharbour.bandcamp.com/album/two-rooms-live".into()), ..Wanted::default() };
        assert_ne!(cat.record_for_request(&wanted).unwrap(), owned);
    }

    /// X8 (the rehearsal's request 89, "Orbital"): a name two live records of the artist share, had or not had, finds neither by
    /// its names: the request takes its own record, its release group's where it has one, and says its names are shared.
    #[test]
    fn a_request_for_a_self_titled_album_with_two_of_that_name_is_not_found_by_names() {
        let asked = |cat: &Catalogue, w: &Wanted| {
            let mut db = cat.db();
            let tx = db.transaction().unwrap();
            let r = request_record_in(&tx, w, 1).unwrap();
            tx.commit().unwrap();
            (r.record, r.made, r.by_names, r.shared)
        };
        let named = Wanted { artist: "Glass Harbour".into(), title: "Glass Harbour".into(), ..Wanted::default() };
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let had = [cat.record_for_tests("Glass Harbour", 2), cat.record_for_tests("Glass Harbour", 3)];
        let (record, made, by_names, shared) = asked(&cat, &named);
        assert!(!had.contains(&record) && made && !by_names && shared, "its own record, its names said to be shared");
        let group = "00000000-0000-4000-8000-000000000204";
        let (record, ..) = asked(&cat, &Wanted { group: Some(group.into()), ..named.clone() });
        assert!(!had.contains(&record));
        assert!(refs_in(&cat.db(), &ids::record(record)).unwrap().iter().any(|(s, k, v)| (s.as_str(), k.as_str(), v.as_str()) == ("musicbrainz", "release-group", group)), "by its group");
        // One had, one not had of another release group (a discography's): two still. The one not had holding no MusicBrainz
        // reference (a list's, a request's) is no album of its own (V3): the had one is found by its names.
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let both = [cat.record_for_tests("Glass Harbour", 2), cat.record_for_tests("Glass Harbour", 0)];
        let (record, _, by_names, shared) = asked(&cat, &named);
        assert!(record == both[0] && by_names && !shared, "V3: the had one, by its names");
        // The one not had of a release group (a discography's) beside an untagged had one: the had one holds it by its names, one
        // album (U2). A second of another group (the discography's other album of the name): two, nothing says which.
        assert!(cat.set_reference(&ids::record(both[1]), &Ref::new("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000205"), "link").unwrap());
        let (record, _, by_names, shared) = asked(&cat, &named);
        assert!(record == both[0] && by_names && !shared, "U2: one album");
        let second = cat.record_for_tests("Glass Harbour", 0);
        assert!(cat.set_reference(&ids::record(second), &Ref::new("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000206"), "link").unwrap());
        let (record, _, by_names, shared) = asked(&cat, &named);
        assert!(![both[0], both[1], second].contains(&record) && !by_names && shared);
    }

    /// X8: a name one live record of the artist alone has is still found by it, had or not had.
    #[test]
    fn a_request_whose_name_is_unique_is_still_found_by_names() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (had, not_had) = (cat.record_for_tests("Tide Tables", 2), cat.record_for_tests("Open Water", 0));
        let w = |title: &str| Wanted { artist: "Glass Harbour".into(), title: title.into(), ..Wanted::default() };
        assert_eq!(cat.record_for_request(&w("Tide Tables")).unwrap(), had);
        assert_eq!(cat.record_for_request(&w("Open Water")).unwrap(), not_had);
    }

    /// X9: records holding one MusicBrainz release group are one album's copies or editions, no clash of names (X8): a request by its
    /// names takes the oldest of them; a record of the name with another group makes it a clash again.
    #[test]
    fn records_sharing_a_group_are_one_album_for_names() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let grouped = |record: i64, group: &str| {
            cat.db().execute("INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES (?1, 'musicbrainz', 'release-group', ?2, 1, 'files')", params![ids::record(record), group]).unwrap();
        };
        let copies = [cat.record_for_tests("Glass Harbour", 2), cat.record_for_tests("Glass Harbour", 2)];
        for c in copies {
            grouped(c, "00000000-0000-4000-8000-000000000301");
        }
        let named = Wanted { artist: "Glass Harbour".into(), title: "Glass Harbour".into(), ..Wanted::default() };
        assert_eq!(cat.record_for_request(&named).unwrap(), copies[0], "two copies of one album: the oldest, by its names");
        let other = cat.record_for_tests("Glass Harbour", 3);
        grouped(other, "00000000-0000-4000-8000-000000000302");
        let record = cat.record_for_request(&named).unwrap();
        assert!(!copies.contains(&record) && record != other, "another album of the name: a clash again, its own record");
    }

    /// X9: requests by their names alone that meet one clash of names share the record made for the first: two accounts asking for
    /// "Orbital" are one record's, not one each. One asked by its own release group takes that group's.
    #[test]
    fn two_requests_for_a_shared_name_share_one_record() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let had = [cat.record_for_tests("Glass Harbour", 2), cat.record_for_tests("Glass Harbour", 3)];
        let named = Wanted { artist: "Glass Harbour".into(), title: "Glass Harbour".into(), ..Wanted::default() };
        let first = cat.record_for_request(&named).unwrap();
        assert!(!had.contains(&first));
        assert_eq!(cat.record_for_request(&Wanted { artist: "GLASS HARBOUR".into(), ..named.clone() }).unwrap(), first, "the second asker, its names folded alike");
        assert_eq!(cat.record_for_request(&named).unwrap(), first, "and a third");
        let by_group = cat.record_for_request(&Wanted { group: Some("00000000-0000-4000-8000-000000000303".into()), ..named.clone() }).unwrap();
        assert!(![had[0], had[1], first].contains(&by_group), "one asked by its group: that group's own record");
    }

    /// X9, Y1: an album held as two copies (a release group in common) reads had through the had rule (`holders_in`), so no
    /// fetcher takes it: its request takes the older copy by its names. Two had records nothing tells apart are two albums of one
    /// name (X8's clash), no longer one owned twice: the request takes a record of its own, which neither holds by its names, and
    /// stays Wanted (Y1, the triage's rule).
    #[test]
    fn a_request_for_an_album_owned_twice_reads_had() {
        for grouped in [true, false] {
            let cat = Catalogue::open(None, Vec::new()).unwrap();
            let twice = [cat.record_for_tests("Two Rooms", 2), cat.record_for_tests("Two Rooms", 2)];
            if grouped {
                for r in twice {
                    cat.db().execute("INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES (?1, 'musicbrainz', 'release-group', '00000000-0000-4000-8000-000000000304', 1, 'files')", [ids::record(r)]).unwrap();
                }
            }
            let record = cat.record_for_request(&Wanted { artist: "Glass Harbour".into(), title: "Two Rooms".into(), ..Wanted::default() }).unwrap();
            assert_eq!(cat.requested(&[record]).unwrap()[&record].holder, grouped.then_some(twice[0]), "grouped {grouped}: held by the older copy; else Wanted");
        }
    }

    /// Y1 (the review's finding 1, the rehearsal's request 89): of two albums of one name, one had and one not (Orbital's 1991
    /// album owned, its 1993 one a discography's), a request for the name is not had by the owned one: by its names alone it takes
    /// X9's shared record, by a group no record holds a record of its own (X8), and neither is held by names. Both stay Wanted.
    /// The owned album is tagged with its group, as Lidarr tags it: an untagged one beside a single record of the name not had is
    /// that record's album (U2), and beside both of the discography's (`an_untagged_owned_album_holds_…`) still a clash.
    #[test]
    fn a_request_for_one_of_two_same_named_albums_is_not_had_by_the_other() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let owned = cat.record_for_tests("Glass Harbour", 2);
        assert!(cat.set_reference(&ids::record(owned), &Ref::new("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000319"), "files").unwrap());
        let listed = cat.record_for_tests("Glass Harbour", 0);
        assert!(cat.set_reference(&ids::record(listed), &Ref::new("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000320"), "link").unwrap());
        let named = Wanted { artist: "Glass Harbour".into(), title: "Glass Harbour".into(), ..Wanted::default() };
        let alone = cat.record_for_request(&named).unwrap();
        let by_group = cat.record_for_request(&Wanted { group: Some("00000000-0000-4000-8000-000000000321".into()), ..named }).unwrap();
        assert!(![owned, listed].contains(&alone) && ![owned, listed, alone].contains(&by_group), "each its own record (X8, X9)");
        let read = cat.requested(&[alone, by_group]).unwrap();
        assert_eq!((read[&alone].holder, read[&by_group].holder), (None, None), "neither had by the owned album");
    }

    /// Y1: a record made for a clash of names (`shared_name_records`, X9) is never held by its names, though the clash is over: the
    /// album of the two that went from the library may be the one its request meant. It stays Wanted.
    #[test]
    fn a_record_made_for_a_clash_of_names_is_not_held_by_its_names() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let had = [cat.record_for_tests("Glass Harbour", 2), cat.record_for_tests("Glass Harbour", 3)];
        let record = cat.record_for_request(&Wanted { artist: "Glass Harbour".into(), title: "Glass Harbour".into(), ..Wanted::default() }).unwrap();
        assert!(shared_names(&cat).iter().any(|(_, r)| *r == record), "made for the clash");
        cat.db().execute("UPDATE records SET retired_at = 5 WHERE id = ?1", [had[1]]).unwrap();
        assert_eq!(cat.requested(&[record]).unwrap()[&record].holder, None, "one album of its name left, and not had by it");
    }

    /// Y1, X9: two copies of one album (a release group in common) are one album of their name, so a record not had of that name
    /// that no reference ties to them (a Discogs list's) is still held by names, by the older copy; two had records nothing tells
    /// apart are two albums, and hold it no longer.
    #[test]
    fn two_copies_of_one_album_still_hold_by_names() {
        for grouped in [true, false] {
            let cat = Catalogue::open(None, Vec::new()).unwrap();
            let copies = [cat.record_for_tests("Two Rooms", 2), cat.record_for_tests("Two Rooms", 2)];
            if grouped {
                for c in copies {
                    assert!(cat.set_reference(&ids::record(c), &Ref::new("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000322"), "files").unwrap());
                }
            }
            let listed = cat.record_for_tests("Two Rooms", 0);
            assert!(cat.set_reference(&ids::record(listed), &Ref::new("discogs", "master", "3020"), "link").unwrap());
            assert_eq!(cat.requested(&[listed]).unwrap()[&listed].holder, grouped.then_some(copies[0]), "grouped {grouped}");
        }
    }

    /// V3 (the review of run 8b, finding 3): a record not had holding no MusicBrainz release or release group, a Bandcamp list's or a
    /// Discogs list's, is no album of its own: beside an owned album of the name, untagged or of a release group a discography's
    /// record holds too, both list records are held by it, a Beyond row of the name is had, and a request by the names alone takes
    /// the owned album.
    #[test]
    fn a_bandcamp_and_a_discogs_list_record_of_an_owned_album_read_had() {
        for tagged in [false, true] {
            let cat = Catalogue::open(None, Vec::new()).unwrap();
            let owned = cat.record_for_tests("Two Rooms", 2);
            if tagged {
                let listed = cat.record_for_tests("Two Rooms", 0);
                for r in [owned, listed] {
                    cat.db().execute("INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES (?1, 'musicbrainz', 'release-group', '00000000-0000-4000-8000-000000000330', 1, 'files')", [ids::record(r)]).unwrap();
                }
            }
            let bandcamp = cat.record_for_tests("Two Rooms", 0);
            assert!(cat.set_reference(&ids::record(bandcamp), &Ref::new("bandcamp", "album", "https://glassharbour.bandcamp.com/album/two-rooms"), "link").unwrap());
            let discogs = cat.record_for_tests("Two Rooms", 0);
            assert!(cat.set_reference(&ids::record(discogs), &Ref::new("discogs", "master", "3030"), "link").unwrap());
            let read = cat.requested(&[bandcamp, discogs]).unwrap();
            assert_eq!((read[&bandcamp].holder, read[&discogs].holder), (Some(owned), Some(owned)), "tagged {tagged}: both lists' records had");
            let row = (Ref::new("discogs", "master", "3031"), "Glass Harbour", "Two Rooms");
            assert_eq!(cat.by_reference(&[row], &[]).unwrap().named, [Some(owned)], "tagged {tagged}: Beyond's row had");
            let asked = cat.record_for_request(&Wanted { artist: "Glass Harbour".into(), title: "Two Rooms".into(), ..Wanted::default() }).unwrap();
            assert_eq!(asked, owned, "tagged {tagged}: no clash of names, the owned album by its names");
        }
    }

    /// U2 (the review of 8c/8d, finding 2): an untagged owned album beside the record MusicBrainz's discography made of it (its
    /// group) and the records Bandcamp's and Discogs' lists made of it: the discography's record is the owned album's by its names,
    /// so the three read had, a request by the names alone takes the owned album, and Beyond's Discogs row of it is had. A list's
    /// record that contradicts the discography's (another Discogs master) is another album, held by none. A second discography
    /// record of another group (Orbital's two self-titled albums): nothing says which the owned one is, and none is held.
    #[test]
    fn an_untagged_owned_album_holds_its_list_records_beside_its_musicbrainz_record() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let owned = cat.record_for_tests("Two Rooms", 2);
        let listed = |kind: Option<&str>, refs: &[(&str, &str, &str)]| {
            let r = cat.record_for_tests("Two Rooms", 0);
            cat.db().execute("UPDATE records SET kind = ?2 WHERE id = ?1", params![r, kind]).unwrap();
            for (s, k, v) in refs {
                assert!(cat.set_reference(&ids::record(r), &Ref::new(s, k, v), "link").unwrap());
            }
            r
        };
        let group = "00000000-0000-4000-8000-000000000340";
        let discography = listed(Some("album"), &[("musicbrainz", "release-group", group), ("discogs", "master", "3040")]);
        let bandcamp = listed(None, &[("bandcamp", "album", "https://glassharbour.bandcamp.com/album/two-rooms")]);
        let discogs = listed(Some("album"), &[("discogs", "release", "3041")]);
        let read = cat.requested(&[discography, bandcamp, discogs]).unwrap();
        assert_eq!((read[&discography].holder, read[&bandcamp].holder, read[&discogs].holder), (Some(owned), Some(owned), Some(owned)), "all three had");
        let row = (Ref::new("discogs", "master", "3040"), "Glass Harbour", "Two Rooms");
        let other_row = (Ref::new("discogs", "release", "3042"), "Glass Harbour", "Two Rooms");
        assert_eq!(cat.by_reference(&[row.clone(), other_row.clone()], &[]).unwrap().named, [None, Some(owned)], "the master's row is the discography's record's; a release's row had");
        let named = Wanted { artist: "Glass Harbour".into(), title: "Two Rooms".into(), ..Wanted::default() };
        assert_eq!(cat.record_for_request(&named).unwrap(), owned, "no clash: the owned album by its names");
        let another = listed(None, &[("discogs", "master", "3043")]);
        assert_eq!(cat.requested(&[another]).unwrap()[&another].holder, None, "another master than the discography's: another album");
        // Orbital: the discography's second album of the name.
        let second = listed(Some("album"), &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000341")]);
        let read = cat.requested(&[discography, bandcamp, discogs, second]).unwrap();
        assert_eq!([read[&discography].holder, read[&bandcamp].holder, read[&discogs].holder, read[&second].holder], [None; 4], "three albums: none held by names");
        let alone = cat.record_for_request(&named).unwrap();
        assert!(![owned, discography, bandcamp, discogs, second].contains(&alone), "a clash of names again");
    }

    /// U2: a single of an album's name is no second album of it (the kind class). A tagged owned album, its discography record, and a
    /// single of the name MusicBrainz lists (opened from Beyond or a label's page): Bandcamp's and Discogs' records of the album read
    /// had, a request by the names takes the owned album, and Beyond's row is had; the single is held by no album.
    #[test]
    fn a_single_of_an_albums_name_is_no_second_album() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let group = "00000000-0000-4000-8000-000000000350";
        let owned = cat.record_for_tests("Two Rooms", 2);
        assert!(cat.set_reference(&ids::record(owned), &Ref::new("musicbrainz", "release-group", group), "files").unwrap());
        let listed = |kind: Option<&str>, refs: &[(&str, &str, &str)]| {
            let r = cat.record_for_tests("Two Rooms", 0);
            cat.db().execute("UPDATE records SET kind = ?2 WHERE id = ?1", params![r, kind]).unwrap();
            for (s, k, v) in refs {
                assert!(cat.set_reference(&ids::record(r), &Ref::new(s, k, v), "link").unwrap());
            }
            r
        };
        listed(Some("album"), &[("musicbrainz", "release-group", group)]);
        let single = listed(Some("single"), &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000351")]);
        let bandcamp = listed(None, &[("bandcamp", "album", "https://glassharbour.bandcamp.com/album/two-rooms")]);
        let discogs = listed(Some("album"), &[("discogs", "master", "3050")]);
        let read = cat.requested(&[bandcamp, discogs, single]).unwrap();
        assert_eq!((read[&bandcamp].holder, read[&discogs].holder, read[&single].holder), (Some(owned), Some(owned), None), "the album's lists had; the single not");
        let row = (Ref::new("discogs", "master", "3051"), "Glass Harbour", "Two Rooms");
        assert_eq!(cat.by_reference(&[row], &[]).unwrap().named, [Some(owned)]);
        assert_eq!(cat.record_for_request(&Wanted { artist: "Glass Harbour".into(), title: "Two Rooms".into(), ..Wanted::default() }).unwrap(), owned);
    }

    /// U2: an untagged owned record beside an album and a single of its name that MusicBrainz lists may be either where the release
    /// type rule cannot place it (two tracks, their lengths not read: T10): it holds neither by its names, and a request by the names
    /// alone is a clash, as before the kind class was counted.
    #[test]
    fn an_untagged_record_beside_an_album_and_a_single_of_its_name_holds_neither() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let owned = cat.record_for_tests("Two Rooms", 2);
        let listed = |kind: &str, group: &str| {
            let r = cat.record_for_tests("Two Rooms", 0);
            cat.db().execute("UPDATE records SET kind = ?2 WHERE id = ?1", params![r, kind]).unwrap();
            assert!(cat.set_reference(&ids::record(r), &Ref::new("musicbrainz", "release-group", group), "link").unwrap());
            r
        };
        let album = listed("album", "00000000-0000-4000-8000-000000000360");
        let single = listed("single", "00000000-0000-4000-8000-000000000361");
        let read = cat.requested(&[album, single]).unwrap();
        assert_eq!((read[&album].holder, read[&single].holder), (None, None));
        let asked = cat.record_for_request(&Wanted { artist: "Glass Harbour".into(), title: "Two Rooms".into(), ..Wanted::default() }).unwrap();
        assert!(![owned, album, single].contains(&asked), "a clash of names");
    }

    /// A had record of no kind (one the files made, as every had record on production is), titled `title`: a track for each of
    /// `lengths`, each a file read that long (ms; none, not read yet).
    fn untyped(cat: &Catalogue, title: &str, lengths: &[Option<i64>]) -> i64 {
        let record = cat.record_for_tests(title, lengths.len());
        cat.copy_for_tests(record, 0);
        let db = cat.db();
        let tracks: Vec<i64> = db.prepare("SELECT id FROM tracks WHERE record = ?1 ORDER BY number").unwrap().query_map([record], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        for (track, ms) in tracks.iter().zip(lengths) {
            db.execute("UPDATE files SET duration_ms = ?2 WHERE track = ?1", params![track, ms]).unwrap();
        }
        record
    }

    /// Minutes as a file's length is read (ms).
    fn minutes(m: f64) -> Option<i64> {
        Some((m * 60_000.0) as i64)
    }

    /// A record not had of `title` that a list made, of `kind` (none: a list that says none, Bandcamp's), holding `refs`.
    fn not_had(cat: &Catalogue, title: &str, kind: Option<&str>, refs: &[(&str, &str, &str)]) -> i64 {
        let r = cat.record_for_tests(title, 0);
        cat.db().execute("UPDATE records SET kind = ?2 WHERE id = ?1", params![r, kind]).unwrap();
        for (s, k, v) in refs {
            assert!(cat.set_reference(&ids::record(r), &Ref::new(s, k, v), "link").unwrap());
        }
        r
    }

    /// The albums of a name's records as the had rule counts them (`albums_of`) for a record of the class `single`, each record read
    /// as the had rule reads it (`classes_of` among them).
    fn counted(cat: &Catalogue, named: &[i64], single: bool) -> Albums {
        let db = cat.db();
        let (refs, had, none) = (refs_of(&db, named).unwrap(), live_had_of(&db, named).unwrap(), Vec::new());
        let classes = classes_of(&db, named, &had).unwrap();
        let records: Vec<Counted> = named.iter().map(|id| (*id, refs.get(id).unwrap_or(&none).as_slice(), had.contains(id), classes[id])).collect();
        albums_of(&records, single)
    }

    /// The class the had rule reads a record by (`classes_of`).
    fn class_of(cat: &Catalogue, record: i64) -> Class {
        let db = cat.db();
        let had = live_had_of(&db, &[record]).unwrap();
        classes_of(&db, &[record], &had).unwrap()[&record]
    }

    /// Each album's records, in id order.
    fn sorted(albums: &Albums) -> Vec<Vec<i64>> {
        albums
            .of
            .iter()
            .map(|a| {
                let mut a = a.clone();
                a.sort_unstable();
                a
            })
            .collect()
    }

    /// T10.1 (run 10's review, finding 10): an untagged owned album (no kind, ten tracks, 48 minutes) beside the album MusicBrainz's
    /// discography lists of its name (D, its group), the single of its title track (S, kind single, its own group), and the records
    /// Bandcamp's (B) and Discogs' (L) lists made of it, neither with a group: by the release type rule it is an album, so it counts
    /// once and is never either class's; B, D and L are its album's and read had; S stays apart, not had.
    #[test]
    fn an_untagged_owned_album_beside_its_title_single_holds_its_list_records() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let owned = untyped(&cat, "Two Rooms", &[minutes(4.8); 10]);
        let discography = not_had(&cat, "Two Rooms", Some("album"), &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000001001")]);
        let single = not_had(&cat, "Two Rooms", Some("single"), &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000001002")]);
        let bandcamp = not_had(&cat, "Two Rooms", None, &[("bandcamp", "album", "https://glassharbour.bandcamp.com/album/two-rooms")]);
        let discogs = not_had(&cat, "Two Rooms", Some("album"), &[("discogs", "master", "4001")]);
        let all = [owned, discography, single, bandcamp, discogs];
        let albums = counted(&cat, &all, false);
        assert_eq!((sorted(&albums), albums.unsure.len()), (vec![vec![owned, discography]], 0), "one album, the owned one counted once");
        assert_eq!([albums.apart(bandcamp), albums.apart(discography), albums.apart(discogs)], [1, 0, 1], "B and L beside one album, D in it");
        let singles = counted(&cat, &all, true);
        assert_eq!((sorted(&singles), singles.unsure.len()), (vec![vec![single]], 0), "the single apart; the owned album not of its class");
        let read = cat.requested(&[bandcamp, discography, discogs, single]).unwrap();
        assert_eq!([read[&bandcamp].holder, read[&discography].holder, read[&discogs].holder], [Some(owned); 3], "B, D and L had");
        assert_eq!(read[&single].holder, None, "the single not had");
    }

    /// T10.2: the owned record untagged, two tracks of eight minutes in all: a single by the rule. It and S are one single, so S
    /// reads had; the album D and the Discogs list's album L read missing. Changed by design (RF1): Bandcamp's record B, of no kind,
    /// counts in either class, and of the single class the owned single (and S) is the name's one album: B reads had.
    #[test]
    fn an_untagged_owned_single_holds_the_single() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let owned = untyped(&cat, "Two Rooms", &[minutes(4.0); 2]);
        let discography = not_had(&cat, "Two Rooms", Some("album"), &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000001003")]);
        let single = not_had(&cat, "Two Rooms", Some("single"), &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000001004")]);
        let bandcamp = not_had(&cat, "Two Rooms", None, &[("bandcamp", "album", "https://glassharbour.bandcamp.com/album/two-rooms")]);
        let discogs = not_had(&cat, "Two Rooms", Some("album"), &[("discogs", "master", "4002")]);
        let all = [owned, discography, single, bandcamp, discogs];
        let singles = counted(&cat, &all, true);
        assert_eq!((sorted(&singles), singles.unsure.len()), (vec![vec![owned, single]], 0), "the owned single and S one, counted once");
        let albums = counted(&cat, &all, false);
        assert_eq!((sorted(&albums), albums.unsure.len()), (vec![vec![discography]], 0), "the owned single is no album of the name");
        let read = cat.requested(&[bandcamp, discography, discogs, single]).unwrap();
        assert_eq!(read[&single].holder, Some(owned), "the single had");
        assert_eq!([read[&discography].holder, read[&discogs].holder], [None; 2], "D and L missing: an album kind is set apart from a single");
        assert_eq!(read[&bandcamp].holder, Some(owned), "B, of no kind, either class's (RF1)");
    }

    /// T10.3: the rule's edges, by the rule itself (`library::type_by_files`, on `release_type_of`), never a second classifier. An
    /// untagged owned record of five tracks and 20 minutes is an EP; of two and 16 minutes, an EP; named "... EP" with two short
    /// tracks, an EP: none a single, so beside an album and a single of its name each counts once and holds the album. Two tracks
    /// whose lengths are not read yet the rule cannot place: it may be either class (`unsure`), as before, and holds nothing.
    #[test]
    fn the_untagged_record_is_classed_by_the_release_type_rule() {
        let cases: [(&str, Vec<Option<i64>>, Option<bool>); 4] = [
            ("Two Rooms", vec![minutes(4.0); 5], Some(false)),
            ("Two Rooms", vec![minutes(8.0); 2], Some(false)),
            ("Two Rooms EP", vec![minutes(4.0); 2], Some(false)),
            ("Two Rooms", vec![None; 2], None),
        ];
        for (n, (title, lengths, single)) in cases.into_iter().enumerate() {
            let cat = Catalogue::open(None, Vec::new()).unwrap();
            let owned = untyped(&cat, title, &lengths);
            let discography = not_had(&cat, title, Some("album"), &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000001011")]);
            let listed_single = not_had(&cat, title, Some("single"), &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000001012")]);
            assert_eq!(class_of(&cat, owned).single(), single, "case {n}: as the rule places it");
            let albums = counted(&cat, &[owned, discography, listed_single], false);
            let placed = single.is_some();
            assert_eq!(albums.unsure.is_empty(), placed, "case {n}: unsure only where the rule cannot place it");
            assert_eq!(albums.apart(0), if placed { 1 } else { 3 }, "case {n}: one album, or the owned one counted twice beside the album");
            assert_eq!(cat.requested(&[discography]).unwrap()[&discography].holder, placed.then_some(owned), "case {n}: the album held where placed");
        }
        // The rule itself, not a copy: the had rule asks the library's, and that is the rule every build types albums by.
        let records = include_str!("records.rs").replace("\r\n", "\n");
        let at = records.find("fn classes_of(").unwrap();
        let body = &records[at..at + records[at..].find("\n}\n").unwrap()];
        assert!(body.contains("crate::serve::library::type_by_files("), "{body}");
        let library = include_str!("../serve/library.rs").replace("\r\n", "\n");
        let at = library.find("pub fn type_by_files(").unwrap();
        let rule = &library[at..at + library[at..].find("\n}\n").unwrap()];
        assert!(rule.contains("release_type_of(None, None, &shape)"), "{rule}");
    }

    /// T10.4, the census's 156 (production's shape): a had album, untagged and of ten tracks, shadowed by the record of its name that
    /// MusicBrainz's discography made (D, its group). 1. By the rule an album: it and D are one album, D reads had and is off its
    /// artist's missing list. 2. An EP of the name listed too (E): three albums of one class (an EP is no single), and U2 joins none
    /// of three, so D and E read missing. 3. The owned record a two-track single by the rule. Changed by design (RF1): only an album
    /// kind is set apart from a single, so D reads missing and the EP, of either class, is the owned single's and reads had. Never
    /// merged by the name, and the owned record never counted twice, by either class.
    #[test]
    fn an_untagged_had_album_holds_the_not_had_record_of_its_name() {
        for case in 1..=3 {
            let cat = Catalogue::open(None, Vec::new()).unwrap();
            let artist = cat.artist_for("Glass Harbour").unwrap();
            let owned = if case == 3 { untyped(&cat, "Two Rooms", &[minutes(4.0); 2]) } else { untyped(&cat, "Two Rooms", &[minutes(4.5); 10]) };
            let listed = |kind: &str, group: &str| {
                let l = Listing { title: "Two Rooms".into(), artist: "Glass Harbour".into(), kind: Some(kind.into()), refs: vec![Ref::new("musicbrainz", "release-group", group)], ..Listing::default() };
                cat.keep_listing(Some(artist), &[l], "link").unwrap()[0]
            };
            let discography = listed("album", "00000000-0000-4000-8000-000000001021");
            let ep = (case > 1).then(|| listed("ep", "00000000-0000-4000-8000-000000001022"));
            let named: Vec<i64> = [owned, discography].into_iter().chain(ep).collect();
            for single in [false, true] {
                assert!(counted(&cat, &named, single).unsure.is_empty(), "case {case}, single {single}: the owned record never counted twice");
            }
            let missing: Vec<i64> = cat.missing_of(artist).unwrap().iter().map(|q| q.row.id).collect();
            let holder = cat.requested(&[discography]).unwrap()[&discography].holder;
            if case == 1 {
                assert_eq!(sorted(&counted(&cat, &named, false)), [vec![owned, discography]], "one album, listed once");
                assert_eq!((holder, missing), (Some(owned), Vec::new()), "D had, not missing beside the owned album");
            } else if case == 2 {
                assert_eq!((holder, missing), (None, [Some(discography), ep].into_iter().flatten().collect::<Vec<_>>()), "case {case}: D and E missing");
            } else {
                let ep = ep.unwrap();
                assert_eq!((holder, missing), (None, vec![discography]), "case 3: D missing beside the owned single");
                assert_eq!(cat.requested(&[ep]).unwrap()[&ep].holder, Some(owned), "case 3: the EP the owned single's (RF1)");
            }
            let merged: i64 = cat.db().query_row("SELECT count(*) FROM records WHERE merged_into IS NOT NULL", [], |r| r.get(0)).unwrap();
            assert_eq!(merged, 0, "case {case}: nothing merged by the name");
        }
    }

    /// As a build leaves a record's album typed (RF1, `Catalogue::keep_types`).
    fn built(cat: &Catalogue, record: i64, kind: &str) {
        cat.keep_types(&[(record, kind.to_string())]).unwrap();
    }

    /// RF1 (RB1's second scenario): the owner's untagged "Spiral", three tracks of 13 minutes, which its build typed a single. A record
    /// not had of no kind of its name, Bandcamp's list's and a request's made by its names before the album came, counts in either
    /// class: the owned single holds both, Bandcamp's is off the artist's missing list, Beyond's row of the name is had, and a Want by
    /// the names is the owned record. Never: the Discogs list's record of kind album held by it (only an album kind is set apart from
    /// a single by the rule).
    #[test]
    fn a_record_of_no_kind_is_held_by_an_owned_single() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let artist = cat.artist_for("Glass Harbour").unwrap();
        let owned = untyped(&cat, "Spiral", &[minutes(4.4); 3]);
        built(&cat, owned, "single");
        let listed = |kind: Option<&str>, r: Ref| {
            let l = Listing { title: "Spiral".into(), artist: "Glass Harbour".into(), kind: kind.map(str::to_string), refs: vec![r], ..Listing::default() };
            cat.keep_listing(Some(artist), &[l], "link").unwrap()[0]
        };
        let bandcamp = listed(None, Ref::new("bandcamp", "album", "https://glassharbour.bandcamp.com/album/spiral"));
        let asked = cat.record_for_tests("Spiral", 0);
        let discogs = listed(Some("album"), Ref::new("discogs", "master", "5101"));
        let read = cat.requested(&[bandcamp, asked, discogs]).unwrap();
        assert_eq!((read[&bandcamp].holder, read[&asked].holder), (Some(owned), Some(owned)), "of no kind: the owned single's");
        assert_eq!(read[&discogs].holder, None, "an album kind set apart from a single");
        let missing: Vec<i64> = cat.missing_of(artist).unwrap().iter().map(|q| q.row.id).collect();
        assert_eq!(missing, [discogs], "Bandcamp's off the missing list, the album on it");
        let row = (Ref::new("discogs", "master", "5102"), "Glass Harbour", "Spiral");
        assert_eq!(cat.by_reference(&[row], &[]).unwrap().named, [Some(owned)], "Beyond's row had");
        let named = Wanted { artist: "Glass Harbour".into(), title: "Spiral".into(), ..Wanted::default() };
        assert_eq!(cat.record_for_request(&named).unwrap(), owned, "a Want by its names: the owned record, no clash");
    }

    /// RF1 (RB1's first scenario): the same owned single, and the EP of its name MusicBrainz's discography lists (kind EP, its group).
    /// MusicBrainz's EP covers a 2-track 12": of either class, it is the owned single's. It reads had and is off the artist's missing
    /// list, and a Want of it by its group takes its record, held, so nothing is fetched of what the owner has.
    #[test]
    fn an_ep_of_its_name_is_held_by_an_owned_single() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let artist = cat.artist_for("Glass Harbour").unwrap();
        let owned = untyped(&cat, "Spiral", &[minutes(4.4); 3]);
        built(&cat, owned, "single");
        let group = "00000000-0000-4000-8000-000000001101";
        let l = Listing { title: "Spiral".into(), artist: "Glass Harbour".into(), kind: Some("ep".into()), refs: vec![Ref::new("musicbrainz", "release-group", group)], ..Listing::default() };
        let ep = cat.keep_listing(Some(artist), &[l], "link").unwrap()[0];
        assert_eq!(cat.requested(&[ep]).unwrap()[&ep].holder, Some(owned), "the EP had");
        assert!(cat.missing_of(artist).unwrap().is_empty(), "off the missing list");
        let wanted = cat.record_for_request(&Wanted { group: Some(group.into()), artist: "Glass Harbour".into(), title: "Spiral".into(), ..Wanted::default() }).unwrap();
        assert_eq!(wanted, ep, "a Want by its group: its record");
        assert_eq!(cat.requested(&[wanted]).unwrap()[&wanted].holder, Some(owned), "held: done, nothing fetched");
    }

    /// RF1, the rule's point 5: a Single said (one song in versions) is an EP at most, however long, so a record of kind single counts
    /// in either class. The owner's untagged five versions of "Two Rooms", built an EP (20 minutes), hold the single MusicBrainz lists
    /// of the name. Built an album, the same record never does: no album is a Single said.
    #[test]
    fn a_single_said_is_held_by_an_owned_ep_never_by_an_album() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let owned = untyped(&cat, "Two Rooms", &[minutes(4.0); 5]);
        built(&cat, owned, "ep");
        let single = not_had(&cat, "Two Rooms", Some("single"), &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000001111")]);
        assert_eq!(cat.requested(&[single]).unwrap()[&single].holder, Some(owned), "the EP holds the single said");
        built(&cat, owned, "album");
        assert_eq!(cat.typed_for_tests(owned).as_deref(), Some("album"));
        assert_eq!(cat.requested(&[single]).unwrap()[&single].holder, None, "an album never");
    }

    /// V3: two albums of one name, each of its own release group (Orbital's 1991 album owned, its 1993 one a discography's), are two
    /// still: a Bandcamp list's record of the name is held by neither, and a request by the names alone takes a record of its own
    /// (X8, X9), held by neither (Y1).
    #[test]
    fn two_albums_of_one_name_by_their_groups_are_still_a_clash() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let owned = cat.record_for_tests("Glass Harbour", 2);
        assert!(cat.set_reference(&ids::record(owned), &Ref::new("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000331"), "files").unwrap());
        let listed = cat.record_for_tests("Glass Harbour", 0);
        assert!(cat.set_reference(&ids::record(listed), &Ref::new("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000332"), "link").unwrap());
        let bandcamp = cat.record_for_tests("Glass Harbour", 0);
        assert!(cat.set_reference(&ids::record(bandcamp), &Ref::new("bandcamp", "album", "https://glassharbour.bandcamp.com/album/glass-harbour"), "link").unwrap());
        let alone = cat.record_for_request(&Wanted { artist: "Glass Harbour".into(), title: "Glass Harbour".into(), ..Wanted::default() }).unwrap();
        assert!(![owned, listed, bandcamp].contains(&alone), "a clash of names: its own record");
        let read = cat.requested(&[bandcamp, alone]).unwrap();
        assert_eq!((read[&bandcamp].holder, read[&alone].holder), (None, None), "neither held by the owned album");
    }

    /// R21 (N1): a self-titled Want by its release group is not another self-titled album of the artist the library holds.
    #[test]
    fn a_self_titled_want_is_not_another_self_titled_album() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let owned = cat.record_for_tests("Glass Harbour", 2);
        assert!(cat.set_reference(&ids::record(owned), &Ref::new("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000202"), "files").unwrap());
        let wanted = crate::app::requests::record_of_album(&cat, Some("00000000-0000-4000-8000-000000000203"), "Glass Harbour", "Glass Harbour", None, None).unwrap();
        assert_ne!(wanted, owned);
    }

    /// W1: a Want's Bandcamp page finds the live record that holds it, merges followed, before its names: the record its list
    /// showed, whatever it is called there.
    #[test]
    fn a_wants_page_finds_the_record_that_holds_it() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let page = "https://kitetheory.bandcamp.com/album/night-walks";
        let held = cat.record_for_tests("Night Walks (Live)", 0);
        assert!(cat.set_reference(&ids::record(held), &Ref::new("bandcamp", "album", page), "link").unwrap());
        let wanted = Wanted { artist: "Kite Theory".into(), title: "Night Walks".into(), page: Some(page.into()), ..Wanted::default() };
        assert_eq!(cat.record_for_request(&wanted).unwrap(), held);
        assert_eq!(cat.record_named_by(None, Some(page)).unwrap(), Some(held));
    }

    /// R28 (N11): of two albums under one loose name, the import matcher's loose step finds the older, as its exact step does, the
    /// albums being added newest first (the step's look-alikes, W2).
    #[test]
    fn the_loose_matcher_keeps_the_older_record() {
        let mut m = crate::app::import::Matcher::default();
        m.add_album("2", "Tide Tables (Live)", Some("Glass Harbour"));
        m.add_album("1", "Tide Tables (Remaster)", Some("Glass Harbour"));
        assert_eq!(m.find_album_by("Tide Tables (Deluxe)", "The Glass Harbour"), Some("1"));
    }

    /// R26 (N8): a request's record holding a release group and its own release (a deluxe edition from a label's list) is not had
    /// by the standard edition the library holds, which names another release of the group; a record of the group alone is had by it.
    #[test]
    fn a_deluxe_request_is_not_had_by_the_standard_edition() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let group = "00000000-0000-4000-8000-000000000220";
        let (deluxe, standard) = ("00000000-0000-4000-8000-000000000221", "00000000-0000-4000-8000-000000000222");
        let refs = |record: i64, of: &[(&str, &str)], how: &str| {
            for (kind, value) in of {
                assert!(cat.set_reference(&ids::record(record), &Ref::new("musicbrainz", kind, value), how).unwrap());
            }
        };
        let owned = cat.record_for_tests("Tide Tables", 2);
        refs(owned, &[("release-group", group), ("release", standard)], "files");
        let asked = cat.record_for_tests("Tide Tables (Deluxe Edition)", 0);
        refs(asked, &[("release-group", group), ("release", deluxe)], "link");
        let plain = cat.record_for_tests("Tide Tables (asked for)", 0);
        refs(plain, &[("release-group", group)], "link");
        let read = cat.requested(&[asked, plain]).unwrap();
        assert_eq!((read[&asked].holder, read[&plain].holder), (None, Some(owned)));
    }

    /// R14 (run 7): a request's record is kept for good, so it never takes another artist's album by its title alone, though the
    /// library holds one album of that title (the import matcher's last step, which the request does without).
    #[test]
    fn a_request_never_takes_another_artists_album_by_its_title() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let had = cat.record_for_tests("Tide Tables", 2);
        let asked = crate::app::requests::record_of_album(&cat, None, "Orbit Lane", "Tide Tables", None, None).unwrap();
        assert_ne!(asked, had);
        assert!(!cat.record(asked).unwrap().unwrap().had);
    }

    /// R13 (F2): a Want by a release group a retired record holds (the owner rejected its copy) names the live record that holds
    /// it, had before not had; the retired one only where no live record holds the group (OQ16).
    #[test]
    fn a_request_names_the_live_record_of_its_group_over_a_retired_one() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let group = "00000000-0000-4000-8000-000000000141";
        let want = || crate::app::requests::record_of_album(&cat, Some(group), "Glass Harbour", "Undertow", None, None).unwrap();
        let holding = |record: i64, how: &str| assert!(cat.set_reference(&ids::record(record), &Ref::new("musicbrainz", "release-group", group), how).unwrap());
        let gone = cat.record_for_tests("Undertow", 2);
        holding(gone, "files");
        cat.db().execute("UPDATE records SET retired_at = 5 WHERE id = ?1", [gone]).unwrap();
        assert_eq!(want(), gone, "no live record holds it: the retired one");
        let wanted = cat.record_for_tests("Undertow (asked for)", 0);
        holding(wanted, "link");
        assert_eq!(want(), wanted, "a live one over the retired");
        let back = cat.record_for_tests("Undertow", 2);
        holding(back, "files");
        assert_eq!(want(), back, "had before not had");
    }

    /// A record of this title, not had.
    fn record(cat: &Catalogue, title: &str) -> String {
        let db = cat.db();
        db.execute("INSERT INTO records (born, title, artist) VALUES (1, ?1, 'Glass Harbour')", [title]).unwrap();
        ids::record(db.last_insert_rowid())
    }

    /// 0.16.2's catalogue (FX01's), opened by today's: the references' tables there, its records' rows as they were, the new
    /// columns empty, and a second opening the same.
    #[test]
    fn the_tables_open_on_a_0_16_catalogue() {
        let d = std::env::temp_dir().join(format!("cratarr-records-0-16-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        crate::fixtures::data_0_16(&d).unwrap();
        let file = d.join("catalogue.sqlite");
        let rows = |db: &Connection| -> Vec<(i64, String, Option<i64>)> {
            db.prepare("SELECT id, title, retired_at FROM records ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
        };
        let before = rows(&Connection::open(&file).unwrap());
        assert!(before.len() >= 6, "{before:?}");
        for _ in 0..2 {
            let cat = Catalogue::open(Some(&file), Vec::new()).unwrap();
            assert_eq!(rows(&cat.db()), before);
            for table in ["references", "reference_tracks"] {
                assert!(cat.db().prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1").unwrap().exists([table]).unwrap(), "{table}");
            }
            let tide = cat.record(1).unwrap().unwrap();
            assert_eq!((tide.title.as_str(), tide.artist.as_str(), tide.year, tide.had), ("Tide Tables", "Glass Harbour", Some(2012), true));
            assert_eq!((tide.counting, tide.counted_by_hand, tide.checked_by_hand, tide.merged_into), (None, false, false, None), "nothing worked out yet");
            assert!(tide.released.is_none() && tide.genres.is_empty() && tide.cover.is_none());
            let retired = cat.record(6).unwrap().unwrap();
            assert!(retired.retired_at.is_some() && !retired.had, "a retired record keeps its row, and has no track there");
            assert_eq!(cat.record(999).unwrap(), None);
            // Written to, as the step will.
            assert!(cat.set_reference(&ids::record(1), &Ref::new("musicbrainz", "release", "00000000-0000-4000-8000-000000000001"), "files").unwrap());
        }
        std::fs::remove_dir_all(&d).ok();
    }

    /// G1: a deluxe edition and the standard one are two records of one release group and one master.
    #[test]
    fn two_records_may_share_a_release_group() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (standard, deluxe) = (record(&cat, "Tide Tables"), record(&cat, "Tide Tables (Deluxe)"));
        for shared in [Ref::new("musicbrainz", "release-group", "rg-1"), Ref::new("discogs", "master", "1000001")] {
            assert!(cat.set_reference(&standard, &shared, "files").unwrap());
            assert!(cat.set_reference(&deluxe, &shared, "search").unwrap(), "{shared:?}");
            assert_eq!(cat.subject_of(&shared).unwrap(), [standard.clone(), deluxe.clone()]);
        }
        // Each its own release.
        let (one, two) = (Ref::new("musicbrainz", "release", "r-1"), Ref::new("musicbrainz", "release", "r-2"));
        assert!(cat.set_reference(&standard, &one, "files").unwrap() && cat.set_reference(&deluxe, &two, "files").unwrap());
        assert!(!cat.set_reference(&deluxe, &one, "files").unwrap(), "a release is one record's: the first keeps it");
        assert_eq!(cat.subject_of(&one).unwrap(), [standard.clone()]);
        assert_eq!(cat.references_of(&deluxe).unwrap().iter().map(|(r, _)| r.kind.as_str()).collect::<Vec<_>>(), ["master", "release", "release-group"]);
    }

    /// A link finds the one subject it names, a record's, an artist's or a label's, and finding it again changes nothing but
    /// for a surer way.
    #[test]
    fn a_reference_finds_its_subject() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (tide, other) = (record(&cat, "Tide Tables"), record(&cat, "Another"));
        let page = Ref::new("bandcamp", "album", "https://glassharbour.bandcamp.com/album/tide-tables");
        let artist = Ref::new("musicbrainz", "artist", "00000000-0000-4000-8000-000000000201");
        let label = Ref::new("discogs", "label", "23528");
        assert!(cat.set_reference(&tide, &page, "search").unwrap());
        assert!(cat.set_reference("ar-4", &artist, "files").unwrap());
        assert!(cat.set_reference("la-2", &label, "link").unwrap());
        assert_eq!(cat.subject_of(&page).unwrap(), [tide.clone()]);
        assert_eq!(cat.subject_of(&artist).unwrap(), ["ar-4"]);
        assert_eq!(cat.subject_of(&label).unwrap(), ["la-2"]);
        assert!(cat.subject_of(&Ref::new("bandcamp", "album", "https://elsewhere.bandcamp.com/album/x")).unwrap().is_empty());
        assert!(cat.subject_of(&Ref::new("discogs", "artist", "23528")).unwrap().is_empty(), "the kind is part of it");

        let how = |subject: &str| cat.references_of(subject).unwrap().into_iter().map(|(r, row)| (r.value, row.how)).collect::<Vec<_>>();
        let first = cat.references_of(&tide).unwrap()[0].1.at;
        cat.set_reference(&tide, &page, "files").unwrap();
        assert_eq!(how(&tide), [(page.value.clone(), "files".to_string())], "a surer way");
        cat.set_reference(&tide, &page, "search").unwrap();
        assert_eq!(how(&tide), [(page.value.clone(), "files".to_string())], "a guess does not take its place");
        assert_eq!(cat.references_of(&tide).unwrap()[0].1.at, first, "found when it was first found");

        assert!(!cat.set_reference(&other, &page, "search").unwrap(), "held by another: a guess is no link");
        assert!(cat.set_reference(&other, &page, "hand").unwrap(), "a link pasted by hand names what it names");
        assert_eq!(cat.subject_of(&page).unwrap(), [other.clone()]);
        assert!(how(&tide).is_empty(), "the other let go of it");
        assert!(!cat.set_reference(&tide, &page, "files").unwrap(), "and a file's tag does not take it back");
    }

    /// G3, OQ13 (a): two records found to be one; the one with copies keeps its id and all the other knew, and the other's id
    /// answers by it for good, through a chain of merges too. Two with copies are one only by their files.
    #[test]
    fn a_merged_record_answers_by_its_new_id() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (had, not_had) = (cat.record_for_tests("Tide Tables", 2), cat.record_for_tests("Tide Tables", 0));
        let (had_id, not_had_id) = (ids::record(had), ids::record(not_had));
        let (group, release) = (Ref::new("musicbrainz", "release-group", "g1"), Ref::new("musicbrainz", "release", "r1"));
        let page = Ref::new("bandcamp", "album", "https://glassharbour.bandcamp.com/album/tide-tables");
        cat.set_reference(&had_id, &group, "files").unwrap();
        cat.set_reference(&had_id, &release, "files").unwrap();
        cat.set_reference(&not_had_id, &group, "link").unwrap();
        cat.set_reference(&not_had_id, &page, "search").unwrap();
        cat.confirm_reference(&not_had_id, "bandcamp").unwrap();
        let list = [RefTrack { disc: 1, position: 1, title: "Slack Water".into(), length_ms: None, recording: None }];
        cat.set_reference_tracks(not_had, "bandcamp", &list).unwrap();
        cat.set_counting(not_had, Some("bandcamp"), true).unwrap();
        cat.set_checked_by_hand(not_had, true).unwrap();
        cat.db().execute("UPDATE records SET released = '2012-03-05', genres = 'ambient' WHERE id = ?1", [not_had]).unwrap();

        assert_eq!(cat.merge(not_had, had).unwrap(), had, "the one with copies keeps its id, whichever is named first");
        let row = cat.record(not_had).unwrap().unwrap();
        assert_eq!(row.id, had, "the old id answers by the new");
        assert_eq!((row.counting.as_deref(), row.counted_by_hand, row.checked_by_hand), (Some("bandcamp"), true, true), "its choices came along");
        assert_eq!((row.released.as_deref(), row.genres.as_slice()), (Some("2012-03-05"), &["ambient".to_string()][..]), "and what the record lacked");
        assert_eq!(cat.subject_of(&page).unwrap(), [had_id.clone()]);
        assert!(cat.references_of(&not_had_id).unwrap().is_empty());
        let how = cat.references_of(&had_id).unwrap().into_iter().map(|(r, row)| (r.kind, row.how, row.confirmed)).collect::<Vec<_>>();
        let by = |kind: &str, how: &str, confirmed: bool| (kind.to_string(), how.to_string(), confirmed);
        assert_eq!(how, [by("album", "search", true), by("release", "files", false), by("release-group", "files", false)], "one group, kept the surer way");
        assert_eq!(cat.reference_tracks(had).unwrap(), [("bandcamp".to_string(), list.to_vec())]);

        let third = cat.record_for_tests("Tide Tables", 0);
        assert_eq!(cat.merge(third, not_had).unwrap(), had, "merged into one that was merged: into where it went");
        assert_eq!(cat.record(third).unwrap().map(|r| r.id), Some(had));
        let fourth = cat.record_for_tests("Tide Tables", 0);
        cat.merge_records(fourth, not_had).unwrap();
        assert_eq!(cat.record(fourth).unwrap().map(|r| r.id), Some(had), "merged by hand into a merged one: the chain is followed");
        let other = cat.record_for_tests("Other", 1);
        assert!(cat.merge(had, other).is_err(), "two with copies");
        assert_eq!(cat.record(had).unwrap().map(|r| r.id), Some(had));
    }

    /// A list's records found by their references, else by a link of the record's name, else records not had of their own,
    /// credited to the list's artist; read again, the same ones (C47, C48b).
    #[test]
    fn a_listing_finds_its_record_by_a_link_and_is_a_record_not_had_otherwise() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let tide = cat.record_for_tests("Tide Tables", 2);
        cat.set_reference(&ids::record(tide), &Ref::new("discogs", "release", "2000001"), "search").unwrap();
        let artist = cat.artist_for("Glass Harbour").unwrap();
        let listing = |title: &str, group: &str, says: &[Ref]| Listing {
            title: title.into(),
            artist: "Glass Harbour".into(),
            year: Some(2016),
            kind: Some("album".into()),
            released: Some("2016-09-30".into()),
            genres: vec!["ambient".into()],
            refs: vec![Ref::new("musicbrainz", "release-group", group)],
            says: says.to_vec(),
            ..Listing::default()
        };
        let list = [listing("Tide Tables", "g1", &[Ref::new("discogs", "release", "2000001")]), listing("Deep Water", "g2", &[]), listing("Tide Tables", "g3", &[])];
        let kept = cat.keep_listing(Some(artist), &list, "link").unwrap();
        assert_eq!(kept[0], tide, "MusicBrainz's link to the record's Discogs release");
        assert!(kept[1] != tide && kept[2] != tide && kept[1] != kept[2], "a name alone is no link: Tide Tables of another group is a record of its own");
        let deep = cat.record(kept[1]).unwrap().unwrap();
        assert_eq!((deep.title.as_str(), deep.year, deep.kind.as_deref(), deep.had), ("Deep Water", Some(2016), Some("album"), false));
        assert_eq!(cat.record_artists(kept[1]).unwrap(), [artist]);
        assert!(cat.references_of(&ids::record(tide)).unwrap().iter().any(|(r, row)| r.value == "g1" && row.how == "link"), "the record found learns its group");
        assert_eq!(cat.keep_listing(Some(artist), &list, "link").unwrap(), kept, "read again: the same records");
    }

    /// R1: a link pasted by hand is taken by nothing but another hand, whatever takes; a retired record holds no release, and
    /// gives up one it held to the live record that names it.
    #[test]
    fn a_hand_is_never_taken_and_a_retired_record_holds_no_release() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (pasted, matched, retired) = (ids::record(cat.record_for_tests("Two Rooms", 2)), ids::record(cat.record_for_tests("Two Rooms", 2)), cat.record_for_tests("Partial Shore", 0));
        let release = Ref::new("musicbrainz", "release", "r2");
        assert!(cat.set_reference(&pasted, &release, "hand").unwrap());
        assert!(!cat.take_reference(&matched, &release, "search").unwrap(), "a certain match takes no hand");
        assert!(!cat.take_reference(&matched, &release, "files").unwrap());
        assert_eq!(cat.subject_of(&release).unwrap(), [pasted.clone()]);
        assert!(cat.set_reference(&matched, &release, "hand").unwrap(), "a hand takes from a hand");
        assert_eq!(cat.subject_of(&release).unwrap(), [matched.clone()]);

        let (old, shore) = (ids::record(retired), Ref::new("musicbrainz", "release", "r6"));
        assert!(cat.set_reference(&old, &shore, "hand").unwrap(), "held while it lives");
        assert!(cat.set_reference(&old, &Ref::new("musicbrainz", "release-group", "g6"), "files").unwrap());
        cat.db().execute("UPDATE records SET retired_at = 5 WHERE id = ?1", [retired]).unwrap();
        assert!(!cat.set_reference(&old, &Ref::new("musicbrainz", "release", "r7"), "hand").unwrap(), "retired: no release, even by hand");
        let live = ids::record(cat.record_for_tests("Partial Shore", 3));
        assert!(cat.set_reference(&live, &shore, "files").unwrap(), "a retired record gives up its release to the live one, even a hand's");
        assert_eq!(cat.subject_of(&shore).unwrap(), [live]);
        assert_eq!(cat.references_of(&old).unwrap().into_iter().map(|(r, _)| r.kind).collect::<Vec<_>>(), ["release-group"], "it keeps its group");
    }

    /// A discography's listing for a test: one album of `title`, by its release group.
    fn group_listing(title: &str, group: &str) -> Listing {
        Listing { title: title.into(), kind: Some("album".into()), refs: vec![Ref::new("musicbrainz", "release-group", group)], ..Default::default() }
    }

    /// The step's discographies alone, carried: `discographies` and nothing else.
    fn carry_discographies(cat: &Catalogue, discographies: Vec<CarriedDiscography>) -> serde_json::Value {
        let read = discographies.len() as u64;
        cat.carry_references(&Carry { discographies, read: vec![("discography", read)], ..Carry::default() }, &|_| Ok(())).unwrap();
        cat.references_carried().unwrap().unwrap()
    }

    /// R6 (N1): a collaboration's files ("A & B", two ids) name each artist by their own id, as a solo album's do; no artist of
    /// the two names together is made, and each discography's records not had are credited to its own artist.
    #[test]
    fn a_collaboration_names_each_artist_by_their_own_id() {
        let cat = crate::catalogue::tests::tracks(3);
        let mbid = |n: u32| format!("00000000-0000-4000-8000-{n:012}");
        for (track, rel, artist, ids) in [(1, "A/01.flac", "Glass Harbour & Tape Club", format!("{}; {}", mbid(201), mbid(204))), (2, "B/01.flac", "Glass Harbour", mbid(201)), (3, "C/01.flac", "Earth, Wind & Fire", mbid(209))] {
            crate::catalogue::tests::file_of_track(&cat, rel, track);
            cat.tag_for_tests(0, rel, &[("albumartist", artist), ("musicbrainz_albumartistid", &ids)]);
        }
        let discography = |n: u32, title: &str, group: u32| CarriedDiscography { mbid: mbid(n), at: 7, listings: vec![group_listing(title, &mbid(group))], names: Vec::new() };
        let report = carry_discographies(&cat, vec![discography(204, "Lost Tapes", 115), discography(201, "Harbour Lights", 111), discography(209, "Gratitude", 120)]);
        let held = |name: &str| cat.artist_of(name).unwrap().map(|id| cat.references_of(&ids::artist(id)).unwrap().into_iter().map(|(r, _)| r.value).collect::<Vec<_>>());
        assert_eq!(held("Tape Club"), Some(vec![mbid(204)]), "named by the collaboration's own pairing");
        assert_eq!(held("Glass Harbour"), Some(vec![mbid(201)]));
        assert_eq!(cat.artist_of("Glass Harbour & Tape Club").unwrap(), None, "no joint artist");
        assert_eq!(cat.artist_of("Earth").unwrap(), None, "three names for one id pair with nothing");
        let credited = |group: u32| cat.subject_of(&Ref::new("musicbrainz", "release-group", &mbid(group))).unwrap().iter().map(|s| cat.record_artists(ids::record_number(s).unwrap()).unwrap()).collect::<Vec<_>>();
        assert_eq!(credited(115), [vec![cat.artist_of("Tape Club").unwrap().unwrap()]]);
        assert!(credited(120).is_empty(), "{report}");
    }

    /// R6 (N13): a discography whose artist's name is held by an artist with another MusicBrainz id is two artists of one name:
    /// listed, no records made, and the id the artist held stays theirs alone.
    #[test]
    fn a_name_holding_another_id_makes_no_records() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let mbid = |n: u32| format!("00000000-0000-4000-8000-{n:012}");
        let rone = cat.artist_for("Rone").unwrap();
        assert!(cat.set_reference(&ids::artist(rone), &Ref::new("musicbrainz", "artist", &mbid(301)), "files").unwrap());
        let other = CarriedDiscography { mbid: mbid(302), at: 7, listings: vec![group_listing("Elsewhere", &mbid(130))], names: vec![("Rone".into(), "link")] };
        let report = carry_discographies(&cat, vec![other]);
        assert!(cat.subject_of(&Ref::new("musicbrainz", "release-group", &mbid(130))).unwrap().is_empty(), "no record made");
        assert_eq!(cat.references_of(&ids::artist(rone)).unwrap().into_iter().map(|(r, _)| r.value).collect::<Vec<_>>(), [mbid(301)]);
        let shared = report["unmatched"].as_array().unwrap().iter().find(|u| u["what"] == SHARED_NAME).cloned().unwrap();
        assert_eq!(shared["sample"], serde_json::json!([format!("{} Rone", mbid(302))]), "{report}");
    }

    /// R9 (N10): a merge into a record merged before goes to where that one went, every reference with it; a retired record
    /// takes none, and nothing of the record is lost for it.
    #[test]
    fn merging_into_a_merged_record_keeps_every_reference() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let [from, between, end] = ["Ferry Lights", "Ferry Lights (2)", "Ferry Lights (3)"].map(|t| cat.record_for_tests(t, 0));
        cat.merge_records(between, end).unwrap();
        let (release, page) = (Ref::new("musicbrainz", "release", "r1"), Ref::new("bandcamp", "album", "https://kitetheory.bandcamp.com/album/ferry-lights"));
        for r in [&release, &page] {
            assert!(cat.set_reference(&ids::record(from), r, "search").unwrap());
        }
        cat.merge_records(from, between).unwrap();
        assert_eq!((cat.subject_of(&release).unwrap(), cat.subject_of(&page).unwrap()), (vec![ids::record(end)], vec![ids::record(end)]));
        assert_eq!(cat.record(from).unwrap().unwrap().id, end);

        let (kept, gone) = (cat.record_for_tests("Signal", 0), cat.record_for_tests("Signal (old)", 0));
        assert!(cat.set_reference(&ids::record(kept), &Ref::new("musicbrainz", "release", "r7"), "search").unwrap());
        cat.db().execute("UPDATE records SET retired_at = 5 WHERE id = ?1", [gone]).unwrap();
        assert!(cat.merge_records(kept, gone).is_err(), "a retired record takes no merge");
        assert_eq!(cat.subject_of(&Ref::new("musicbrainz", "release", "r7")).unwrap(), [ids::record(kept)], "nothing lost");
    }

    /// R9 (N8): a label a merged record brought (from a list, not the files) survives the next build: the build replaces only
    /// its own rows.
    #[test]
    fn a_merged_records_label_survives_the_next_build() {
        let cat = crate::catalogue::tests::tracks(2);
        let copy = super::super::AlbumCopy { labels: vec![("Low Tide Records".into(), "LT-1".into())], artists: vec!["Glass Harbour".into()], ..crate::catalogue::tests::copy(None, "Glass Harbour", "Tide Tables", &[(1, "One", 200_000), (2, "Two", 200_000)]) };
        cat.assign_records(std::slice::from_ref(&copy)).unwrap();
        let had = crate::catalogue::tests::record_of(&cat, 1).unwrap();
        let listed = cat.record_for_tests("Tide Tables", 0);
        let lantern = cat.label_for("Paper Lantern").unwrap();
        cat.db().execute("INSERT INTO record_labels (record, label, catno, how) VALUES (?1, ?2, 'PL-3', 'listing')", params![listed, lantern]).unwrap();
        cat.merge_records(listed, had).unwrap();
        cat.keep_files_names(std::slice::from_ref(&copy)).unwrap();
        let labels: Vec<(String, String)> = cat.label_records(lantern).unwrap().into_iter().map(|(r, catno)| (crate::serve::ids::record(r), catno)).collect();
        assert_eq!(labels, [(ids::record(had), "PL-3".to_string())], "the list's label kept");
        let low_tide = cat.label_for("Low Tide Records").unwrap();
        assert_eq!(cat.label_records(low_tide).unwrap().len(), 1, "the files' own, as before");
    }

    /// A refused write deletes nothing: every holder is weighed before any lets go, so a guess held beside a hand (made here
    /// with the one-holder index dropped, the guess first) is still there after a certain match is refused by the hand.
    #[test]
    fn a_refused_write_deletes_nothing() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let [guess, pasted, matched] = ["Guess", "Pasted", "Match"].map(|t| ids::record(cat.record_for_tests(t, 1)));
        assert!(guess < pasted, "the guess comes first");
        cat.db().execute_batch("DROP INDEX references_one").unwrap();
        for (subject, how) in [(&guess, "search"), (&pasted, "hand")] {
            cat.db().execute("INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES (?1, 'musicbrainz', 'release', 'r1', 1, ?2)", params![subject, how]).unwrap();
        }
        let release = Ref::new("musicbrainz", "release", "r1");
        assert!(!cat.take_reference(&matched, &release, "files").unwrap(), "a hand holds it");
        assert_eq!(cat.subject_of(&release).unwrap(), [guess, pasted], "nothing let go of");
    }

    /// Each source's list kept by record, and the count's source: worked out until chosen by hand, then the hand's.
    #[test]
    fn a_records_lists_and_the_one_that_counts_are_kept() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let id: i64 = record(&cat, "Tide Tables")[3..].parse().unwrap();
        let track = |position: u32, title: &str, length_ms: Option<i64>, recording: Option<&str>| RefTrack { disc: 1, position, title: title.into(), length_ms, recording: recording.map(str::to_string) };
        let mb = vec![track(1, "Slack Water", Some(181_000), Some("00000000-0000-4000-8000-000000000301")), track(2, "Neap", None, None)];
        let discogs = vec![track(1, "Slack Water", None, None)];
        cat.set_reference_tracks(id, "musicbrainz", &mb).unwrap();
        cat.set_reference_tracks(id, "discogs", &discogs).unwrap();
        cat.set_reference_tracks(id, "discogs", &mb[..1]).unwrap();
        assert_eq!(cat.reference_tracks(id).unwrap(), [("discogs".to_string(), mb[..1].to_vec()), ("musicbrainz".to_string(), mb.clone())], "read again, the new list");
        assert!(cat.reference_tracks(id + 1).unwrap().is_empty());

        let counts = || cat.record(id).unwrap().map(|r| (r.counting, r.counted_by_hand)).unwrap();
        assert!(cat.set_counting(id, Some("musicbrainz"), false).unwrap());
        assert_eq!(counts(), (Some("musicbrainz".to_string()), false));
        assert!(cat.set_counting(id, Some("discogs"), true).unwrap());
        assert!(!cat.set_counting(id, Some("bandcamp"), false).unwrap(), "worked out, it never replaces the hand's");
        assert_eq!(counts(), (Some("discogs".to_string()), true));
        assert!(cat.set_counting(id, None, true).unwrap());
        assert_eq!(counts(), (None, false), "taken back by hand: to be worked out again");
        assert!(cat.set_counting(id, Some("bandcamp"), false).unwrap());
        assert_eq!(counts(), (Some("bandcamp".to_string()), false));
    }

    /// RT01's clock in the tests: now, and a month.
    const NOW: i64 = 1_800_000_000;
    const MONTH: i64 = 30 * 86_400;

    /// A record not had of `title`, last seen `months` before `NOW`; none: never since 0.17.
    fn unseen_for(cat: &Catalogue, title: &str, months: Option<i64>) -> i64 {
        let record = cat.record_for_tests(title, 0);
        cat.db().execute("UPDATE records SET seen_at = ?2 WHERE id = ?1", params![record, months.map(|m| NOW - m * MONTH)]).unwrap();
        record
    }

    /// What the forgetting takes at `NOW`, nothing kept from outside the catalogue.
    fn forget(cat: &Catalogue) -> Vec<i64> {
        cat.forget_quiet(NOW, &std::collections::BTreeSet::new()).unwrap()
    }

    /// RT01: a record not had that no list showed and nobody opened for six months is forgotten, with its references, lists and
    /// credits, and leaves a tombstone; one seen or opened since stays, a had one always, and one from before 0.17 (not seen
    /// since) until six months after the first pass; one never seen made since, six months after it was made. Its id is never
    /// given again.
    #[test]
    fn a_record_nobody_touched_for_six_months_is_forgotten() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let artist = cat.artist_for("Glass Harbour").unwrap();
        let had = cat.record_for_tests("Tide Tables", 2);
        cat.db().execute("UPDATE records SET seen_at = ?2 WHERE id = ?1", params![had, NOW - 7 * MONTH]).unwrap();
        let (recent, opened, before) = (unseen_for(&cat, "Two Rooms", Some(5)), unseen_for(&cat, "Partial Shore", Some(7)), unseen_for(&cat, "Ferry Lights", None));
        cat.db().execute("UPDATE records SET opened_at = ?2 WHERE id = ?1", params![opened, NOW - MONTH]).unwrap();
        // The last made, so an id given again would be its own.
        let stale = credited(&cat, artist, "Undertow", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000310")]);
        let label = cat.label_for("Low Tide Records").unwrap();
        cat.db().execute("INSERT INTO record_labels (record, label, catno, how) VALUES (?1, ?2, 'LT-2', 'listing')", params![stale, label]).unwrap();
        cat.set_reference_tracks(stale, "musicbrainz", &[RefTrack { disc: 1, position: 1, title: "Neap".into(), length_ms: None, recording: None }]).unwrap();
        cat.db().execute("UPDATE records SET seen_at = ?2 WHERE id = ?1", params![stale, NOW - 7 * MONTH]).unwrap();

        assert_eq!(forget(&cat), [stale]);
        assert_eq!(cat.forgotten_at(&ids::record(stale)).unwrap(), Some(NOW), "its tombstone");
        assert!(cat.record(stale).unwrap().is_none());
        assert!(cat.references_of(&ids::record(stale)).unwrap().is_empty() && cat.reference_tracks(stale).unwrap().is_empty(), "its references and lists gone");
        assert!(cat.record_artists(stale).unwrap().is_empty() && cat.label_records(label).unwrap().is_empty(), "its credits gone");
        for kept in [had, recent, opened, before] {
            assert!(cat.record(kept).unwrap().is_some() && cat.forgotten_at(&ids::record(kept)).unwrap().is_none(), "{kept} kept");
        }
        // Made a month after the first pass, never seen.
        let since = unseen_for(&cat, "Open Water", None);
        cat.db().execute("UPDATE records SET born = ?2 WHERE id = ?1", params![since, NOW + MONTH]).unwrap();
        assert!(since > stale, "its id never given again");
        let later = cat.forget_quiet(NOW + 7 * MONTH, &std::collections::BTreeSet::new()).unwrap();
        assert!(later.contains(&before) && !later.contains(&had), "from before 0.17: six months after the first pass");
        assert!(!later.contains(&since), "made since: six months after it was made");
    }

    /// Z5: a Want opens the record it names, in the transaction that names it, so the forgetting, which gathers the requests that
    /// keep a record before its own transaction, keeps it six months from the Want; a record no one wanted goes.
    #[test]
    fn a_want_opens_its_record() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let group = "00000000-0000-4000-8000-000000000315";
        let (wanted, other) = (cat.record_for_tests("Undertow", 0), cat.record_for_tests("Open Water", 0));
        assert!(cat.set_reference(&ids::record(wanted), &Ref::new("musicbrainz", "release-group", group), "link").unwrap());
        let before = now();
        cat.db().execute("UPDATE records SET seen_at = ?1 WHERE id IN (?2, ?3)", params![before - 7 * MONTH, wanted, other]).unwrap();
        let w = Wanted { group: Some(group.into()), artist: "Glass Harbour".into(), title: "Undertow".into(), ..Wanted::default() };
        assert_eq!(cat.record_for_request(&w).unwrap(), wanted);
        let opened: Option<i64> = cat.db().query_row("SELECT opened_at FROM records WHERE id = ?1", [wanted], |r| r.get(0)).unwrap();
        assert!(opened.is_some_and(|at| at >= before), "opened by the Want");
        assert_eq!(cat.forget_quiet(before + 5 * MONTH, &std::collections::BTreeSet::new()).unwrap(), [other], "kept six months from the Want");
    }

    /// RT01: a reference pasted or chosen by hand keeps its record not had, however long nobody saw it; one a link found does not.
    #[test]
    fn a_hand_reference_keeps_its_record() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (pasted, linked) = (unseen_for(&cat, "Undertow", Some(7)), unseen_for(&cat, "Open Water", Some(7)));
        assert!(cat.set_reference(&ids::record(pasted), &Ref::new("bandcamp", "album", "https://glassharbour.bandcamp.com/album/undertow"), "hand").unwrap());
        assert!(cat.set_reference(&ids::record(linked), &Ref::new("bandcamp", "album", "https://glassharbour.bandcamp.com/album/open-water"), "link").unwrap());
        assert_eq!(forget(&cat), [linked]);
    }

    /// RT01: a count or a check chosen by hand (0.14's "Use this", "Checked by hand") keeps its record not had.
    #[test]
    fn a_choice_keeps_its_record() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let [counted, checked, other] = ["Undertow", "Open Water", "Two Rooms"].map(|t| unseen_for(&cat, t, Some(7)));
        assert!(cat.set_counting(counted, Some("musicbrainz"), true).unwrap());
        cat.set_checked_by_hand(checked, true).unwrap();
        assert_eq!(forget(&cat), [other]);
    }

    /// RT01: a guess confirmed once (0.14's "That's it") keeps its record not had; one never confirmed does not.
    #[test]
    fn a_confirmation_keeps_its_record() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (confirmed, guessed) = (unseen_for(&cat, "Undertow", Some(7)), unseen_for(&cat, "Open Water", Some(7)));
        for (record, group) in [(confirmed, "00000000-0000-4000-8000-000000000311"), (guessed, "00000000-0000-4000-8000-000000000312")] {
            assert!(cat.set_reference(&ids::record(record), &Ref::new("musicbrainz", "release-group", group), "search").unwrap());
        }
        assert_eq!(cat.confirm_reference(&ids::record(confirmed), "musicbrainz").unwrap(), 1);
        assert_eq!(forget(&cat), [guessed]);
    }

    /// RT01, G3: a merge keeps both its records: the one merged, whose id answers through the other, and the one it went into.
    #[test]
    fn a_merge_keeps_both_its_records() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let [from, into, other] = ["Undertow (2)", "Undertow", "Open Water"].map(|t| unseen_for(&cat, t, Some(7)));
        cat.merge_records(from, into).unwrap();
        assert_eq!(forget(&cat), [other]);
        assert_eq!(cat.record(from).unwrap().map(|r| r.id), Some(into), "the merged one answers through the other");
    }

    /// RT01, C50: a folder a fetcher placed for a record not had keeps it while the folder is expected.
    #[test]
    fn an_expected_folder_keeps_its_record() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (expected, other) = (unseen_for(&cat, "Undertow", Some(7)), unseen_for(&cat, "Open Water", Some(7)));
        assert!(cat.expect_record(1, "soulseek/Glass Harbour - Undertow", expected).unwrap());
        assert_eq!(forget(&cat), [other]);
    }

    /// RT01: an id from before 0.16 that went to a record (`album_moved`) keeps it: an old link opens it.
    #[test]
    fn a_link_from_before_0_16_keeps_its_record() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (moved, other) = (unseen_for(&cat, "Undertow", Some(7)), unseen_for(&cat, "Open Water", Some(7)));
        cat.db().execute("INSERT INTO album_moved (old, form, record, folder, rank) VALUES ('5f2b9c', 'id', ?1, 0, 0)", [moved]).unwrap();
        assert_eq!(forget(&cat), [other]);
    }

    /// RT01, OQ16: a retired record ("Was in Music until…") is never forgotten.
    #[test]
    fn a_retired_record_is_never_forgotten() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (retired, other) = (unseen_for(&cat, "Undertow", Some(7)), unseen_for(&cat, "Open Water", Some(7)));
        cat.db().execute("UPDATE records SET retired_at = 5 WHERE id = ?1", [retired]).unwrap();
        assert_eq!(forget(&cat), [other]);
    }

    /// Y12: the records of a Want's names are found by the names each row keeps, as X8 found them by folding every live record:
    /// their artist and title folded; a row renamed found by its new names and no longer by its old; a retired one left out.
    #[test]
    fn a_wants_names_find_their_records_by_the_kept_names() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let db = cat.db();
        let made = |artist: &str, title: &str| {
            db.execute("INSERT INTO records (born, title, artist) VALUES (1, ?1, ?2)", params![title, artist]).unwrap();
            db.last_insert_rowid()
        };
        let (a, b, other, gone) = (made("Glass Harbour", "Two Rooms"), made("GLASS-HARBOUR", "Two Rooms!"), made("Glass Harbour", "Open Water"), made("Glass Harbour", "Two Rooms"));
        db.execute("UPDATE records SET retired_at = 5 WHERE id = ?1", [gone]).unwrap();
        let names = names_of("glass harbour", "two rooms");
        assert_eq!(live_named_in(&db, &names).unwrap(), [a, b], "folded, as the clash folds them; the retired one left out");
        db.execute("UPDATE records SET title = 'Two Rooms' WHERE id = ?1", [other]).unwrap();
        db.execute("UPDATE records SET title = 'Open Water' WHERE id = ?1", [a]).unwrap();
        assert_eq!(live_named_in(&db, &names).unwrap(), [b, other], "renamed: by its new names, no longer by its old");
    }

    /// The shared names (X9) as kept: (names, record).
    fn shared_names(cat: &Catalogue) -> Vec<(String, i64)> {
        let db = cat.db();
        let mut st = db.prepare("SELECT names, record FROM shared_name_records ORDER BY names").unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    }

    /// RT01, X9: the record a request's shared name names is kept: the next request by that name takes it.
    #[test]
    fn a_shared_name_row_keeps_its_record() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (named, other) = (unseen_for(&cat, "Glass Harbour", Some(7)), unseen_for(&cat, "Open Water", Some(7)));
        cat.db().execute("INSERT INTO shared_name_records (names, record) VALUES ('glass harbour|glass harbour', ?1)", [named]).unwrap();
        assert_eq!(forget(&cat), [other]);
        assert_eq!(shared_names(&cat), [("glass harbour|glass harbour".to_string(), named)]);
    }

    /// RT01, X9: a shared name whose record is gone, however it went, is let go of by the forgetting; a live record's stays.
    #[test]
    fn shared_name_rows_of_gone_records_are_pruned() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let live = unseen_for(&cat, "Open Water", Some(1));
        for (names, record) in [("glass harbour|open water", live), ("glass harbour|lost tapes", live + 100)] {
            cat.db().execute("INSERT INTO shared_name_records (names, record) VALUES (?1, ?2)", params![names, record]).unwrap();
        }
        assert!(forget(&cat).is_empty());
        assert_eq!(shared_names(&cat), [("glass harbour|open water".to_string(), live)]);
    }

    /// RT01: a list read sees what it lists (`keep_listing`), read again too, and never makes a record seen earlier than it was;
    /// what was seen is kept from the forgetting.
    #[test]
    fn a_list_read_refreshes_what_it_shows() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let artist = cat.artist_for("Glass Harbour").unwrap();
        let group = [("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000313")];
        let read_at = now();
        let listed = credited(&cat, artist, "Undertow", &group);
        let seen_at = || cat.db().query_row("SELECT seen_at FROM records WHERE id = ?1", [listed], |r| r.get::<_, Option<i64>>(0)).unwrap();
        assert!(seen_at().is_some_and(|at| at >= read_at), "a list read: seen now");
        cat.db().execute("UPDATE records SET seen_at = 5 WHERE id = ?1", [listed]).unwrap();
        assert_eq!(credited(&cat, artist, "Undertow", &group), listed, "read again: the same record");
        assert!(seen_at().is_some_and(|at| at >= read_at), "and seen again");
        // Seen later than this read, a month ahead of the real clock (Z7: not the tests' fixed one, which the real clock passes in
        // January 2027): read again, it stays.
        let later = now() + MONTH;
        cat.db().execute("UPDATE records SET seen_at = ?2 WHERE id = ?1", params![listed, later]).unwrap();
        assert_eq!(credited(&cat, artist, "Undertow", &group), listed);
        assert_eq!(seen_at(), Some(later), "never seen earlier than it was");
        assert!(cat.forget_quiet(later + 5 * MONTH, &std::collections::BTreeSet::new()).unwrap().is_empty(), "seen five months before: kept");
    }

    /// Z10: an artist is forgotten a day after their last credit went, not a day after they were given their id: one a retag took
    /// their last credit from (the build), and one whose only record not had was forgotten, each born long before, are kept within
    /// the day and go a day on; the name the retag credits stays.
    #[test]
    fn an_artist_is_forgotten_a_day_after_their_last_credit_went() {
        const DAY: i64 = 86_400;
        let cat = crate::catalogue::tests::tracks(2);
        let album = |artist: &str| crate::catalogue::AlbumCopy {
            artists: vec![artist.to_string()],
            ..crate::catalogue::tests::copy(None, artist, "Tide Tables", &[(1, "One", 200_000), (2, "Two", 180_000)])
        };
        cat.assign_records(&[album("The Grouch")]).unwrap();
        let (grouch, lonely) = (cat.artist_of("The Grouch").unwrap().unwrap(), cat.artist_for("Orbit Lane").unwrap());
        let quiet = credited(&cat, lonely, "Undertow", &[]);
        cat.db().execute("UPDATE records SET seen_at = 1 WHERE id = ?1", [quiet]).unwrap();
        cat.db().execute("UPDATE artists SET born = 1", []).unwrap();
        let went = now();
        // The owner's plan retags The Grouch's album "Grouch" (0092); the forgetting takes Orbit Lane's only record.
        cat.assign_records(&[album("Grouch")]).unwrap();
        assert_eq!(cat.forget_quiet(went, &std::collections::BTreeSet::new()).unwrap(), [quiet]);
        let none = std::collections::BTreeSet::new();
        assert!(cat.forget_quiet_artists(went + DAY - 1, &none).unwrap().is_empty(), "within the day their last credit went: kept");
        assert_eq!(cat.forget_quiet_artists(went + 2 * DAY, &none).unwrap(), [grouch, lonely], "a day on, still with nothing");
        assert!(cat.artist_of("Grouch").unwrap().is_some(), "the name the retag credits stays");
    }

    /// RT01b: an artist a record credits, a reference pasted by hand or confirmed names, or an id from before 0.16 went to is
    /// kept, and so is one given an id within the day; one with nothing goes, their tombstone written, and their name credited
    /// again is a new artist.
    #[test]
    fn an_artist_a_record_a_hand_or_an_old_id_names_is_kept() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let [credited, pasted, confirmed, moved, new, lonely] =
            ["Glass Harbour", "Orbit Lane", "Tape Club", "Kite Theory", "Noor Field", "Paper Lantern"].map(|n| cat.artist_for(n).unwrap());
        // Born a month before the tests' clock, whatever the real one says (Z7); `new` within the day, below.
        cat.db().execute("UPDATE artists SET born = ?1", [NOW - MONTH]).unwrap();
        let record = cat.record_for_tests("Tide Tables", 0);
        cat.db().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'listing')", params![record, credited]).unwrap();
        assert!(cat.set_reference(&ids::artist(pasted), &Ref::new("musicbrainz", "artist", "00000000-0000-4000-8000-000000000401"), "hand").unwrap());
        assert!(cat.set_reference(&ids::artist(confirmed), &Ref::new("musicbrainz", "artist", "00000000-0000-4000-8000-000000000402"), "search").unwrap());
        assert_eq!(cat.confirm_reference(&ids::artist(confirmed), "musicbrainz").unwrap(), 1);
        cat.db().execute("INSERT INTO artist_moved (old, id) VALUES ('ar-5vcUvYmuWJAgyxLXEUmaPz', ?1)", [moved]).unwrap();
        cat.db().execute("UPDATE artists SET born = ?2 WHERE id = ?1", params![new, NOW - 3600]).unwrap();
        assert_eq!(cat.forget_quiet_artists(NOW, &std::collections::BTreeSet::new()).unwrap(), [lonely]);
        assert_eq!(cat.forgotten_at(&ids::artist(lonely)).unwrap(), Some(NOW), "their tombstone");
        assert_eq!(cat.artist_of("Paper Lantern").unwrap(), None, "their row gone");
        assert!(cat.artist_for("Paper Lantern").unwrap() > lonely, "named again: a new id, never theirs");
    }

    /// Air's MusicBrainz id in T7's tests.
    const AIR_ID: &str = "00000000-0000-4000-8000-000000000701";

    /// T7's shape (the review of run 10, finding 7): the library's "Air", credited on a had record where `air_had`; "Air (2)", made as
    /// a card's open makes them (G7: not sure, so a new artist of the name), holding Air's id found `how`, credited on two records
    /// their list made and, where `theirs_had`, on a had record. Air and Air (2).
    fn air_and_air_2(cat: &Catalogue, reference: &Ref, air_had: bool, how: &str, theirs_had: bool) -> (i64, i64) {
        let credit = |record: i64, artist: i64| {
            cat.db().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'files')", params![record, artist]).unwrap();
        };
        let air = cat.artist_for("Air").unwrap();
        if air_had {
            credit(cat.record_for_tests("Moon Safari", 2), air);
        }
        let second = cat.artist_by_reference(reference, "Air", how, false).unwrap();
        assert_ne!(second, air, "not sure: Air (2)");
        let listing = |title: &str| Listing { title: title.into(), artist: "Air".into(), kind: Some("album".into()), ..Listing::default() };
        assert_eq!(cat.keep_listing(Some(second), &[listing("Talkie Walkie"), listing("Pocket Symphony")], "link").unwrap().len(), 2);
        if theirs_had {
            credit(cat.record_for_tests("Le Voyage Dans La Lune", 2), second);
        }
        (air, second)
    }

    /// The monthly read's put of the id it found by the artist's name (`discover::read_one`'s `set_reference`, `search`), as it ends.
    fn monthly_read(cat: &Catalogue, artist: i64, reference: &Ref) -> Put {
        let mut db = cat.db();
        let tx = db.transaction().unwrap();
        let put = put_reference(&tx, &ids::artist(artist), reference, "search", NOW, false).unwrap();
        tx.commit().unwrap();
        put
    }

    /// The records an artist is credited on, and how.
    fn credits_of(cat: &Catalogue, artist: i64) -> Vec<(i64, String)> {
        let db = cat.db();
        let mut st = db.prepare("SELECT record, how FROM record_artists WHERE artist = ?1 ORDER BY record").unwrap();
        st.query_map([artist], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    }

    /// Who holds a reference, and how each found it.
    fn holders_of(cat: &Catalogue, r: &Ref) -> Vec<(String, String)> {
        let db = cat.db();
        let mut st = db.prepare("SELECT subject, how FROM \"references\" WHERE source = ?1 AND kind = ?2 AND value = ?3 ORDER BY subject").unwrap();
        st.query_map(params![r.source, r.kind, r.value], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    }

    /// T7.1 (the review of run 10, finding 7): the monthly read finds the library's "Air" by their name and puts Air's id on them,
    /// which "Air (2)" (a card's open, G7) holds by `search` and no had record: taken (`Put::Kept`), with the records Air (2)'s list
    /// made, credited `listing` (U4). One artist holds the id.
    #[test]
    fn a_cards_artist_is_the_library_artist_where_the_name_is_sure() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let reference = Ref::new("musicbrainz", "artist", AIR_ID);
        let (air, second) = air_and_air_2(&cat, &reference, true, "search", false);
        let listed: Vec<(i64, String)> = credits_of(&cat, second);
        assert!(listed.len() == 2 && listed.iter().all(|(_, how)| how == "listing"), "two listing credits: {listed:?}");
        let mut both = [credits_of(&cat, air), listed].concat();
        both.sort();
        assert_eq!(monthly_read(&cat, air, &reference), Put::Kept(vec![ids::artist(second)]));
        assert_eq!(holders_of(&cat, &reference), [(ids::artist(air), "search".to_string())], "Air holds it, alone");
        assert!(cat.references_of(&ids::artist(second)).unwrap().is_empty(), "Air (2) holds none");
        assert!(credits_of(&cat, second).is_empty(), "Air (2)'s listing credits moved");
        assert_eq!(credits_of(&cat, air), both, "to Air");
    }

    /// T7.2: the monthly read takes nothing where "Air (2)" is credited on a had record, or holds the id by hand or by their files;
    /// nor (T7's default) for a library "Air" with no had record of their own. `Put::Held` each time, and nothing moves.
    #[test]
    fn an_id_held_by_an_artist_with_a_had_record_stays() {
        let cases = [
            ("Air (2) credited on a had record", true, "search", true),
            ("held by hand", true, "hand", false),
            ("held by their files", true, "files", false),
            ("Air with no had record", false, "search", false),
        ];
        for (case, air_had, how, theirs_had) in cases {
            let cat = Catalogue::open(None, Vec::new()).unwrap();
            let reference = Ref::new("musicbrainz", "artist", AIR_ID);
            let (air, second) = air_and_air_2(&cat, &reference, air_had, how, theirs_had);
            let before = (credits_of(&cat, air), credits_of(&cat, second));
            assert_eq!(monthly_read(&cat, air, &reference), Put::Held, "{case}");
            assert_eq!(holders_of(&cat, &reference), [(ids::artist(second), how.to_string())], "{case}: still Air (2)'s");
            assert_eq!((credits_of(&cat, air), credits_of(&cat, second)), before, "{case}: nothing moves");
        }
    }
}
