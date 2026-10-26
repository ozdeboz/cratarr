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
