//! What the library's records are as music (0.10, the owner, 2026-10-30: Classical as a kind, and Other versions):
//! each MusicBrainz release as the works' pass read it (its genres, its Discogs links, its performers, its tracks'
//! works), each work's title and composers, which artists are another's performance name ("is person") and their
//! Discogs page, Discogs' real names and aliases and its genres by a verified link, the household's marks (classical or
//! not, per record) and the composers' keys as the page addresses them. Household-wide, as the release types are;
//! what the passes keep is asked again in time, the marks only change by hand.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use rusqlite::{OptionalExtension, params};

use super::UserData;

pub const SCHEMA: &str = "
-- A MusicBrainz release as the works' pass read it (0.10; JSON, app::works::Stored: its genres, its verified Discogs
-- links, its performers in order, whether someone is credited as performing, its tracks' works); `found` 0 where
-- MusicBrainz had no such release; when it was asked.
CREATE TABLE IF NOT EXISTS mb_releases (release TEXT PRIMARY KEY, found INTEGER NOT NULL, facts TEXT NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;
-- A MusicBrainz work (0.10): its title and its composers (JSON, app::works::Person), and when it was read.
CREATE TABLE IF NOT EXISTS mb_works (work TEXT PRIMARY KEY, title TEXT NOT NULL, composers TEXT NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;
-- The work a MusicBrainz work is a part of (0.12, decision 0086: a movement's symphony; MusicBrainz's \"parts\"): the
-- parent, its title, the part's number in it (the relation's ordering key; none where not given), and when it was read.
CREATE TABLE IF NOT EXISTS mb_work_parents (work TEXT PRIMARY KEY, parent TEXT NOT NULL, title TEXT NOT NULL, number INTEGER, at INTEGER NOT NULL) WITHOUT ROWID;
-- An artist by their MusicBrainz id (0.10): the person they are where they are a performance name (MusicBrainz's \"is
-- person\"; none: not one, or not known), that person's name, their Discogs artist (MusicBrainz's link; none: none),
-- and when the artist was asked (0: only seen from the person's side, not asked yet).
CREATE TABLE IF NOT EXISTS artist_person (artist TEXT PRIMARY KEY, person TEXT, name TEXT, discogs INTEGER, at INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID;
-- A Discogs artist's real name and aliases (0.10; aliases joined by U+001F), asked only by MusicBrainz's link, and when.
CREATE TABLE IF NOT EXISTS discogs_names (discogs INTEGER PRIMARY KEY, realname TEXT, aliases TEXT NOT NULL, at INTEGER NOT NULL);
-- Discogs' genres and styles of a release or master MusicBrainz links (0.10; `release:<id>`, `master:<id>`; joined by
-- U+001F, empty: none), and when.
CREATE TABLE IF NOT EXISTS discogs_link_genres (link TEXT PRIMARY KEY, genres TEXT NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;
-- A record marked by hand (0.10, the owner): `classical` or `not`, by its Music-form id (as its tags), library-wide; no
-- pass changes it.
CREATE TABLE IF NOT EXISTS classical_marks (album TEXT PRIMARY KEY, mark TEXT NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;
-- Every composer key the page was given (0.10): a MusicBrainz artist or `name:<loose name>`, the name shown, and the key
-- it is now (`alias_of`, once a name was found to be a MusicBrainz artist); kept, so an address keeps leading there.
CREATE TABLE IF NOT EXISTS composer_keys (key TEXT PRIMARY KEY, name TEXT NOT NULL, alias_of TEXT, at INTEGER NOT NULL) WITHOUT ROWID;
";

/// A work as kept: (title, composers as JSON).
pub type KeptWork = (String, String);
/// The work a work is a part of, as kept (0.12): (the parent, its title, the part's number in it).
pub type KeptParent = (String, String, Option<u32>);
/// A work as a release's answer gives it: (id, title, composers as JSON, its parent where MusicBrainz names one).
pub type ReadWork = (String, String, String, Option<KeptParent>);

impl UserData {
    /// The releases not to ask about again: those found and asked since `found_since`, those not found and asked since
    /// `none_since`.
    pub fn mb_releases_asked(&self, found_since: i64, none_since: i64) -> Result<HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT release FROM mb_releases WHERE at >= CASE WHEN found <> 0 THEN ?1 ELSE ?2 END")?;
        let rows = st.query_map(params![found_since, none_since], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What MusicBrainz answered for a release (`facts`: JSON; none: no such release), with its works (id, title,
    /// composers as JSON, and the work each is a part of: a parent no longer named is forgotten), as one write.
    pub fn set_mb_release(&self, release: &str, facts: Option<&str>, works: &[ReadWork], at: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO mb_releases (release, found, facts, at) VALUES (?1, ?2, ?3, ?4)",
            params![release, facts.is_some(), facts.unwrap_or("{}"), at],
        )?;
        {
            let mut st = tx.prepare_cached("INSERT OR REPLACE INTO mb_works (work, title, composers, at) VALUES (?1, ?2, ?3, ?4)")?;
            let mut parent = tx.prepare_cached("INSERT OR REPLACE INTO mb_work_parents (work, parent, title, number, at) VALUES (?1, ?2, ?3, ?4, ?5)")?;
            let mut orphan = tx.prepare_cached("DELETE FROM mb_work_parents WHERE work = ?1")?;
            for (work, title, composers, of) in works {
                st.execute(params![work, title, composers, at])?;
                match of {
                    Some((p, p_title, number)) => parent.execute(params![work, p, p_title, number, at])?,
                    None => orphan.execute([work])?,
                };
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The releases MusicBrainz knew that were kept before their works' parents were read (0.12): asked again first.
    pub fn mb_releases_without_parents(&self) -> Result<HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT release FROM mb_releases WHERE found <> 0 AND coalesce(json_extract(facts, '$.with_parents'), 0) = 0")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every work's parent as read (0.12), by the work's MusicBrainz id.
    pub fn mb_work_parents(&self) -> Result<HashMap<String, KeptParent>> {
        let db = self.db();
        let mut st = db.prepare("SELECT work, parent, title, number FROM mb_work_parents")?;
        let rows = st.query_map([], |r| {
            let number: Option<i64> = r.get(3)?;
            Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, r.get::<_, String>(2)?, number.and_then(|n| u32::try_from(n).ok()))))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every release MusicBrainz knew, with what was read of it (JSON).
    pub fn mb_releases(&self) -> Result<HashMap<String, String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT release, facts FROM mb_releases WHERE found <> 0")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every work read, by its MusicBrainz id.
    pub fn mb_works(&self) -> Result<HashMap<String, KeptWork>> {
        let db = self.db();
        let mut st = db.prepare("SELECT work, title, composers FROM mb_works")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, r.get::<_, String>(2)?))))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Performance names and the persons they are, as found (alias, person, the person's name): from either side, the
    /// alias's asking left as it was.
    pub fn add_persons(&self, found: &[(String, String, String)]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare_cached(
                "INSERT INTO artist_person (artist, person, name, at) VALUES (?1, ?2, ?3, 0)
                 ON CONFLICT (artist) DO UPDATE SET person = excluded.person, name = excluded.name",
            )?;
            for (alias, person, name) in found.iter().filter(|(a, p, _)| a != p) {
                st.execute(params![alias, person, name])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// An artist asked about: their Discogs artist where MusicBrainz links one.
    pub fn set_artist_asked(&self, artist: &str, discogs: Option<u64>, at: i64) -> Result<()> {
        self.db().execute(
            "INSERT INTO artist_person (artist, discogs, at) VALUES (?1, ?2, ?3)
             ON CONFLICT (artist) DO UPDATE SET discogs = excluded.discogs, at = excluded.at",
            params![artist, discogs.map(|d| d as i64), at],
        )?;
        Ok(())
    }

    /// The artists asked about since `since`.
    pub fn artists_asked(&self, since: i64) -> Result<HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT artist FROM artist_person WHERE at > 0 AND at >= ?1")?;
        let rows = st.query_map([since], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every performance name with the person they are: alias → (person, the person's name).
    pub fn persons(&self) -> Result<HashMap<String, (String, String)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT artist, person, coalesce(name, '') FROM artist_person WHERE person IS NOT NULL AND person <> artist")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, r.get::<_, String>(2)?))))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every artist's Discogs artist, by MusicBrainz's link.
    pub fn artist_discogs(&self) -> Result<HashMap<String, u64>> {
        let db = self.db();
        let mut st = db.prepare("SELECT artist, discogs FROM artist_person WHERE discogs IS NOT NULL")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as u64)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A Discogs artist's real name and aliases, as asked.
    pub fn set_discogs_names(&self, discogs: u64, realname: Option<&str>, aliases: &[String], at: i64) -> Result<()> {
        self.db().execute(
            "INSERT OR REPLACE INTO discogs_names (discogs, realname, aliases, at) VALUES (?1, ?2, ?3, ?4)",
            params![discogs as i64, realname, aliases.join("\u{1f}"), at],
        )?;
        Ok(())
    }

    /// Every Discogs artist's names: their real name (where Discogs gives one) and aliases.
    pub fn discogs_names(&self) -> Result<HashMap<u64, Vec<String>>> {
        let db = self.db();
        let mut st = db.prepare("SELECT discogs, realname, aliases FROM discogs_names")?;
        let rows = st.query_map([], |r| {
            let (id, real, aliases): (i64, Option<String>, String) = (r.get(0)?, r.get(1)?, r.get(2)?);
            let names = real.into_iter().chain(aliases.split('\u{1f}').map(str::to_string)).filter(|n| !n.trim().is_empty()).collect();
            Ok((id.max(0) as u64, names))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The Discogs artists asked about since `since`.
    pub fn discogs_names_asked(&self, since: i64) -> Result<HashSet<u64>> {
        let db = self.db();
        let mut st = db.prepare("SELECT discogs FROM discogs_names WHERE at >= ?1")?;
        let rows = st.query_map([since], |r| Ok(r.get::<_, i64>(0)?.max(0) as u64))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Discogs' genres and styles of a linked release or master (none found: empty).
    pub fn set_discogs_link_genres(&self, link: &str, genres: &[String], at: i64) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO discogs_link_genres (link, genres, at) VALUES (?1, ?2, ?3)", params![link, genres.join("\u{1f}"), at])?;
        Ok(())
    }

    /// Every linked release's or master's genres and styles.
    pub fn discogs_link_genres(&self) -> Result<HashMap<String, Vec<String>>> {
        let db = self.db();
        let mut st = db.prepare("SELECT link, genres FROM discogs_link_genres")?;
        let rows = st.query_map([], |r| {
            let genres: String = r.get(1)?;
            Ok((r.get::<_, String>(0)?, genres.split('\u{1f}').filter(|g| !g.is_empty()).map(str::to_string).collect()))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The links asked about since `since`.
    pub fn discogs_links_asked(&self, since: i64) -> Result<HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT link FROM discogs_link_genres WHERE at >= ?1")?;
        let rows = st.query_map([since], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A record's mark (`classical` or `not`), by its Music-form id; none takes it away.
    pub fn set_classical_mark(&self, album: &str, mark: Option<&str>, at: i64) -> Result<()> {
        match mark {
            Some(m) => self.db().execute("INSERT OR REPLACE INTO classical_marks (album, mark, at) VALUES (?1, ?2, ?3)", params![album, m, at])?,
            None => self.db().execute("DELETE FROM classical_marks WHERE album = ?1", [album])?,
        };
        Ok(())
    }

    /// Every record's mark, by its Music-form id.
    pub fn classical_marks(&self) -> Result<HashMap<String, String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT album, mark FROM classical_marks")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// One record's mark.
    pub fn classical_mark(&self, album: &str) -> Result<Option<String>> {
        Ok(self.db().query_row("SELECT mark FROM classical_marks WHERE album = ?1", [album], |r| r.get(0)).optional()?)
    }

    /// Every composer key given out: key → (name, the key it is now).
    pub fn composer_keys(&self) -> Result<HashMap<String, (String, Option<String>)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT key, name, alias_of FROM composer_keys")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Keys given out now (key, name), kept from their first time; and names now found to be another key (key, the key
    /// it is): once, never moved again.
    pub fn note_composer_keys(&self, seen: &[(String, String)], aliases: &[(String, String)], at: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut add = tx.prepare_cached("INSERT OR IGNORE INTO composer_keys (key, name, at) VALUES (?1, ?2, ?3)")?;
            for (key, name) in seen {
                add.execute(params![key, name, at])?;
            }
            let mut alias = tx.prepare_cached("UPDATE composer_keys SET alias_of = ?2 WHERE key = ?1 AND alias_of IS NULL AND key <> ?2")?;
            for (key, to) in aliases {
                alias.execute(params![key, to])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// What the passes found, cheaply (`app::classical`: read again when it moves): how many of each and when last.
    pub fn works_stamp(&self) -> Result<i64> {
        let db = self.db();
        let mut stamp: i64 = 0;
        for table in ["mb_releases", "mb_works", "mb_work_parents", "artist_person", "discogs_names", "discogs_link_genres"] {
            let (n, at): (i64, i64) = db.query_row(&format!("SELECT count(*), coalesce(sum(at), 0) FROM {table}"), [], |r| Ok((r.get(0)?, r.get(1)?)))?;
            stamp = stamp.wrapping_mul(31).wrapping_add(n).wrapping_mul(31).wrapping_add(at);
        }
        Ok(stamp)
    }

    /// The marks, cheaply: a mark counts at once.
    pub fn marks_stamp(&self) -> Result<i64> {
        let (n, at): (i64, i64) =
            self.db().query_row("SELECT count(*), coalesce(sum(at + length(mark) * 1000003), 0) FROM classical_marks", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(n.wrapping_mul(1_000_003).wrapping_add(at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_and_its_works_are_kept_and_one_not_found_is_asked_again_sooner() {
        let u = UserData::open(None).unwrap();
        u.set_mb_release("r1", Some("{\"genres\":[\"classical\"]}"), &[("w1".into(), "Sonata".into(), "[]".into(), None)], 100).unwrap();
        u.set_mb_release("r2", None, &[], 100).unwrap();
        assert_eq!(u.mb_releases().unwrap().keys().collect::<Vec<_>>(), ["r1"]);
        assert_eq!(u.mb_works().unwrap()["w1"].0, "Sonata");
        assert_eq!(u.mb_releases_asked(50, 50).unwrap().len(), 2);
        assert_eq!(u.mb_releases_asked(50, 200).unwrap(), ["r1".to_string()].into(), "the one not found is due first");
    }

    #[test]
    fn a_movement_s_work_is_kept_and_a_release_kept_before_is_asked_again() {
        let u = UserData::open(None).unwrap();
        // Kept before the parents were read (0.10's facts), and one read since.
        u.set_mb_release("r-old", Some("{\"genres\":[\"classical\"]}"), &[], 100).unwrap();
        let movement = ("w-5-1".to_string(), "Symphony no. 5: I.".to_string(), "[]".to_string(), Some(("w-5".to_string(), "Symphony no. 5".to_string(), Some(1))));
        u.set_mb_release("r-new", Some("{\"with_parents\":true}"), &[movement], 100).unwrap();
        u.set_mb_release("r-gone", None, &[], 100).unwrap();
        assert_eq!(u.mb_releases_without_parents().unwrap(), ["r-old".to_string()].into());
        assert_eq!(u.mb_work_parents().unwrap()["w-5-1"], ("w-5".to_string(), "Symphony no. 5".to_string(), Some(1)));
        // Asked again, MusicBrainz no longer names the parent: forgotten, and the stamp moves.
        let before = u.works_stamp().unwrap();
        u.set_mb_release("r-new", Some("{\"with_parents\":true}"), &[("w-5-1".into(), "Symphony no. 5: I.".into(), "[]".into(), None)], 101).unwrap();
        assert!(u.mb_work_parents().unwrap().is_empty());
        assert_ne!(u.works_stamp().unwrap(), before);
    }

    #[test]
    fn a_performance_name_keeps_its_person_and_its_asking_apart() {
        let u = UserData::open(None).unwrap();
        u.add_persons(&[("aphex".into(), "rdj".into(), "Richard D. James".into())]).unwrap();
        assert!(u.artists_asked(0).unwrap().is_empty(), "seen from the person's side, not asked yet");
        u.set_artist_asked("aphex", Some(45), 100).unwrap();
        assert_eq!(u.persons().unwrap()["aphex"], ("rdj".to_string(), "Richard D. James".to_string()), "asking keeps the person");
        assert_eq!(u.artist_discogs().unwrap()["aphex"], 45u64);
        u.set_discogs_names(45, Some("Richard David James"), &["AFX".into(), "Polygon Window".into()], 100).unwrap();
        assert_eq!(u.discogs_names().unwrap()[&45u64], ["Richard David James", "AFX", "Polygon Window"]);
    }

    #[test]
    fn a_mark_is_set_and_taken_away_and_moves_its_stamp() {
        let u = UserData::open(None).unwrap();
        let before = u.marks_stamp().unwrap();
        u.set_classical_mark("al-1", Some("not"), 5).unwrap();
        assert_eq!(u.classical_mark("al-1").unwrap().as_deref(), Some("not"));
        assert_ne!(u.marks_stamp().unwrap(), before);
        u.set_classical_mark("al-1", None, 6).unwrap();
        assert!(u.classical_marks().unwrap().is_empty());
    }

    #[test]
    fn a_composer_key_keeps_its_first_alias() {
        let u = UserData::open(None).unwrap();
        u.note_composer_keys(&[("name:brahms".into(), "Brahms".into())], &[], 1).unwrap();
        u.note_composer_keys(&[("name:brahms".into(), "Other".into())], &[("name:brahms".into(), "mb-brahms".into())], 2).unwrap();
        u.note_composer_keys(&[], &[("name:brahms".into(), "mb-other".into())], 3).unwrap();
        assert_eq!(u.composer_keys().unwrap()["name:brahms"], ("Brahms".to_string(), Some("mb-brahms".to_string())));
    }
}
