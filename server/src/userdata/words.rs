//! The library's words as each source said them (0.9.7, `app::words`): by subject (`album:<its Music-form id>`,
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
-- no release holds it), and when.
CREATE TABLE IF NOT EXISTS album_complete (album TEXT PRIMARY KEY, at INTEGER NOT NULL, found TEXT) WITHOUT ROWID;
-- What Discogs says of a subject beyond its words (0.9.8, app::discogs_facts): an album's (`album:<its Music-form id>`),
-- an artist's (`artist:<name key>`), a release group's (`rg:<id>`); JSON, none where Discogs has nothing; and when.
CREATE TABLE IF NOT EXISTS discogs_facts (subject TEXT PRIMARY KEY, at INTEGER NOT NULL, facts TEXT) WITHOUT ROWID;
";

/// A word as a source said it.
#[derive(Debug, Clone, PartialEq)]
pub struct RawWord {
    pub word: String,
    /// genre, style, mood or tag, as the source has it.
    pub kind: String,
    pub weight: f64,
}

impl UserData {
    /// What Discogs said of a subject (`app::discogs_facts`), and when; none where it was never asked.
    pub fn discogs_facts(&self, subject: &str) -> Result<Option<(i64, Option<String>)>> {
        use rusqlite::OptionalExtension;
        Ok(self.db().query_row("SELECT at, facts FROM discogs_facts WHERE subject = ?1", [subject], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
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

    pub fn set_album_complete(&self, album: &str, found: Option<&str>, at: i64) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO album_complete (album, at, found) VALUES (?1, ?2, ?3)", params![album, at, found])?;
        Ok(())
    }

    /// A cheap summary of the words and the album tags as kept: it changes when either does.
    pub fn words_stamp(&self) -> Result<i64> {
        let db = self.db();
        let (n, at): (i64, i64) = db.query_row("SELECT count(*), coalesce(max(at), 0) FROM words", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let (tags, tagged): (i64, i64) = db.query_row("SELECT count(*), coalesce(max(at), 0) FROM album_tags", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(n.wrapping_mul(31).wrapping_add(at).wrapping_mul(31).wrapping_add(tags).wrapping_mul(31).wrapping_add(tagged))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
