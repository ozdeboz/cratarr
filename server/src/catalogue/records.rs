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
-- C50: a folder a fetcher placed for a record not had (`expect_record`): the album that arrives there is that record, its tracks
-- given to it. By the folder's id, which is for life, and the album's folder in it; forgotten after a week unclaimed.
CREATE TABLE IF NOT EXISTS expected (
  folder INTEGER NOT NULL, dir TEXT NOT NULL, record INTEGER NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (folder, dir)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS lists_to_read (record INTEGER PRIMARY KEY, release TEXT NOT NULL, at INTEGER NOT NULL, tries INTEGER NOT NULL DEFAULT 0);
";

/// A chain of merges longer than this is a loop: the record it began at answers none.
const MERGES: usize = 32;

/// The record `id` is now, its merges followed. None: no such id, or merges that go round.
fn merged_end(db: &Connection, mut id: i64) -> Result<Option<i64>> {
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

/// `Catalogue::keep_listing` in the caller's transaction, found at `at`.
pub(super) fn keep_listing_in(tx: &Connection, artist: Option<i64>, list: &[Listing], how: &str, at: i64) -> Result<Vec<i64>> {
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
                let record = tx.last_insert_rowid();
                if let Some(artist) = artist {
                    tx.prepare_cached("INSERT OR IGNORE INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'listing')")?.execute(params![record, artist])?;
                }
                record
            }
        };
        for r in &l.refs {
            put_reference(tx, &ids::record(record), r, how, at, false)?;
        }
        out.push(record);
    }
    Ok(out)
}

