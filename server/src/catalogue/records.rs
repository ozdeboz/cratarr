//! A record's references (decision 0100, step 7 of docs/records-and-copies.md): its id at each source that knows it, and the
//! tracklist each gives, beside the record's own row. A record had or not, one a discography names, the same rows.
//!
//! - **A link finds its subject.** A source's release, album page, artist or label is one subject's alone, so the record a pasted
//!   link or a file's tag names is found by it. A release group or a master is not: a deluxe edition and the standard one are one
//!   group and one master, and two records by the same-record rule, so every record of one is found (G1).
//! - **Kept as found, the hand first.** A reference is kept with when it was first found and how (`files`, `link`, `search`,
//!   `hand`): found again it stays as it was, but for a surer way, and a hand never gives way to a guess. A reference one subject
//!   holds alone goes to another only by hand.
//! - **The tracklist that counts** is the record's `counting`: worked out from the lists and the files (`identity::counting`)
//!   until one is chosen by hand (0.14's "Use this"), which a worked-out one never replaces.
//!
//! The table is `"references"`, quoted in every statement: REFERENCES is SQLite's keyword, with no fallback to a name.

use anyhow::Result;
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
";

/// The kinds a source gives to one subject alone (G1): what `references_one` holds unique.
const ONE_SUBJECT: [&str; 4] = ["release", "album", "artist", "label"];

/// A subject's id at a source: `kind` is `release-group`, `release`, `master`, `album`, `artist` or `label`, `value` the source's
/// id (a MusicBrainz id, a Discogs number, a page as `bandcamp::page` makes it).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ref {
    pub source: String,
    pub kind: String,
    pub value: String,
}

impl Ref {
    pub fn new(source: &str, kind: &str, value: &str) -> Self {
        Self { source: source.to_string(), kind: kind.to_string(), value: value.to_string() }
    }
}

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

/// What each album's files say its record is at MusicBrainz, kept as `files` references in the record pass's transaction
/// (`records[i]` is `albums[i]`'s): the release and the release group as tagged. Those kept already are left as they are, so a
/// pass over a library as it was writes nothing; one a search found becomes the files'. In the records' order: where one release's files make two records (G1, a
/// bonus track, another master), the older record keeps the release and the newer only the group, until a certain match or a
/// link pasted says which it is.
pub(super) fn files_references(tx: &Connection, albums: &[super::AlbumCopy], records: &[Option<i64>], at: i64) -> Result<()> {
    let mut named: Vec<(i64, &str, &str)> = Vec::new();
    for (a, record) in albums.iter().zip(records) {
        let Some(record) = *record else { continue };
        named.extend(a.facts.release.as_deref().map(|v| (record, "release", v)));
        named.extend(a.group.as_deref().map(|v| (record, "release-group", v)));
    }
    named.retain(|(_, _, v)| is_mbid(v));
    named.sort_unstable();
    named.dedup();
    let known: std::collections::HashSet<(String, String, String)> = {
        let mut st = tx.prepare_cached("SELECT subject, kind, value FROM \"references\" WHERE source = 'musicbrainz' AND kind IN ('release', 'release-group') AND how IN ('files', 'hand')")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?
    };
    for (record, kind, value) in named {
        let subject = ids::record(record);
        if !known.contains(&(subject.clone(), kind.to_string(), value.to_string())) {
            put_reference(tx, &subject, &Ref::new("musicbrainz", kind, value), "files", at, false)?;
        }
    }
    Ok(())
}

