//! An artist's id for life (docs/records-and-copies.md, step 8): a number given once per name, as a track's and a record's
//! are, and served as `ar-<id>`. The hash of the name it replaces could only ever be one id per name; a number can be split
//! when references say two people share one. Until then one name is one artist, and a row stays when nothing credits them
//! any more, so they come back to their id, and to what the user data holds under it.

use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::{Catalogue, now};
use crate::serve::ids;

pub(super) const SCHEMA: &str = "
-- C55 (the owner, 2026-11-16): each artist's number among the artists of their name (`key`), given once in the order they were
-- made, the first 1, and kept for life: a forgotten artist's row stays (RT01b), so their number is never given again. A view
-- that shows two artists of one name shows it: \"Rone (1)\", \"Rone (2)\".
CREATE TABLE IF NOT EXISTS artist_numbers (artist INTEGER PRIMARY KEY, key TEXT NOT NULL, number INTEGER NOT NULL);
CREATE UNIQUE INDEX IF NOT EXISTS artist_numbers_once ON artist_numbers (key, number);
";

/// An artist given their number (C55): the next of their key, never one given before.
fn number(db: &Connection, artist: i64, key: &str) -> Result<()> {
    db.prepare_cached("INSERT INTO artist_numbers (artist, key, number) VALUES (?1, ?2, (SELECT COALESCE(max(number), 0) + 1 FROM artist_numbers WHERE key = ?2))")?
        .execute(params![artist, key])?;
    Ok(())
}

/// Every artist with no number given theirs (C55), in the order they were made: as the catalogue opens, so those from before the
/// numbers have them and a key's first is 1. How many. Blocking.
pub(super) fn number_all(db: &Connection) -> Result<usize> {
    let tx = db.unchecked_transaction()?;
    let left: Vec<(i64, String)> = {
        let mut st = tx.prepare("SELECT id, key FROM artists WHERE id NOT IN (SELECT artist FROM artist_numbers) ORDER BY id")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?
    };
    for (artist, key) in &left {
        number(&tx, *artist, key)?;
    }
    tx.commit()?;
    Ok(left.len())
}

/// An artist as their page names them (C55): their name, their number among the artists of their name, and whether another
/// artist has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub id: i64,
    pub name: String,
    pub number: Option<i64>,
    pub namesakes: bool,
}

/// The artist a key names. A split leaves two rows of one key, and the first is the one the name went on meaning: the lowest
/// number of those not forgotten (C55: a bare name from outside opens the first).
fn artist_by_key(db: &Connection, key: &str) -> Result<Option<i64>> {
    Ok(db.prepare_cached("SELECT min(id) FROM artists WHERE key = ?1")?.query_row([key], |r| r.get(0))?)
}