/// The record a listing's references name: one a release or an album page names first, else the oldest of its release group
/// or master (G1: any of them is it, for a list that names the group), each as merged now.
fn found_by_reference(tx: &Connection, l: &Listing) -> Result<Option<i64>> {
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

fn record_in(db: &Connection, id: i64) -> Result<Option<RecordRow>> {
    let mut st = db.prepare_cached(
        "SELECT id, COALESCE(title, ''), COALESCE(artist, ''), year, kind, released, genres, cover, counting, counted_by_hand, checked_by_hand, merged_into, retired_at,
           EXISTS (SELECT 1 FROM tracks t WHERE t.record = records.id AND t.retired_at IS NULL)
         FROM records WHERE id = ?1",
    )?;
    Ok(st
        .query_row([id], |r| {
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
        })
        .optional()?)
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

/// A record's MusicBrainz list once its files let go of the release it was read for (R8): gone, unless the count was chosen by
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
        if !now.contains(&(subject.clone(), kind.as_str(), value.as_str())) {
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
/// nothing; what its files say now takes the place of what they said before.
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
    for (record, a) in first {
        let mut ids: Vec<i64> = Vec::new();
        for name in &a.artists {
            if let Some(id) = artists.get(&ids::artist_key(name))
                && !ids.contains(id)
            {
                ids.push(*id);
            }
        }
        if held_artists.get(&record).map_or(&[][..], Vec::as_slice) != ids.as_slice() {
            tx.prepare_cached("DELETE FROM record_artists WHERE record = ?1 AND how = 'files'")?.execute([record])?;
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
        if held_labels.get(&record).map_or(&[][..], Vec::as_slice) != named.as_slice() {
            tx.prepare_cached("DELETE FROM record_labels WHERE record = ?1 AND how = 'files'")?.execute([record])?;
            for (label, catno) in &named {
                tx.prepare_cached("INSERT INTO record_labels (record, label, catno, how) VALUES (?1, ?2, ?3, 'files') ON CONFLICT (record, label, catno) DO UPDATE SET how = 'files'")?
                    .execute(params![record, label, catno])?;
            }
        }
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
/// or where `takes` (a certain match), and never from a hand by anything else: a link pasted names what it names.
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
        for (holder, held_how) in &holders {
            if !retired(tx, holder)? && ((held_how == "hand" && how != "hand") || (!takes && how != "hand")) {
                return Ok(Put::Held);
            }
        }
        for (holder, _) in holders {
            tx.prepare_cached("DELETE FROM \"references\" WHERE source = ?1 AND kind = ?2 AND value = ?3 AND subject = ?4")?.execute(params![r.source, r.kind, r.value, holder])?;
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

fn subjects_in(db: &Connection, r: &Ref) -> Result<Vec<String>> {
    let mut st = db.prepare_cached("SELECT subject FROM \"references\" WHERE source = ?1 AND kind = ?2 AND value = ?3 ORDER BY subject")?;
    let rows = st.query_map(params![r.source, r.kind, r.value], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

impl Catalogue {
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
    /// records of its name, else a record not had of its own, credited to `artist` where the list is one artist's. Its
    /// references are kept, found `how`. Their ids, in the list's order. Blocking.
    #[cfg_attr(not(test), allow(dead_code))] // an artist's and a label's listings as they are read (C53a, C56)
    pub fn keep_listing(&self, artist: Option<i64>, list: &[Listing], how: &str) -> Result<Vec<i64>> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let kept = keep_listing_in(&tx, artist, list, how, now())?;
        tx.commit()?;
        Ok(kept)
    }

    /// Two records found to be one (G3, OQ13 (a)): the one with copies keeps its id, else the older; the other answers through
    /// `merged_into` for good. The id kept. Blocking.
    #[cfg_attr(not(test), allow(dead_code))] // a tracklist read that shows two records are one (C51a)
    pub fn merge(&self, a: i64, b: i64) -> Result<i64> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let (a, b) = (merged_end(&tx, a)?.context("no such record")?, merged_end(&tx, b)?.context("no such record")?);
        let into = match (had(&tx, a)?, had(&tx, b)?) {
            (true, false) => a,
            (false, true) => b,
            _ => a.min(b),
        };
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
               AND EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL)
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

    /// Each source's tracklist of a record, as kept, by source. Blocking.
    pub fn reference_tracks(&self, record: i64) -> Result<Vec<(String, Vec<RefTrack>)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT source, tracks FROM reference_tracks WHERE record = ?1 ORDER BY source")?;
        let rows: Vec<(String, String)> = st.query_map([record], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        rows.into_iter().map(|(source, tracks)| Ok((source, serde_json::from_str(&tracks)?))).collect()
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
    pub title: String,
    pub label: Option<String>,
    pub year: Option<i32>,
    pub cover: Option<String>,
    pub page: Option<String>,
}

/// The record an album asked for is, in the caller's transaction (C49a): the record its release group names (the oldest, each
/// followed through its merges; under OQ16 a retired record is its group's), else the oldest live record had of its names,
/// else a new record not had, credited to its artist (`listing`). Its label, where a label named it, credited too (`listing`);
/// its Bandcamp album page, where it has no release group, a reference (`search`), so Soulseek can judge by Bandcamp's list
/// (C51b). Its id, and whether it was made now.
pub(super) fn request_record_in(tx: &Connection, w: &Wanted, at: i64) -> Result<(i64, bool)> {
    use super::identity::fold;
    let group = w.group.as_deref().map(str::to_ascii_lowercase).filter(|g| is_mbid(g)).map(|g| Ref::new("musicbrainz", "release-group", &g));
    let by_group = match &group {
        Some(g) => found_by_reference(tx, &Listing { refs: vec![g.clone()], ..Listing::default() })?,
        None => None,
    };
    let by_names = || -> Result<Option<i64>> {
        let (artist, title) = (fold(&w.artist), fold(&w.title));
        let mut st = tx.prepare_cached(
            "SELECT r.id, COALESCE(r.artist, ''), COALESCE(r.title, '') FROM records r
             WHERE r.merged_into IS NULL AND r.retired_at IS NULL AND EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL)
             ORDER BY r.id",
        )?;
        let rows: Vec<(i64, String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(rows.into_iter().find(|(_, a, t)| !title.is_empty() && fold(a) == artist && fold(t) == title).map(|(id, ..)| id))
    };
    let found = match by_group {
        Some(record) => Some(record),
        None => by_names()?,
    };
    let (record, made) = match found {
        Some(record) => (record, false),
        None => {
            let artist = match w.artist.trim() {
                "" => None,
                name => Some(super::artists::artists_in(tx, &std::collections::BTreeSet::from([name.to_string()]), at)?[&ids::artist_key(name)]),
            };
            let listing = Listing { title: w.title.clone(), artist: w.artist.clone(), year: w.year, cover: w.cover.clone(), refs: group.clone().into_iter().collect(), ..Listing::default() };
            (keep_listing_in(tx, artist, &[listing], "link", at)?[0], true)
        }
    };
    if let Some(label) = w.label.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        let id = super::labels::labels_in(tx, &std::collections::BTreeSet::from([label.to_string()]), at)?[&ids::artist_key(label)];
        tx.prepare_cached("INSERT OR IGNORE INTO record_labels (record, label, catno, how) VALUES (?1, ?2, '', 'listing')")?.execute(params![record, id])?;
    }
    if group.is_none()
        && let Some(page) = w.page.as_deref()
    {
        put_reference(tx, &ids::record(record), &Ref::new("bandcamp", "album", page), "search", at, false)?;
    }
    Ok((record, made))
}

/// How long a folder expected for a record waits for its album (`expect_record`).
const EXPECTED_SECS: i64 = 7 * 86_400;

/// The folders expected for records (C50), by (folder, the album's folder in it), those older than `EXPECTED_SECS` let go of
/// first, in the record pass's transaction.
pub(super) fn expected_in(tx: &Connection, now: i64) -> Result<std::collections::HashMap<(i64, String), i64>> {
    tx.execute("DELETE FROM expected WHERE at < ?1", [now - EXPECTED_SECS])?;
    let mut st = tx.prepare_cached("SELECT folder, dir, record FROM expected")?;
    let rows = st.query_map([], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The record not had a new album is (C50), where it is one, held in turn: the record expected in its folder (a fetcher placed
/// it there for it, `expect_record`), else a record not had its files' release or release group names, else one of its names
/// whose kept tracklist agrees with its files (`identity::agrees`: names alone never join two records). Merges are followed, and
/// the oldest is taken where several are. Never a retired record: under OQ16 a retired record stays its group's record not
/// had, but an album that arrives is new tracks, and makes a record of its own, as before 0.17.
pub(super) fn wanted_record(tx: &Connection, a: &super::AlbumCopy, expected: Option<i64>) -> Result<Option<i64>> {
    use super::identity::{agrees, fold};
    // A record that can take an album's tracks: live, not retired, with none of its own.
    let open = |record: i64| -> Result<Option<i64>> {
        let Some(id) = merged_end(tx, record)? else { return Ok(None) };
        let free: bool = tx.query_row(
            "SELECT r.retired_at IS NULL AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL) FROM records r WHERE r.id = ?1",
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
    let named: Vec<i64> = {
        let mut st = tx.prepare_cached(
            "SELECT r.id, COALESCE(r.artist, ''), COALESCE(r.title, '') FROM records r
             WHERE r.merged_into IS NULL AND r.retired_at IS NULL AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id) ORDER BY r.id",
        )?;
        let rows: Vec<(i64, String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        rows.into_iter().filter(|(_, artist, title)| !a.facts.title.is_empty() && fold(artist) == a.facts.artist && fold(title) == a.facts.title).map(|(id, ..)| id).collect()
    };
    for id in named {
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
            let kept = keep_listing_in(self.tx, Some(artist), &listings, "link", d.at)?;
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
                "SELECT r.id, COALESCE(r.artist, ''), COALESCE(r.title, ''), EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL)
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
        let pages: std::collections::HashMap<&str, &str> = c.pages.iter().map(|p| (p.key.as_str(), p.page.as_str())).collect();
        let mut given: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
        let (mut made, mut paged) = (0, 0);
        for r in &c.requests {
            let named = crate::bandcamp::label_release(&r.artist, &r.album, None);
            let page = r.group.is_none().then(|| pages.get(r.key.as_str()).map(|p| p.to_string())).flatten();
            paged += u64::from(page.is_some());
            let w = Wanted { group: r.group.clone(), artist: named.artist, title: named.title, label: named.label, year: r.year, cover: r.cover.clone(), page };
            let (record, new) = request_record_in(self.tx, &w, r.at)?;
            made += u64::from(new);
            given.insert(r.id, record);
            self.moved += 1;
        }
        self.tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![REQUEST_RECORDS, serde_json::to_string(&given)?])?;
        self.counts = vec![("records_made_for_requests", made), ("pages_for_requests", paged)];
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
    let mut w = Carrying { tx, refused: Vec::new(), retired: Vec::new(), taken: Vec::new(), unmatched: std::collections::BTreeMap::new(), moved: 0, tables: Vec::new(), counts: Vec::new() };
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
    }
    Ok(report)
}

impl Catalogue {
    /// The album a fetcher placed in `dir` of catalogue folder `folder` is `record` (C50, `wanted_record`): when the record pass
    /// meets it as a new album, its tracks are given to that record, a record not had, and the expectation is used up. Forgotten
    /// after a week unclaimed. Blocking.
    #[cfg_attr(not(test), allow(dead_code))] // Soulseek's `place` (C51b)
    pub fn expect_record(&self, folder: i64, dir: &str, record: i64) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO expected (folder, dir, record, at) VALUES (?1, ?2, ?3, ?4)", params![folder, dir.trim_matches('/'), record, now()])?;
        Ok(())
    }

    /// The records 0.17's step gave the requests (C49a), by request id, for the user data's `request.record`. Blocking.
    pub fn request_records_017(&self) -> Result<Vec<(i64, i64)>> {
        let Some(text) = self.meta(REQUEST_RECORDS) else { return Ok(Vec::new()) };
        let given: std::collections::BTreeMap<i64, i64> = serde_json::from_str(&text)?;
        Ok(given.into_iter().collect())
    }

    /// The record an album asked for is, found or made (`request_record_in`, C49a). Blocking.
    pub fn record_for_request(&self, w: &Wanted) -> Result<i64> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (record, _) = request_record_in(&tx, w, now())?;
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
    /// finds its mark inside it. `after` hears each part done, inside its transaction: an error from it leaves that part
    /// unwritten, the parts before it done. The parts run now. Blocking.
    pub fn carry_references(&self, c: &Carry, after: &dyn Fn(&str) -> Result<()>) -> Result<Vec<&'static str>> {
        let mut db = self.db();
        let mut ran = Vec::new();
        for part in PARTS_017 {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let mark = mark_017(part);
            if tx.query_row("SELECT 1 FROM meta WHERE key = ?1", [&mark], |_| Ok(())).optional()?.is_some() {
                continue;
            }
            let report = carry_part(&tx, c, part, now())?;
            after(part)?;
            tx.execute("INSERT INTO meta (key, value) VALUES (?1, ?2)", params![mark, report.to_string()])?;
            tx.commit()?;
            ran.push(part);
        }
        Ok(ran)
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
}
