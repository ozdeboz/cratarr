//! The library's words as each source said them (0.9.7, `app::words`): by subject (`album:<its id>`,
//! `artist:<its id>`), source (musicbrainz, discogs, lastfm, files) and word, with what the source called it (genre,
//! style, mood, tag) and its own strength (0 to 1); and when each source was last asked about each subject, so the
//! harvest's passes ask again only when due. Household-wide, as the album tags are.

use anyhow::Result;
use rusqlite::params;

use super::UserData;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS words (
  subject TEXT NOT NULL, source TEXT NOT NULL, word TEXT NOT NULL, kind TEXT NOT NULL, weight REAL NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (subject, source, word)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS words_asked (subject TEXT NOT NULL, source TEXT NOT NULL, at INTEGER NOT NULL, ok INTEGER NOT NULL,
  PRIMARY KEY (subject, source)) WITHOUT ROWID;
-- An album's match with a MusicBrainz release (0.9.7, app::matching; JSON, none where none was found), and when.
CREATE TABLE IF NOT EXISTS album_matches (album TEXT PRIMARY KEY, at INTEGER NOT NULL, found TEXT) WITHOUT ROWID;
-- An album held against its release group's releases (0.9.7, app::incomplete): what it misses (JSON; none: whole, or
-- no release holds it), and when. Added later (0.13): the discs its tracks are on in the release that holds it
-- (`discs`; none: no release holds it), and the rule it was looked at by (`rule`, `COMPLETE_RULE`).
CREATE TABLE IF NOT EXISTS album_complete (album TEXT PRIMARY KEY, at INTEGER NOT NULL, found TEXT) WITHOUT ROWID;
-- What Discogs says of a subject beyond its words (0.9.8, app::discogs_facts): an album's (`album:<its id>`),
-- an artist's (`artist:<name key>`), a release group's (`rg:<id>`); JSON, none where Discogs has nothing; and when.
CREATE TABLE IF NOT EXISTS discogs_facts (subject TEXT PRIMARY KEY, at INTEGER NOT NULL, facts TEXT) WITHOUT ROWID;
-- What hearing a record waiting in Incoming found (0.14, app::intake_card): each track decoded whole and its recordings
-- as AcoustID knows them (JSON), and when.
CREATE TABLE IF NOT EXISTS intake_heard (album TEXT NOT NULL, folder INTEGER NOT NULL DEFAULT -1, at INTEGER NOT NULL, heard TEXT NOT NULL, PRIMARY KEY (album, folder)) WITHOUT ROWID;
-- What was chosen of a record's references (0.14, app::references): the source that counts, links pasted, checked by
-- hand (JSON).
CREATE TABLE IF NOT EXISTS record_references (album TEXT PRIMARY KEY, chosen TEXT NOT NULL) WITHOUT ROWID;
-- A track's own words set by hand (0.15, decision 0097's track Edit): by its id and kind (genre, style, mood, tag), the
-- words (JSON), and when; none: its record's.
CREATE TABLE IF NOT EXISTS track_words (track TEXT NOT NULL, kind TEXT NOT NULL, words TEXT NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (track, kind)) WITHOUT ROWID;
";

/// A word as a source said it.
#[derive(Debug, Clone, PartialEq)]
pub struct RawWord {
    pub word: String,
    /// genre, style, mood or tag, as the source has it.
    pub kind: String,
    pub weight: f64,
}

/// The rule an album is held against its releases by (0.13: 1, the discs its tracks are on kept too).
pub const COMPLETE_RULE: i64 = 1;

/// The albums held against their releases before the discs were kept get the columns (rule 0: looked at again).
/// Safe to repeat.
pub(super) fn add_album_complete_discs(conn: &rusqlite::Connection) -> Result<()> {
    let has: bool = conn.prepare("SELECT 1 FROM pragma_table_info('album_complete') WHERE name = 'discs'")?.exists([])?;
    if !has {
        conn.execute_batch("ALTER TABLE album_complete ADD COLUMN discs INTEGER; ALTER TABLE album_complete ADD COLUMN rule INTEGER NOT NULL DEFAULT 0;")?;
    }
    Ok(())
}

