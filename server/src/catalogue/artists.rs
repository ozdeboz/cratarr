//! An artist's id for life (docs/records-and-copies.md, step 8): a number given once per name, as a track's and a record's
//! are, and served as `ar-<id>`. The hash of the name it replaces could only ever be one id per name; a number can be split
//! when references say two people share one. Until then one name is one artist, and a row stays when nothing credits them
//! any more, so they come back to their id, and to what the user data holds under it.

use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use rusqlite::{Connection, params};

use super::{Catalogue, now};
use crate::serve::ids;

/// The artist a key names. A split leaves two rows of one key, and the first is the one the name went on meaning.
fn artist_by_key(db: &Connection, key: &str) -> Result<Option<i64>> {
    Ok(db.prepare_cached("SELECT min(id) FROM artists WHERE key = ?1")?.query_row([key], |r| r.get(0))?)
}

fn every_artist(db: &Connection) -> Result<HashMap<String, i64>> {
    let mut st = db.prepare_cached("SELECT key, min(id) FROM artists GROUP BY key")?;
    let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn give_artist(db: &Connection, key: &str, name: &str, born: i64) -> Result<i64> {
    db.prepare_cached("INSERT INTO artists (born, name, key) VALUES (?1, ?2, ?3)")?.execute(params![born, name, key])?;
    Ok(db.last_insert_rowid())
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
}
