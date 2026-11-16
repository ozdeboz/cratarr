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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::ids;

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