/// What was heard before a record had copies: by album alone, the key a table cannot change in place. It goes for every copy
/// of its record. Safe to repeat.
pub(super) fn add_heard_folder(conn: &rusqlite::Connection) -> Result<()> {
    let has: bool = conn.prepare("SELECT 1 FROM pragma_table_info('intake_heard') WHERE name = 'folder'")?.exists([])?;
    if !has {
        conn.execute_batch(
            "BEGIN;
             ALTER TABLE intake_heard RENAME TO intake_heard_by_album;
             CREATE TABLE intake_heard (album TEXT NOT NULL, folder INTEGER NOT NULL DEFAULT -1, at INTEGER NOT NULL, heard TEXT NOT NULL, PRIMARY KEY (album, folder)) WITHOUT ROWID;
             INSERT INTO intake_heard (album, at, heard) SELECT album, at, heard FROM intake_heard_by_album;
             DROP TABLE intake_heard_by_album;
             COMMIT;",
        )?;
    }
    Ok(())
}

impl UserData {
    /// What Discogs said of a subject (`app::discogs_facts`), and when; none where it was never asked.
    pub fn discogs_facts(&self, subject: &str) -> Result<Option<(i64, Option<String>)>> {
        use rusqlite::OptionalExtension;
        Ok(self.db().query_row("SELECT at, facts FROM discogs_facts WHERE subject = ?1", [subject], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    /// What Discogs said of every subject of one kind (`prefix`: "album:"), where it said something (0.11, quick crates:
    /// the albums' labels for the crates).
    pub fn discogs_facts_of(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT subject, facts FROM discogs_facts WHERE substr(subject, 1, ?2) = ?1 AND facts IS NOT NULL")?;
        let rows = st.query_map(params![prefix, prefix.chars().count() as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// How many albums Discogs said something of, cheaply: one more may bring the crates a label. Not when: a round
    /// asks old albums again all along, which changes nothing, and reading every file for that is what the stamp is for.
    pub fn discogs_albums_stamp(&self) -> Result<i64> {
        Ok(self.db().query_row("SELECT count(*) FROM discogs_facts WHERE subject >= 'album:' AND subject < 'album;' AND facts IS NOT NULL", [], |r| r.get(0))?)
    }

    pub fn set_discogs_facts(&self, subject: &str, facts: Option<&str>, at: i64) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO discogs_facts (subject, at, facts) VALUES (?1, ?2, ?3)", params![subject, at, facts])?;
        Ok(())
    }

    /// What a source says of a subject now, in place of what it said before; noted as asked (`ok`: it answered).
    pub fn set_words(&self, subject: &str, source: &str, words: &[RawWord], ok: bool, at: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        if ok {
            tx.execute("DELETE FROM words WHERE subject = ?1 AND source = ?2", params![subject, source])?;
            let mut ins = tx.prepare_cached("INSERT OR REPLACE INTO words (subject, source, word, kind, weight, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
            for w in words {
                ins.execute(params![subject, source, w.word, w.kind, w.weight, at])?;
            }
        }
        tx.execute("INSERT OR REPLACE INTO words_asked (subject, source, at, ok) VALUES (?1, ?2, ?3, ?4)", params![subject, source, at, ok])?;
        tx.commit()?;
        Ok(())
    }

    /// The subjects a source was asked about since `since` (answered or not): not asked again yet.
    pub fn words_asked(&self, source: &str, since: i64) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT subject FROM words_asked WHERE source = ?1 AND at >= ?2")?;
        let rows = st.query_map(params![source, since], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// When each subject was last asked of a source: the oldest asked first when every one is due.
    pub fn words_asked_at(&self, source: &str) -> Result<std::collections::HashMap<String, i64>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT subject, at FROM words_asked WHERE source = ?1")?;
        let rows = st.query_map([source], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every word kept: (subject, source, word, kind, weight).
    pub fn all_words(&self) -> Result<Vec<(String, String, String, String, f64)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT subject, source, word, kind, weight FROM words")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// These albums' matches as kept: when each was looked for, and what was found (JSON; none: nothing).
    pub fn album_matches(&self, albums: &[String]) -> Result<std::collections::HashMap<String, (i64, Option<String>)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT at, found FROM album_matches WHERE album = ?1")?;
        let mut out = std::collections::HashMap::new();
        for a in albums {
            if let Some(row) = st.query_map([a], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)))?.next() {
                out.insert(a.clone(), row?);
            }
        }
        Ok(out)
    }

    /// What was chosen of a record's references (`app::references`; JSON).
    pub fn record_references(&self, album: &str) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        Ok(self.db().query_row("SELECT chosen FROM record_references WHERE album = ?1", [album], |r| r.get(0)).optional()?)
    }

    pub fn set_record_references(&self, album: &str, chosen: &str) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO record_references (album, chosen) VALUES (?1, ?2)", params![album, chosen])?;
        Ok(())
    }

    /// Every record's chosen references, by album.
    pub fn all_record_references(&self) -> Result<std::collections::HashMap<String, String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT album, chosen FROM record_references")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What hearing a record's copy in catalogue folder `folder` found (`app::intake_card`), and when; with none named, the
    /// last heard of the record. One from before copies goes for any.
    pub fn intake_heard(&self, album: &str, folder: Option<i64>) -> Result<Option<(i64, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached(&format!("SELECT at, heard FROM intake_heard WHERE album = ?1 AND (?2 IS NULL OR folder = ?2 OR folder = {}) ORDER BY at DESC LIMIT 1", super::NO_FOLDER))?;
        Ok(st.query_map(params![album, folder], |r| Ok((r.get(0)?, r.get(1)?)))?.next().transpose()?)
    }

    pub fn set_intake_heard(&self, album: &str, folder: Option<i64>, at: i64, heard: &str) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO intake_heard (album, folder, at, heard) VALUES (?1, ?2, ?3, ?4)", params![album, folder.unwrap_or(super::NO_FOLDER), at, heard])?;
        Ok(())
    }

    pub fn set_album_match(&self, album: &str, found: Option<&str>, at: i64) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO album_matches (album, at, found) VALUES (?1, ?2, ?3)", params![album, at, found])?;
        Ok(())
    }

    /// When each album was last held against its releases (`app::incomplete`).
    pub fn album_complete_at(&self) -> Result<std::collections::HashMap<String, i64>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT album, at FROM album_complete")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The albums found missing tracks: (album, what it misses as JSON).
    pub fn albums_incomplete(&self) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT album, found FROM album_complete WHERE found IS NOT NULL")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_album_complete(&self, album: &str, found: Option<&str>, discs: Option<u32>, at: i64) -> Result<()> {
        self.db().execute(
            "INSERT OR REPLACE INTO album_complete (album, at, found, discs, rule) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![album, at, found, discs, COMPLETE_RULE],
        )?;
        Ok(())
    }