fn every_artist(db: &Connection) -> Result<HashMap<String, i64>> {
    let mut st = db.prepare_cached("SELECT key, min(id) FROM artists GROUP BY key")?;
    let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// A new artist of a name, with their number (C55).
fn give_artist(db: &Connection, key: &str, name: &str, born: i64) -> Result<i64> {
    db.prepare_cached("INSERT INTO artists (born, name, key) VALUES (?1, ?2, ?3)")?.execute(params![born, name, key])?;
    let artist = db.last_insert_rowid();
    number(db, artist, key)?;
    Ok(artist)
}

/// Every artist's id by key, these names' among them, in the caller's transaction: at first start the migration's.
pub(super) fn artists_in(tx: &Connection, names: &BTreeSet<String>, born: i64) -> Result<HashMap<String, i64>> {
    let mut known = every_artist(tx)?;
    for name in names {
        let key = ids::artist_key(name);
        // Two spellings of a new name meet here: the first gives the id, the second finds it.
        if !known.contains_key(&key) {
            let id = give_artist(tx, &key, name, born)?;
            known.insert(key, id);
        }
    }
    Ok(known)
}

/// Notes where each artist's id from before 0.16 went, in the transaction that gives them, as the map commits with what it
/// maps. The table alone says: `key` is what the hash was made of, so no name needs looking up. Safe to repeat. The count of
/// rows new here.
pub(super) fn note_artists_moved(tx: &Connection) -> Result<usize> {
    let mut note = tx.prepare_cached("INSERT OR IGNORE INTO artist_moved (old, id) VALUES (?1, ?2)")?;
    let mut noted = 0;
    for (key, id) in every_artist(tx)? {
        noted += note.execute(params![ids::old_artist(&key), id])?;
    }
    Ok(noted)
}

impl Catalogue {
    /// The artist a name is, given an id if it has none: for a name that must have one, a heart on someone no file credits
    /// yet. Blocking.
    pub fn artist_for(&self, name: &str) -> Result<i64> {
        let key = ids::artist_key(name);
        let db = self.db();
        match artist_by_key(&db, &key)? {
            Some(id) => Ok(id),
            None => give_artist(&db, &key, name, now()),
        }
    }

    /// An artist of their own, though another has the name (C55): a new id, and the next number of the name. A split, and
    /// Beyond's artists (C57, `artist_by_reference`), which no name merges. Blocking.
    #[cfg_attr(not(test), allow(dead_code))] // a split
    pub fn artist_apart(&self, name: &str) -> Result<i64> {
        give_artist(&self.db(), &ids::artist_key(name), name, now())
    }

    /// The artist a source's id names (C57: an artist of Beyond's opened, OQ12 (a)): the one who holds it; else, where the id is the
    /// source's first sure match for the name (`sure`, U6), the one live artist of the name who holds no id at that source (Z2), as
    /// the monthly read finds a library artist by their name (`discover::mb_artist_by_name`); else one of their own made now, as
    /// `artist_apart` makes them (the name's next number where another has it). Two or more of the name, one holding another id,
    /// or an id not the name's sure match (a namesake opened on purpose) say nothing of which: a new artist. The one found or made
    /// holds the id, found `how`. In one transaction, so two opens at once make one artist. Blocking.
    pub fn artist_by_reference(&self, r: &super::records::Ref, name: &str, how: &str, sure: bool) -> Result<i64> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let held: Vec<String> = tx
            .prepare_cached("SELECT subject FROM \"references\" WHERE source = ?1 AND kind = ?2 AND value = ?3 ORDER BY subject")?
            .query_map(params![r.source, r.kind, r.value], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        if let Some(artist) = held.iter().find_map(|s| ids::artist_number(s)) {
            tx.commit()?;
            return Ok(artist);
        }
        let key = ids::artist_key(name);
        // The live artists of the name (a forgotten one's row goes, RT01b, its tombstone kept), and whether each holds an id at
        // that source.
        let named: Vec<(i64, bool)> = tx
            .prepare_cached(
                "SELECT a.id, EXISTS (SELECT 1 FROM \"references\" f WHERE f.subject = 'ar-' || a.id AND f.source = ?2 AND f.kind = ?3)
                 FROM artists a WHERE a.key = ?1 AND NOT EXISTS (SELECT 1 FROM forgotten g WHERE g.subject = 'ar-' || a.id) ORDER BY a.id",
            )?
            .query_map(params![key, r.source, r.kind], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let at = now();
        let artist = match named.as_slice() {
            [(only, false)] if sure => *only,
            _ => give_artist(&tx, &key, name, at)?,
        };
        super::records::put_reference(&tx, &ids::artist(artist), r, how, at, false)?;
        tx.commit()?;
        Ok(artist)
    }

    /// Each of these artists' number for their name (C55, G5), where they have one, in one query: what a list shows as "Name (n)"
    /// where it holds two of a name. On a reader. Blocking.
    pub fn artist_numbers(&self, artists: &[i64]) -> Result<HashMap<i64, i64>> {
        if artists.is_empty() {
            return Ok(HashMap::new());
        }
        let read = |db: &Connection| -> Result<HashMap<i64, i64>> {
            let ids = artists.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            let mut st = db.prepare(&format!("SELECT artist, number FROM artist_numbers WHERE artist IN ({ids})"))?;
            let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// An artist as their page names them (C55): their name, number, and whether another artist has the name. None: no such
    /// artist, or one forgotten. On a reader. Blocking.
    pub fn artist_named(&self, artist: i64) -> Result<Option<Named>> {
        let read = |db: &Connection| -> Result<Option<Named>> {
            let mut st = db.prepare_cached(
                "SELECT a.name, n.number, EXISTS (SELECT 1 FROM artists b WHERE b.key = a.key AND b.id <> a.id)
                 FROM artists a LEFT JOIN artist_numbers n ON n.artist = a.id WHERE a.id = ?1",
            )?;
            Ok(st.query_row([artist], |r| Ok(Named { id: artist, name: r.get(0)?, number: r.get(1)?, namesakes: r.get(2)? })).optional()?)
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// The artist a name is, if it has an id. It gives none: a look at a name must not make an artist of it. Blocking.
    pub fn artist_of(&self, name: &str) -> Result<Option<i64>> {
        let key = ids::artist_key(name);
        match self.reader() {
            Some(c) => artist_by_key(&c, &key),
            None => artist_by_key(&self.db(), &key),
        }
    }

    /// Every artist's id by their key (`ids::artist_key`). Blocking.
    pub fn artist_ids(&self) -> Result<HashMap<String, i64>> {
        match self.reader() {
            Some(c) => every_artist(&c),
            None => every_artist(&self.db()),
        }
    }

    /// Every artist's id by their key, these names' among them: those new here are given theirs together, as a first build
    /// meets thousands. In the names' order, so a library read twice numbers its artists alike. Blocking.
    pub fn artists_for(&self, names: &BTreeSet<String>) -> Result<HashMap<String, i64>> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let known = artists_in(&tx, names, now())?;
        tx.commit()?;
        Ok(known)
    }

    /// `note_artists_moved`, in a transaction of its own. Blocking.
    #[cfg(test)]
    pub fn note_artists_moved(&self) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let noted = note_artists_moved(&tx)?;
        tx.commit()?;
        Ok(noted)
    }

    /// What the user data moves by: each artist's id from before 0.16 to the id it became, both as served. Blocking.
    pub fn artists_moved(&self) -> Result<HashMap<String, String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT old, id FROM artist_moved")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, ids::artist(r.get(1)?))))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn one_name_is_one_artist() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let guns = cat.artist_for("Guns N’ Roses").unwrap();
        assert_eq!(cat.artist_for("guns n' roses").unwrap(), guns, "case and typographic quotes are no other name");
        assert_ne!(cat.artist_for("Björk").unwrap(), cat.artist_for("Bjork").unwrap(), "a letter is");
        assert_eq!(cat.artist_of("GUNS N' ROSES").unwrap(), Some(guns));
        assert_eq!(cat.artist_of("Nobody Yet").unwrap(), None);
        assert_eq!(cat.artist_of("Nobody Yet").unwrap(), None, "looking gave no id");

        // Two spellings first met together are one too, by the first in order.
        let known = cat.artists_for(&names(&["THE ORB", "The Orb", "Björk"])).unwrap();
        assert_eq!(known.len(), 4, "{known:?}");
        assert_eq!(known[&ids::artist_key("the orb")], cat.artist_for("The Orb").unwrap());
        let rows: Vec<(String, String)> = cat.db().prepare("SELECT name, key FROM artists ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
        let written = |n: &str, k: &str| (n.to_string(), k.to_string());
        assert_eq!(rows, [written("Guns N’ Roses", "guns n' roses"), written("Björk", "björk"), written("Bjork", "bjork"), written("THE ORB", "the orb")]);
        assert_eq!(cat.artist_ids().unwrap(), known);
    }

    /// CR3, as for records: in a new file `ar-12` in a favourite or a crate must not quietly name another artist.
    #[test]
    fn a_fresh_start_numbers_artists_above_every_old_artist_id() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        cat.number_above(3_000_000).unwrap();
        assert_eq!(cat.artist_for("New").unwrap(), 3_000_001);
        assert_eq!(cat.top_id(), 3_000_001, "what the user data remembers next");
        let fresh = Catalogue::open(None, Vec::new()).unwrap();
        assert_eq!(fresh.artist_for("New").unwrap(), 1, "as any new catalogue");
        assert_eq!(fresh.top_id(), 1, "an artist counts though no track was given");
    }

    /// The map is the artists' table read: an old id needs no name looked up, and a second look notes nothing again.
    #[test]
    fn every_artists_old_id_leads_to_their_id_for_life() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let bjork = cat.artist_for("Björk").unwrap();
        let orb = cat.artist_for("The Orb").unwrap();
        assert_eq!(cat.note_artists_moved().unwrap(), 2);
        assert_eq!(cat.note_artists_moved().unwrap(), 0, "safe to repeat");
        let moved = cat.artists_moved().unwrap();
        assert_eq!(moved.len(), 2);
        assert_eq!(moved["ar-5vcUvYmuWJAgyxLXEUmaPz"], ids::artist(bjork), "the hash 0.15 served, from the key alone");
        assert_eq!(moved[&ids::old_artist("the orb")], ids::artist(orb));
    }

    /// A split leaves two rows of one key, and the name stays with the first (Q37: none is built yet, the table allows it).
    #[test]
    fn a_name_two_artists_share_means_the_first() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let first = cat.artist_for("Air").unwrap();
        cat.db().execute("INSERT INTO artists (born, name, key) VALUES (1, 'Air', 'air')", []).unwrap();
        assert_eq!((cat.artist_for("Air").unwrap(), cat.artist_of("Air").unwrap()), (first, Some(first)));
        assert_eq!(cat.artist_ids().unwrap()["air"], first);
        assert_eq!(cat.artists_for(&names(&["Air"])).unwrap()["air"], first);
    }

    /// C55 (the owner, 2026-11-16): each artist of a name gets a number once, in the order they were made, the first 1: those from
    /// before the numbers as the catalogue opens, those made since as they are made. A forgotten artist keeps theirs (RT01b), so
    /// it is never given again.
    #[test]
    fn same_name_artists_are_numbered_once_for_life() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        for name in ["Air", "Rone", "AIR"] {
            cat.db().execute("INSERT INTO artists (born, name, key) VALUES (1, ?1, ?2)", params![name, ids::artist_key(name)]).unwrap();
        }
        assert_eq!(number_all(&cat.db()).unwrap(), 3, "those from before");
        assert_eq!(number_all(&cat.db()).unwrap(), 0, "once");
        let number = |artist: i64| cat.db().query_row("SELECT number FROM artist_numbers WHERE artist = ?1", [artist], |r| r.get::<_, i64>(0)).optional().unwrap();
        assert_eq!([1, 2, 3].map(number), [Some(1), Some(1), Some(2)], "in the order they were made, a name's own count");
        let rone = cat.artist_apart("Rone").unwrap();
        assert_eq!(cat.artist_named(rone).unwrap().map(|n| (n.name, n.number, n.namesakes)), Some(("Rone".to_string(), Some(2), true)));
        assert_eq!(cat.artist_named(3).unwrap().map(|n| n.namesakes), Some(true), "AIR shares Air's name");
        let older: BTreeSet<i64> = [1, 2, 3].into();
        assert_eq!(cat.forget_quiet_artists(now() + 2 * 86_400, &older).unwrap(), [rone]);
        assert_eq!(cat.artist_named(rone).unwrap(), None, "forgotten");
        assert_eq!(number(rone), Some(2), "their number stays theirs");
        let next = cat.artist_apart("rone").unwrap();
        assert_eq!(number(next), Some(3), "and is not given again");
        assert_eq!(cat.artist_named(2).unwrap().map(|n| (n.number, n.namesakes)), Some((Some(1), true)));
    }

    /// C55: a bare name from outside (Home Assistant, an old address) opens the first artist of it, the lowest number of those not
    /// forgotten, as it always meant the first id; one made since of the name is no answer to it.
    #[test]
    fn a_bare_name_opens_the_first() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let first = cat.artist_for("Rone").unwrap();
        let second = cat.artist_apart("RONE").unwrap();
        assert_ne!(first, second);
        assert_eq!((cat.artist_of("rone").unwrap(), cat.artist_for("Rone").unwrap()), (Some(first), first));
        assert_eq!(cat.artist_ids().unwrap()["rone"], first);
        assert_eq!([first, second].map(|a| cat.artist_named(a).unwrap().and_then(|n| n.number)), [Some(1), Some(2)]);
        let kept: BTreeSet<i64> = [second].into();
        assert_eq!(cat.forget_quiet_artists(now() + 2 * 86_400, &kept).unwrap(), [first]);
        assert_eq!(cat.artist_of("Rone").unwrap(), Some(second), "the first left: the lowest number now");
    }
}
