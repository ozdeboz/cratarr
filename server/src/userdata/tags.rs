//! Album tags (decision 0030): household-wide labels on albums ("gig", "summer"), as Jellyfin's album tags were, kept
//! in the user data database. An album's tags are kept under its id as it is (or would be) in Music, so they follow an
//! album kept from Incoming. Tags are stored lower-case and trimmed, so they compare without regard to case. Every
//! change is one statement, so two at once cannot lose each other's work.

use anyhow::Result;
use rusqlite::params;

use super::UserData;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS album_tags (album TEXT NOT NULL, tag TEXT NOT NULL, at INTEGER NOT NULL, PRIMARY KEY (album, tag)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS album_tags_by_tag ON album_tags (tag, album);
";

/// A tag as it is kept: trimmed, lower case; `None` when nothing is left.
pub fn clean(tag: &str) -> Option<String> {
    let t = tag.trim().to_lowercase();
    (!t.is_empty()).then_some(t)
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl UserData {
    /// An album's tags, in the order they were added.
    pub fn tags_of(&self, album: &str) -> Result<Vec<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT tag FROM album_tags WHERE album = ?1 ORDER BY at, tag")?;
        let rows = st.query_map([album], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every album's tags: (album, tag), the library's words read them (`app::words`).
    pub fn all_album_tags(&self) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT album, tag FROM album_tags")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Adds a tag; `false` when the album had it already.
    pub fn add_tag(&self, album: &str, tag: &str) -> Result<bool> {
        let Some(tag) = clean(tag) else { return Ok(false) };
        let n = self.db().execute("INSERT OR IGNORE INTO album_tags (album, tag, at) VALUES (?1, ?2, ?3)", params![album, tag, now()])?;
        Ok(n > 0)
    }

    /// Removes a tag; `false` when the album did not have it.
    pub fn remove_tag(&self, album: &str, tag: &str) -> Result<bool> {
        let Some(tag) = clean(tag) else { return Ok(false) };
        Ok(self.db().execute("DELETE FROM album_tags WHERE album = ?1 AND tag = ?2", params![album, tag])? > 0)
    }

    /// Removes every tag of an album and says which there were.
    pub fn clear_tags(&self, album: &str) -> Result<Vec<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("DELETE FROM album_tags WHERE album = ?1 RETURNING tag")?;
        let rows = st.query_map([album], |r| r.get::<_, String>(0))?;
        let mut removed: Vec<String> = rows.collect::<rusqlite::Result<_>>()?;
        removed.sort();
        Ok(removed)
    }

    /// Every tag in use, with how many albums carry it.
    pub fn all_tags(&self) -> Result<Vec<(String, u32)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT tag, count(*) FROM album_tags GROUP BY tag ORDER BY tag")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as u32)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The albums (their Music-form ids) carrying a tag, most recently tagged first.
    pub fn albums_tagged(&self, tag: &str) -> Result<Vec<String>> {
        let Some(tag) = clean(tag) else { return Ok(Vec::new()) };
        let db = self.db();
        let mut st = db.prepare_cached("SELECT album FROM album_tags WHERE tag = ?1 ORDER BY at DESC, album")?;
        let rows = st.query_map([tag], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_kept_clean_and_once() {
        let u = UserData::open(None).unwrap();
        assert!(u.add_tag("al-x", "  Gig ").unwrap());
        assert!(!u.add_tag("al-x", "gig").unwrap(), "already there, whatever the case");
        assert!(u.add_tag("al-x", "Summer").unwrap());
        assert!(!u.add_tag("al-x", "   ").unwrap(), "nothing to add");
        assert_eq!(u.tags_of("al-x").unwrap(), ["gig", "summer"]);
        assert_eq!(u.all_tags().unwrap(), [("gig".to_string(), 1), ("summer".to_string(), 1)]);
        assert_eq!(u.albums_tagged("GIG").unwrap(), ["al-x"]);
        assert!(u.remove_tag("al-x", "GIG").unwrap());
        assert!(!u.remove_tag("al-x", "gig").unwrap());
        assert_eq!(u.clear_tags("al-x").unwrap(), ["summer"]);
        assert!(u.tags_of("al-x").unwrap().is_empty());
    }
}