/// `set_reference` in the caller's transaction, found at `at`. `takes`: one another subject holds alone comes here, as by hand.
pub(super) fn put_reference(tx: &Connection, subject: &str, r: &Ref, how: &str, at: i64, takes: bool) -> Result<bool> {
    if ONE_SUBJECT.contains(&r.kind.as_str()) && subjects_in(tx, r)?.iter().any(|s| s != subject) {
        if !takes {
            return Ok(false);
        }
        tx.prepare_cached("DELETE FROM \"references\" WHERE source = ?1 AND kind = ?2 AND value = ?3 AND subject <> ?4")?.execute(params![r.source, r.kind, r.value, subject])?;
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
    Ok(true)
}

fn subjects_in(db: &Connection, r: &Ref) -> Result<Vec<String>> {
    let mut st = db.prepare_cached("SELECT subject FROM \"references\" WHERE source = ?1 AND kind = ?2 AND value = ?3 ORDER BY subject")?;
    let rows = st.query_map(params![r.source, r.kind, r.value], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

impl Catalogue {
    /// A record by its id for life, as it stands: had or not, retired or not. None: no such id. Blocking.
    pub fn record(&self, id: i64) -> Result<Option<RecordRow>> {
        match self.reader() {
            Some(c) => record_in(&c, id),
            None => record_in(&self.db(), id),
        }
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
        let kept = put_reference(&tx, subject, r, how, now(), how == "hand")?;
        tx.commit()?;
        Ok(kept)
    }

    /// `set_reference`, taking a reference another subject holds alone whatever the way: a certain match's release, as
    /// MusicBrainz matched the record's own tracks to it (where one release's files make two records, G1). Blocking.
    pub fn take_reference(&self, subject: &str, r: &Ref, how: &str) -> Result<bool> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let kept = put_reference(&tx, subject, r, how, now(), true)?;
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
    /// by hand, and no release its files name nor a link pasted. A certain match counts (0.17's step, `work_out` after it), so
    /// what 0.16 left off the item stays off. Blocking.
    pub fn unreferenced(&self) -> Result<Vec<i64>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT r.id FROM records r
             WHERE r.retired_at IS NULL AND r.merged_into IS NULL AND r.counting IS NULL AND r.checked_by_hand = 0
               AND EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id AND t.retired_at IS NULL)
               AND NOT EXISTS (SELECT 1 FROM \"references\" f WHERE f.subject = 'al-' || r.id
                 AND (f.how = 'hand' OR (f.how = 'files' AND f.source = 'musicbrainz' AND f.kind = 'release')))
             ORDER BY r.id",
        )?;
        let rows = st.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The subjects a reference names: one, for a release, an album page, an artist or a label; for a release group or a master,
    /// every record of it (G1). None: no subject known by it. Blocking.
    #[cfg_attr(not(test), allow(dead_code))] // a record found by its reference: C50's arrivals, C53a's missing list
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

/// The mark 0.17's step leaves in `meta`, its report in it: there, the step is done.
const CARRIED: &str = "references_017";

/// What 0.17's step carries, as the user data kept it (`app::step_017` reads it and makes these of it).
#[derive(Debug, Clone, Default)]
pub struct Carry {
    pub chosen: Vec<CarriedChoice>,
    pub matches: Vec<CarriedMatch>,
    pub discogs: Vec<CarriedDiscogs>,
    pub pages: Vec<CarriedPage>,
    pub links: Vec<CarriedLinks>,
    /// Each table's rows as read: the report's before and after alike, as nothing is taken out of it.
    pub read: Vec<(&'static str, u64)>,
    /// Rows whose JSON could not be read, by table: their keys, in the report as found by nothing.
    pub unread: Vec<(&'static str, Vec<String>)>,
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

/// Matching's find (`album_matches`): its release and release group, certain or likely, how many tracks it matched, and when.
#[derive(Debug, Clone)]
pub struct CarriedMatch {
    pub album: String,
    pub at: i64,
    pub release: String,
    pub group: Option<String>,
    pub certain: bool,
    pub tracks: usize,
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

/// MusicBrainz's verified Discogs links of a release (`mb_releases`), and when it was read.
#[derive(Debug, Clone)]
pub struct CarriedLinks {
    pub release: String,
    pub at: i64,
    pub discogs: Vec<Ref>,
}

/// What each record's files name at MusicBrainz, as their tags say: (record, `release` or `release-group`, id), the id most
/// of its files present name, the records in order. A tag that is no MusicBrainz id names nothing.
fn files_named(tx: &Connection) -> Result<Vec<(i64, &'static str, String)>> {
    let mut st = tx.prepare(
        "SELECT t.record, g.name, lower(g.value) FROM tags g
         JOIN files f ON f.folder = g.folder AND f.rel = g.rel
         JOIN tracks t ON t.id = f.track
         WHERE g.name IN ('musicbrainz_albumid', 'musicbrainz_releasegroupid') AND f.folder >= 0 AND f.gone_at IS NULL AND f.trashed IS NULL
           AND t.record IS NOT NULL AND t.retired_at IS NULL
         GROUP BY t.record, g.name, lower(g.value)
         ORDER BY t.record, g.name, count(*) DESC, lower(g.value)",
    )?;
    let rows: Vec<(i64, String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (record, name, value) in rows {
        let kind = if name == "musicbrainz_albumid" { "release" } else { "release-group" };
        if is_mbid(&value) && seen.insert((record, kind)) {
            out.push((record, kind, value));
        }
    }
    Ok(out)
}

/// The step's writes in its transaction, and what the report says of them.
struct Carrying<'t> {
    tx: &'t Connection,
    /// References another record holds alone, not taken: "<subject> <source>:<kind>:<value>".
    refused: Vec<String>,
    /// What found no record, by where it was kept.
    unmatched: std::collections::BTreeMap<&'static str, Vec<String>>,
}

impl Carrying<'_> {
    /// The record an album id names: one of the catalogue's, retired or not. None, counted under `table`.
    fn record(&mut self, table: &'static str, album: &str) -> Result<Option<i64>> {
        let found = match ids::record_number(album) {
            Some(id) => self.tx.prepare_cached("SELECT id FROM records WHERE id = ?1")?.query_row([id], |r| r.get(0)).optional()?,
            None => None,
        };
        if found.is_none() {
            self.unmatched.entry(table).or_default().push(album.to_string());
        }
        Ok(found)
    }

    fn put(&mut self, record: i64, r: &Ref, how: &str, at: i64, takes: bool) -> Result<bool> {
        let subject = ids::record(record);
        let kept = put_reference(self.tx, &subject, r, how, at, takes)?;
        if !kept {
            self.refused.push(format!("{subject} {}:{}:{}", r.source, r.kind, r.value));
        }
        Ok(kept)
    }

    /// The records a reference names, by their numbers.
    fn holders(&self, r: &Ref) -> Result<Vec<i64>> {
        Ok(subjects_in(self.tx, r)?.iter().filter_map(|s| ids::record_number(s)).collect())
    }
}

impl Catalogue {
    /// 0.17's step's report, where it is done: its mark. Blocking.
    pub fn references_carried(&self) -> Result<Option<serde_json::Value>> {
        Ok(self.meta(CARRIED).map(|v| serde_json::from_str(&v)).transpose()?)
    }

    /// 0.17's step on the catalogue's side: `c` into the references, in one transaction whose last write is its mark, holding
    /// the report returned. Who keeps a release two records' files name goes by the order of the parts: a link pasted by hand,
    /// a certain match, the files' own (the older record first), a likely match. `after` hears each part done, inside the
    /// transaction: an error from it leaves nothing written. Blocking.
    pub fn carry_references(&self, c: &Carry, after: &dyn Fn(&str) -> Result<()>) -> Result<serde_json::Value> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let now = now();
        let count = |tx: &Connection| -> Result<u64> { Ok(tx.query_row("SELECT count(*) FROM \"references\"", [], |r| r.get::<_, i64>(0))? as u64) };
        let before = count(&tx)?;
        let files = files_named(&tx)?;
        let named: std::collections::HashSet<(i64, &str, &str)> = files.iter().map(|(r, k, v)| (*r, *k, v.as_str())).collect();
        let mut w = Carrying { tx: &tx, refused: Vec::new(), unmatched: std::collections::BTreeMap::new() };
        let mut moved: std::collections::HashMap<&str, u64> = std::collections::HashMap::new();

        // 1. A link pasted by hand names what it names, whoever held it.
        let mut chosen: Vec<(i64, &CarriedChoice)> = Vec::new();
        for choice in &c.chosen {
            let Some(record) = w.record("record_references", &choice.album)? else { continue };
            for link in &choice.links {
                w.put(record, link, "hand", now, true)?;
            }
            w.unmatched.entry("record_references: links").or_default().extend(choice.unlinked.iter().map(|url| format!("{} {url}", choice.album)));
            chosen.push((record, choice));
        }
        *moved.entry("record_references").or_default() = chosen.len() as u64;
        after("hand")?;

        // 2. A certain match: MusicBrainz matched the record's own tracks to the release, which it takes. The files' word where
        //    they name it too.
        let mut matched: Vec<(i64, &CarriedMatch)> = Vec::new();
        for m in &c.matches {
            if let Some(record) = w.record("album_matches", &m.album)? {
                matched.push((record, m));
            }
        }
        *moved.entry("album_matches").or_default() = matched.len() as u64;
        let match_refs = |w: &mut Carrying, record: i64, m: &CarriedMatch| -> Result<()> {
            let how = |kind: &str, value: &str| if named.contains(&(record, kind, value)) { "files" } else { "search" };
            w.put(record, &Ref::new("musicbrainz", "release", &m.release), how("release", &m.release), m.at, m.certain)?;
            if let Some(group) = &m.group {
                w.put(record, &Ref::new("musicbrainz", "release-group", group), how("release-group", group), m.at, false)?;
            }
            Ok(())
        };
        for (record, m) in matched.iter().filter(|(_, m)| m.certain) {
            match_refs(&mut w, *record, m)?;
        }
        after("certain")?;

        // 3. What the files name, the older record first.
        let mut by_files = 0;
        for (record, kind, value) in &files {
            by_files += u64::from(w.put(*record, &Ref::new("musicbrainz", kind, value), "files", now, false)?);
        }
        after("files")?;

        // 4. A likely match, where no one else holds its release.
        for (record, m) in matched.iter().filter(|(_, m)| !m.certain) {
            match_refs(&mut w, *record, m)?;
        }
        after("likely")?;

        // 5. Discogs' facts: its master and main release, found by the record's name or MusicBrainz's link (`search`, which
        //    the links below make surer).
        for d in &c.discogs {
            let Some(record) = w.record("discogs_facts: albums", &d.album)? else { continue };
            for (kind, id) in [("master", d.master), ("release", d.release)] {
                if let Some(id) = id {
                    w.put(record, &Ref::new("discogs", kind, &id.to_string()), "search", d.at, false)?;
                }
            }
            *moved.entry("discogs_facts: albums").or_default() += 1;
        }
        after("discogs")?;

        // 6. MusicBrainz's links: the Discogs master and release of the records its release is, as the parts above left them.
        for l in &c.links {
            let holders = w.holders(&Ref::new("musicbrainz", "release", &l.release))?;
            if holders.is_empty() {
                w.unmatched.entry("mb_releases").or_default().push(l.release.clone());
                continue;
            }
            for record in holders {
                for r in &l.discogs {
                    w.put(record, r, "link", l.at, false)?;
                }
            }
            *moved.entry("mb_releases").or_default() += 1;
        }
        after("links")?;

        // 7. Bandcamp's pages: where exactly one record is what a page was kept by, its release group or its names.
        let mut names: std::collections::HashMap<String, Vec<i64>> = std::collections::HashMap::new();
        {
            let mut st = tx.prepare("SELECT id, COALESCE(artist, ''), COALESCE(title, '') FROM records WHERE merged_into IS NULL ORDER BY id")?;
            for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
                let (id, artist, title) = row?;
                names.entry(format!("{}|{}", cratarr_rules::names::key(&artist), cratarr_rules::names::key(&title))).or_default().push(id);
            }
        }
        for p in &c.pages {
            let records = if is_mbid(&p.key) { w.holders(&Ref::new("musicbrainz", "release-group", &p.key))? } else { names.get(&p.key).cloned().unwrap_or_default() };
            match records[..] {
                [record] => {
                    w.put(record, &Ref::new("bandcamp", "album", &p.page), "search", p.at, false)?;
                    *moved.entry("bandcamp_found").or_default() += 1;
                }
                [] => w.unmatched.entry("bandcamp_found").or_default().push(p.key.clone()),
                _ => w.unmatched.entry("bandcamp_found: several records").or_default().push(p.key.clone()),
            }
        }
        after("pages")?;

        // 8. 0.14's choices: the source chosen, by hand; checked by hand; its guesses confirmed.
        let (mut by_hand, mut checked, mut confirmed) = (0, 0, 0);
        for (record, choice) in &chosen {
            if let Some(source) = &choice.counting {
                by_hand += tx.execute("UPDATE records SET counting = ?2, counted_by_hand = 1 WHERE id = ?1", params![record, source])? as u64;
            }
            if choice.checked {
                checked += tx.execute("UPDATE records SET checked_by_hand = 1 WHERE id = ?1", [record])? as u64;
            }
            for source in &choice.confirmed {
                confirmed += tx.execute("UPDATE \"references\" SET confirmed = 1 WHERE subject = ?1 AND source = ?2", params![ids::record(*record), source])? as u64;
            }
        }
        after("chosen")?;

        // 9. A certain match counts where nothing was chosen: its tracks were held against the release's, which is what
        //    `identity::agrees` does once the list is read (`references::work_out`, when the record is opened).
        let mut by_match = 0;
        for (record, m) in matched.iter().filter(|(_, m)| m.certain) {
            let holds = w.holders(&Ref::new("musicbrainz", "release", &m.release))?.contains(record);
            let tracks: i64 = tx.query_row("SELECT count(*) FROM tracks WHERE record = ?1 AND retired_at IS NULL", [record], |r| r.get(0))?;
            if holds && tracks as usize == m.tracks {
                by_match += tx.execute("UPDATE records SET counting = 'musicbrainz' WHERE id = ?1 AND counting IS NULL AND counted_by_hand = 0", [record])? as u64;
            }
        }
        after("counting")?;

        let mut report = crate::moved::Report::default();
        for (table, rows) in &c.read {
            report.table(table, *rows, *rows, moved.get(table).copied().unwrap_or(0));
        }
        report.table("files: musicbrainz", files.len() as u64, files.len() as u64, by_files);
        let after_count = count(&tx)?;
        report.table("references", before, after_count, after_count - before);
        let (refused, mut unmatched) = (std::mem::take(&mut w.refused), std::mem::take(&mut w.unmatched));
        for (table, keys) in &c.unread {
            unmatched.entry(table).or_default().extend(keys.iter().map(|k| format!("{k} (not read)")));
        }
        for what in ["record_references", "record_references: links", "album_matches", "discogs_facts: albums", "bandcamp_found", "bandcamp_found: several records", "mb_releases"] {
            report.unmatched(what, unmatched.remove(what).unwrap_or_default());
        }
        report.differed("references: held by another record", refused);
        let mut report = serde_json::to_value(&report)?;
        if let Some(fields) = report.as_object_mut() {
            for (field, n) in [("references", after_count), ("counted_by_hand", by_hand), ("counted_by_a_match", by_match), ("checked_by_hand", checked), ("confirmed", confirmed)] {
                fields.insert(field.into(), n.into());
            }
        }
        tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![CARRIED, report.to_string()])?;
        tx.commit()?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