    /// When each album was last held against its releases by this rule (`COMPLETE_RULE`); one looked at by an older
    /// rule is left out, so it is looked at again first.
    pub fn album_complete_now(&self) -> Result<std::collections::HashMap<String, i64>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT album, at FROM album_complete WHERE rule >= ?1")?;
        let rows = st.query_map([COMPLETE_RULE], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The discs each album's tracks are on in the release that holds it (0.13), by album.
    pub fn album_discs(&self) -> Result<std::collections::HashMap<String, u32>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT album, discs FROM album_complete WHERE discs IS NOT NULL")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.clamp(0, i64::from(u32::MAX)) as u32)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A cheap summary of the words and the album tags as kept: it changes when either does.
    pub fn words_stamp(&self) -> Result<i64> {
        let db = self.db();
        let (n, at): (i64, i64) = db.query_row("SELECT count(*), coalesce(max(at), 0) FROM words", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let (tags, tagged): (i64, i64) = db.query_row("SELECT count(*), coalesce(max(at), 0) FROM album_tags", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        // A track's own words, and the Catalogue sheet's choices (sources switched off, aliases: `words:` choices).
        let (own, owned): (i64, i64) = db.query_row("SELECT count(*), coalesce(max(at), 0) FROM track_words", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let choices: String = db.query_row("SELECT coalesce(group_concat(source || '=' || choice, ';'), '') FROM import_choices WHERE source LIKE 'words:%'", [], |r| r.get(0))?;
        let chosen = choices.bytes().fold(0i64, |h, b| h.wrapping_mul(131).wrapping_add(i64::from(b)));
        Ok(n.wrapping_mul(31)
            .wrapping_add(at)
            .wrapping_mul(31)
            .wrapping_add(tags)
            .wrapping_mul(31)
            .wrapping_add(tagged)
            .wrapping_mul(31)
            .wrapping_add(own)
            .wrapping_mul(31)
            .wrapping_add(owned)
            .wrapping_mul(31)
            .wrapping_add(chosen))
    }

    /// Every track's own words set by hand: (track, kind, words).
    pub fn all_track_words(&self) -> Result<Vec<(String, String, Vec<String>)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT track, kind, words FROM track_words")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (t, k, w) = row?;
            out.push((t, k, serde_json::from_str(&w).unwrap_or_default()));
        }
        Ok(out)
    }

    /// A track's own words of a kind set by hand; none: back to its record's.
    pub fn set_track_words(&self, track: &str, kind: &str, words: Option<&[String]>) -> Result<()> {
        let db = self.db();
        match words {
            Some(w) => db.execute(
                "INSERT INTO track_words (track, kind, words, at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (track, kind) DO UPDATE SET words = excluded.words, at = excluded.at",
                params![track, kind, serde_json::to_string(w)?, super::now_ms()],
            )?,
            None => db.execute("DELETE FROM track_words WHERE track = ?1 AND kind = ?2", params![track, kind])?,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_album_more_moves_the_discogs_stamp_and_a_round_asking_old_ones_again_does_not() {
        let u = UserData::open(None).unwrap();
        u.set_discogs_facts("album:a", Some("{}"), 1).unwrap();
        let said = u.discogs_albums_stamp().unwrap();
        assert_eq!(said, 1);
        u.set_discogs_facts("album:a", Some("{}"), 2).unwrap();
        u.set_discogs_facts("album:unknown there", None, 3).unwrap();
        u.set_discogs_facts("artist:x", Some("{}"), 3).unwrap();
        assert_eq!(u.discogs_albums_stamp().unwrap(), said, "asked again, nothing said, and no album's");
        u.set_discogs_facts("album:b", Some("{}"), 4).unwrap();
        assert_eq!(u.discogs_albums_stamp().unwrap(), 2);
    }

    #[test]
    fn a_source_s_words_replace_its_own_and_an_unanswered_ask_keeps_them() {
        let u = UserData::open(None).unwrap();
        let w = |word: &str| RawWord { word: word.into(), kind: "tag".into(), weight: 0.5 };
        u.set_words("album:al-1", "lastfm", &[w("chill"), w("dub")], true, 10).unwrap();
        u.set_words("album:al-1", "discogs", &[w("Dub")], true, 10).unwrap();
        u.set_words("album:al-1", "lastfm", &[w("dub")], true, 20).unwrap();
        let mut all: Vec<(String, String)> = u.all_words().unwrap().into_iter().map(|(_, s, w, _, _)| (s, w)).collect();
        all.sort();
        assert_eq!(all, [("discogs".to_string(), "Dub".to_string()), ("lastfm".into(), "dub".into())]);
        let before = u.words_stamp().unwrap();
        u.set_words("album:al-1", "lastfm", &[], false, 30).unwrap();
        assert_eq!(u.all_words().unwrap().len(), 2, "no answer: what it said stands");
        assert!(u.words_asked("lastfm", 25).unwrap().contains("album:al-1"), "but it was asked");
        assert_eq!(u.words_stamp().unwrap(), before);
    }
}
