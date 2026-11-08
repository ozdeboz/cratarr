//! The first records (decision 0101 point 21, docs/records-and-copies.md, "The migration"): the catalogue's step of the first
//! start of 0.16. Every album there is gets its record, copies of one record take one set of tracks, and each id an album or an
//! artist went by is kept with what it became, all in one transaction: a track that went is on no file after it, so the map
//! could never be worked out again, and the user data's step, last, may need it after a stop. What was read comes from the
//! catalogue, as a new layout has dropped 0.15's `library.sqlite` by now. Once done it says so, and is not done again.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{AlbumCopy, Catalogue, InUse, ServedRow, artists, now, rows_in, taken_by};
use crate::moved::Moved;
use crate::serve::copies::Kind;
use crate::serve::ids;
use crate::userdata::albums::{Albums, OldCopy};

/// The meta key of what this step found: its presence says it has run.
const DONE: &str = "first_records";

/// A file with a track that no list serves: gone a while, in a trash, or in a folder parked.
const UNSERVED: &str = "NOT (folder >= 0 AND gone_at IS NULL AND trashed IS NULL) AND f.track IS NOT NULL";

/// What the first start gives records to (`serve::library::first_albums`).
#[derive(Debug, Default)]
pub struct FirstAlbums {
    /// What the libraries list.
    pub served: Vec<AlbumCopy>,
    /// What no list shows and can still come back (CR13): files gone a while, in a trash, in a parked folder. With no record
    /// now, an Undo or a source switched on again would bring its album back as new, and what names it would stay under its
    /// old id for good.
    pub unserved: Vec<AlbumCopy>,
    /// Every name these credit: no build has run, so no name has its id yet.
    pub names: BTreeSet<String>,
}

/// What the step found, for the report: the catalogue cannot write that part itself, as only the user data holds it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct First {
    pub records: u64,
    /// Albums given a record: two copies of one record count two, and what no list shows counts too, so neither this nor `records`
    /// is the number of albums listed.
    pub copies: u64,
    pub tracks_merged: u64,
    pub bases_on_two_records: u64,
    pub artists: u64,
    /// Rows of `analysis_shape` that followed their file to the place its library has, and those that could not say which file
    /// they are of. `beats_*`: the same of `beat_this`.
    pub shapes_moved: u64,
    pub shapes_left: u64,
    pub beats_moved: u64,
    pub beats_left: u64,
}

/// Where a library's albums come in (N4, CR18): the main libraries, then own music, then Incoming, each in the libraries' order,
/// as of two tracks found to be one the first met stays, and of two rows that cannot both stand, the first met does. A folder
/// listed nowhere comes last.
fn place_of(kinds: &[(i64, Kind)], folder: i64) -> (u8, usize) {
    match kinds.iter().position(|(f, _)| *f == folder) {
        Some(at) => (
            match kinds[at].1 {
                Kind::Main => 0,
                Kind::Own => 1,
                Kind::Incoming => 2,
            },
            at,
        ),
        None => (3, kinds.len()),
    }
}

fn done(db: &Connection) -> Result<Option<First>> {
    let kept: Option<String> = db.query_row("SELECT value FROM meta WHERE key = ?1", [DONE], |r| r.get(0)).optional()?;
    Ok(kept.map(|k| serde_json::from_str(&k)).transpose()?)
}

fn count(db: &Connection, sql: &str) -> Result<u64> {
    Ok(db.query_row(sql, [], |r| r.get::<_, i64>(0))? as u64)
}

/// Where a library that changed place left its files' rows: 0.15's `set_folders` took `analysis` along but not these, so `analysis`
/// says a file is done, nothing asks again, and its waveform and beats are read by nothing, or by a file that comes to that path.
/// Each goes to the one folder that holds its path, where its own holds none. What stays beside a file with no row of its own is
/// counted, as a path alone cannot say which of the two it is of. Music's place never moved. How many moved and stayed, by table.
fn follow_files(tx: &Connection) -> Result<[(u64, u64); 2]> {
    tx.execute_batch(
        "CREATE TEMP TABLE places (rel TEXT NOT NULL, folder INTEGER NOT NULL, PRIMARY KEY (rel, folder)) WITHOUT ROWID;
         INSERT INTO temp.places SELECT rel, folder FROM files WHERE folder >= 0;",
    )?;
    let follow = |table: &str| -> Result<(u64, u64)> {
        let moved = tx.execute(
            &format!(
                "UPDATE OR IGNORE {table} SET folder = (SELECT p.folder FROM temp.places p WHERE p.rel = {table}.rel)
                 WHERE rel IN (SELECT rel FROM temp.places GROUP BY rel HAVING count(*) = 1)
                   AND NOT EXISTS (SELECT 1 FROM temp.places p WHERE p.rel = {table}.rel AND p.folder = {table}.folder)"
            ),
            [],
        )?;
        let left = count(
            tx,
            &format!(
                "SELECT count(*) FROM {table} s WHERE s.folder > 0 AND EXISTS (
                   SELECT 1 FROM temp.places p WHERE p.rel = s.rel AND p.folder <> s.folder AND NOT EXISTS (SELECT 1 FROM {table} o WHERE o.folder = p.folder AND o.rel = p.rel))"
            ),
        )?;
        Ok((moved as u64, left))
    };
    let found = [follow("analysis_shape")?, follow("beat_this")?];
    tx.execute_batch("DROP TABLE temp.places;")?;
    Ok(found)
}

/// Every album in the order the map keeps, and each one's record, what lists serve first: a file that went must not decide
/// which of two tracks stays. A piece of a served album no list shows, the tracks a person deleted from it, is no album: it joins
/// that album's record, or its id would name two.
fn give_records(tx: &Connection, mut served: Vec<AlbumCopy>, unserved: Vec<AlbumCopy>, kinds: &[(i64, Kind)], now: i64) -> Result<(Vec<AlbumCopy>, Vec<Option<i64>>)> {
    let place = |a: &AlbumCopy| place_of(kinds, a.folder);
    served.sort_by_key(place);
    let listed: HashSet<&str> = served.iter().map(|a| a.old_id.as_str()).collect();
    let (pieces, mut unserved): (Vec<AlbumCopy>, Vec<AlbumCopy>) = unserved.into_iter().partition(|a| listed.contains(a.old_id.as_str()));
    unserved.sort_by_key(place);
    let albums: Vec<AlbumCopy> = served.into_iter().chain(unserved).collect();
    let (_, records) = Catalogue::assign_in(tx, &albums, now)?;
    let of_id: HashMap<&str, i64> = albums.iter().zip(&records).filter_map(|(a, r)| Some((a.old_id.as_str(), (*r)?))).collect();
    let mut join = tx.prepare_cached("UPDATE tracks SET record = ?2, disc = ?3, number = ?4 WHERE id = ?1 AND record IS NULL")?;
    for a in &pieces {
        let Some(record) = of_id.get(a.old_id.as_str()) else { continue };
        for (track, disc, number) in &a.tracks {
            join.execute(params![taken_by(tx, *track)?, record, disc, number])?;
        }
    }
    drop(join);
    Ok((albums, records))
}

/// Both forms of the map. A form `id` row names one copy, so of two that would say another record the first stands: real ids
/// come before the ones worked out, and a real album's id is never taken for a kept one's.
fn note_albums(tx: &Connection, albums: &[AlbumCopy], records: &[Option<i64>], kinds: &[(i64, Kind)]) -> Result<()> {
    let mut id = tx.prepare_cached("INSERT OR IGNORE INTO album_moved (old, form, record, folder, rank) VALUES (?1, 'id', ?2, ?3, ?4)")?;
    let mut base = tx.prepare_cached("INSERT OR IGNORE INTO album_moved (old, form, record, rank) VALUES (?1, 'base', ?2, ?3)")?;
    let given = || albums.iter().zip(records).enumerate().filter_map(|(rank, (a, r))| Some((rank as i64, a, (*r)?)));
    for (rank, a, record) in given() {
        id.execute(params![a.old_id, record, a.folder, rank])?;
        base.execute(params![a.old_base, record, rank])?;
    }
    // What names an album by an id it does not have now. A Keep left its plays under the id it had in Incoming (`data` section 10), a
    // copy kept in Music and gone left what went by Music's while the release waits in Incoming, and a folder's id was its place in a
    // sorted list, so a library that moved along left its rows under the place it had. They are found by working the id out for Music
    // and for each Incoming folder, after every real id, so the album's own row stands over them. Each is of the copy there is, where
    // it lies now: a row by the place it was worked out for would read on a copy nobody reviewed, or miss the one somebody did.
    let incoming: Vec<i64> = kinds.iter().filter(|(_, kind)| *kind == Kind::Incoming).map(|(folder, _)| *folder).collect();
    let after = albums.len() as i64;
    // Of a release on two records, the id of a place in Incoming was the copy's that lay there: Music's has one of its own.
    let mut worked_out: Vec<_> = given().collect();
    worked_out.sort_by_key(|(_, a, _)| !incoming.contains(&a.folder));
    for (rank, a, record) in worked_out {
        for folder in std::iter::once(&0).chain(&incoming) {
            id.execute(params![ids::album_at(&a.pid, *folder), record, a.folder, after + rank])?;
        }
    }
    Ok(())
}

impl Catalogue {
    /// What the step found, if it has run. Blocking.
    pub fn first_records_done(&self) -> Result<Option<First>> {
        done(&self.db())
    }

    /// Every file with a track no list serves, by folder: its id now, the id its albums went by while they were served, its
    /// rows. A folder parked before folders had ids for life does not say what it was: its own negative id then, which names
    /// nothing, leaving its Music-form ids and those worked out for each Incoming folder to find what named it. Blocking.
    pub fn unserved_rows(&self) -> Result<Vec<(i64, i64, Vec<ServedRow>)>> {
        let db = self.db();
        let folders: Vec<(i64, Option<i64>)> = db.prepare("SELECT id, home FROM folders ORDER BY id")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        folders.into_iter().map(|(id, home)| Ok((id, home.unwrap_or(id), rows_in(&db, id, None, UNSERVED)?))).collect()
    }

    /// Whether this catalogue has been through this step: all a start asks once the first start is over. Blocking.
    pub fn first_records_given(&self) -> Result<bool> {
        Ok(self.db().prepare_cached("SELECT 1 FROM meta WHERE key = ?1")?.exists([DONE])?)
    }

    /// Says this step is past, for a catalogue started afresh after the first start: it has nothing to give records to, and must not
    /// read as one from before it. Blocking.
    pub fn first_records_over(&self) -> Result<()> {
        self.db().execute("INSERT OR IGNORE INTO meta (key, value) VALUES (?1, ?2)", params![DONE, serde_json::to_string(&First::default())?])?;
        Ok(())
    }

    /// The libraries that hold files and cannot be seen, as a mount that is not up looks: in use and showing nothing, or parked by
    /// this very open, as a folder that is gone reads as out of use. `home` tells which: every folder in use has one by then, and
    /// one 0.15 parked has none. So it holds however a server says its paths, and for a source that cannot say. Blocking.
    pub fn folders_not_there(&self) -> Result<Vec<PathBuf>> {
        let (held, parked): (HashSet<i64>, Vec<PathBuf>) = {
            let db = self.db();
            let mut st = db.prepare(&format!("SELECT DISTINCT folder FROM files WHERE {}", super::PRESENT))?;
            let held = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            let mut st = db.prepare("SELECT path FROM folders d WHERE id < 0 AND home IS NOT NULL AND EXISTS (SELECT 1 FROM files f WHERE f.folder = d.id)")?;
            let parked = st.query_map([], |r| Ok(PathBuf::from(r.get::<_, String>(0)?)))?.collect::<rusqlite::Result<_>>()?;
            (held, parked)
        };
        let empty = self.folders().iter().filter(|(id, path)| held.contains(id) && super::holds_nothing(&path.to_string_lossy())).map(|(_, path)| path.clone()).collect::<Vec<_>>();
        Ok(empty.into_iter().chain(parked).collect())
    }

    /// Gives every album its record, its copies one set of tracks, and every name its artist's id, and keeps what each old id
    /// became, in one transaction. `kinds`: each folder's library, in the libraries' order. Safe to repeat: done, it does
    /// nothing, whatever it is given. What it found. Blocking.
    pub fn first_records(&self, found: FirstAlbums, kinds: &[(i64, Kind)]) -> Result<First> {
        let mut db = self.db();
        let tx = db.transaction()?;
        if let Some(found) = done(&tx)? {
            return Ok(found);
        }
        let now = now();
        let FirstAlbums { served, unserved, names } = found;
        let (albums, records) = give_records(&tx, served, unserved, kinds, now)?;
        note_albums(&tx, &albums, &records, kinds)?;
        let [(shapes_moved, shapes_left), (beats_moved, beats_left)] = follow_files(&tx)?;
        // As old as what it holds: to the passes after this it is no folder still arriving, and all that names it stays (CR19).
        tx.execute("UPDATE records SET born = COALESCE((SELECT min(t.born) FROM tracks t WHERE t.record = records.id), born)", [])?;
        artists::artists_in(&tx, &names, now)?;
        artists::note_artists_moved(&tx)?;
        // What Cratarr was handing a file names the track as it was (0101 point 11).
        tx.execute("UPDATE handovers SET track = (SELECT id FROM track_moved WHERE old = handovers.track) WHERE track IN (SELECT old FROM track_moved)", [])?;
        let first = First {
            records: count(&tx, "SELECT count(*) FROM records")?,
            copies: records.iter().flatten().count() as u64,
            tracks_merged: count(&tx, "SELECT count(*) FROM track_moved")?,
            bases_on_two_records: count(&tx, "SELECT count(*) FROM (SELECT old FROM album_moved WHERE form = 'base' GROUP BY old HAVING count(*) > 1)")?,
            artists: count(&tx, "SELECT count(*) FROM artists")?,
            shapes_moved,
            shapes_left,
            beats_moved,
            beats_left,
        };
        tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![DONE, serde_json::to_string(&first)?])?;
        tx.commit()?;
        Ok(first)
    }

    /// The albums' map as the user data moves by it, in the order of its ranks. Blocking.
    pub fn albums_moved(&self) -> Result<Albums> {
        let db = self.db();
        let mut st = db.prepare("SELECT old, record, folder FROM album_moved WHERE form = 'id' ORDER BY rank, old")?;
        let copies = st.query_map([], |r| Ok(OldCopy { id: r.get(0)?, record: ids::record(r.get(1)?), folder: r.get(2)? }))?.collect::<rusqlite::Result<_>>()?;
        let mut st = db.prepare("SELECT old, record FROM album_moved WHERE form = 'base' ORDER BY rank, old")?;
        let bases = st.query_map([], |r| Ok((r.get(0)?, ids::record(r.get(1)?))))?.collect::<rusqlite::Result<_>>()?;
        Ok(Albums { copies, bases })
    }

    /// What the stores in the data folder move by: every id that went, as it was served, an album's by what one copy was known
    /// by, as a store holds what it was given; and every file's line, gone ones and those in a folder parked too, to its track.
    /// A file in a trash lost its path as it went, so a line can name it no more. Blocking.
    pub fn moved(&self) -> Result<Moved> {
        let folders = self.folders();
        let db = self.db();
        let mut pairs: HashMap<String, String> = HashMap::new();
        let mut st = db.prepare("SELECT old, record FROM album_moved WHERE form = 'id'")?;
        pairs.extend(st.query_map([], |r| Ok((r.get::<_, String>(0)?, ids::record(r.get(1)?))))?.collect::<rusqlite::Result<Vec<_>>>()?);
        let mut st = db.prepare("SELECT old, id FROM artist_moved")?;
        pairs.extend(st.query_map([], |r| Ok((r.get::<_, String>(0)?, ids::artist(r.get(1)?))))?.collect::<rusqlite::Result<Vec<_>>>()?);
        let mut st = db.prepare("SELECT old, id FROM track_moved")?;
        pairs.extend(st.query_map([], |r| Ok((ids::song(r.get(0)?), ids::song(r.get(1)?))))?.collect::<rusqlite::Result<Vec<_>>>()?);
        Ok(Moved { ids: pairs, lines: lines(&db, &folders)? })
    }

    /// What each of `old` is now, for a device's one fresh start at 0.16 (decision 0101 point 23): those that moved, old id to new
    /// id, ids only. One left out did not move, or is no id from before. Blocking.
    pub fn named_now_all(&self, old: &[String]) -> std::collections::BTreeMap<String, String> {
        old.iter().filter_map(|o| self.named_now(o).map(|now| (o.clone(), now))).collect()
    }

    /// What an id from before 0.16 is now, for a link or a seed kept elsewhere: an album's by what one copy was known by, an
    /// artist's or a track's. None for an id that did not move. Blocking.
    pub fn named_now(&self, old: &str) -> Option<String> {
        let db = self.db();
        let of = |sql: &str, key: &dyn rusqlite::ToSql| db.prepare_cached(sql).ok()?.query_row([key], |r| r.get::<_, i64>(0)).optional().ok().flatten();
        if let Some(track) = ids::track_of(old) {
            return of("SELECT id FROM track_moved WHERE old = ?1", &track).map(ids::song);
        }
        if old.starts_with(ids::ARTIST) {
            return of("SELECT id FROM artist_moved WHERE old = ?1", &old).map(ids::artist);
        }
        of("SELECT record FROM album_moved WHERE old = ?1 AND form = 'id'", &old).map(ids::record)
    }
}

/// Every file's line to its track, by the folder's path, as a folder parked is not among those in use.
fn lines(db: &Connection, folders: &InUse) -> Result<HashMap<String, String>> {
    let Some((first, root)) = folders.first() else { return Ok(HashMap::new()) };
    let mut st = db.prepare("SELECT f.folder, f.rel, f.track, d.path FROM files f JOIN folders d ON d.id = f.folder WHERE f.track IS NOT NULL")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)))?;
    let mut out = HashMap::new();
    for row in rows {
        let (folder, rel, track, path) = row?;
        let line = if folder == first { Some(rel) } else { crate::playlists::relative_to(root, &Path::new(&path).join(&rel)) };
        out.extend(line.map(|line| (line, ids::song(track))));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{copy, file_of_track, record_of, retired, tracks};
    use super::*;

    const MAIN: (i64, Kind) = (0, Kind::Main);
    const INCOMING: (i64, Kind) = (1, Kind::Incoming);

    /// An album's ids as a folder gives them, from its tags as `pid`.
    fn of(mut a: AlbumCopy, folder: i64, pid: &str) -> AlbumCopy {
        a.folder = folder;
        a.pid = pid.to_string();
        a.old_id = ids::album_at(pid, folder);
        a.old_base = ids::album_at(pid, 0);
        a
    }

    /// The album of three tracks on these ids, its files in `folder`.
    fn three(cat: &Catalogue, folder: i64, ids: [i64; 3], dir: &str, pid: &str) -> AlbumCopy {
        let a = copy(None, "A", "Three", &[(ids[0], "One", 200_000), (ids[1], "Two", 200_000), (ids[2], "Three", 200_000)]);
        let mut a = of(a, folder, pid);
        a.files = ids.iter().map(|t| (folder, format!("{dir}/{t}.flac"))).collect();
        for (track, (_, rel)) in ids.iter().zip(&a.files) {
            file_of_track(cat, rel, *track);
            cat.db().execute("UPDATE files SET folder = ?1 WHERE rel = ?2", params![folder, rel]).unwrap();
        }
        a
    }

    fn found(served: Vec<AlbumCopy>) -> FirstAlbums {
        FirstAlbums { served, ..Default::default() }
    }

    fn map(cat: &Catalogue) -> Vec<(String, String, i64)> {
        let db = cat.db();
        let mut st = db.prepare("SELECT old, form, record FROM album_moved ORDER BY rank, old, form").unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
    }

    /// N4: `album_copies` comes out by hash, and of two tracks found to be one the album met first stays, so without the sort
    /// the main library's ids could be the ones to go.
    #[test]
    fn the_main_librarys_id_survives() {
        let cat = tracks(6);
        let waiting = three(&cat, 1, [1, 2, 3], "In", "pid");
        let kept = three(&cat, 0, [4, 5, 6], "Kept", "pid");
        let done = cat.first_records(found(vec![waiting, kept]), &[MAIN, INCOMING]).unwrap();
        assert_eq!(cat.moved_tracks().unwrap(), HashMap::from([(1, 4), (2, 5), (3, 6)]), "Incoming's ids go, listed first or not");
        assert_eq!((cat.track_of(1, "In/1.flac"), cat.track_of(0, "Kept/4.flac")), (Some(4), Some(4)));
        assert!((1..=3).all(|t| retired(&cat, t)));
        assert_eq!((done.records, done.copies, done.tracks_merged), (1, 2, 3));
    }

    /// Whatever order the libraries are listed in: it is the kind that decides, then the place among main libraries.
    #[test]
    fn of_two_main_libraries_the_first_listed_survives() {
        let cat = tracks(9);
        let (in_third, second, first) = (three(&cat, 2, [1, 2, 3], "Third", "pid"), three(&cat, 1, [4, 5, 6], "Second", "pid"), three(&cat, 0, [7, 8, 9], "First", "pid"));
        let kinds = [(2, Kind::Incoming), (1, Kind::Main), (0, Kind::Main)];
        cat.first_records(found(vec![in_third, first, second]), &kinds).unwrap();
        assert_eq!(cat.track_of(0, "First/7.flac"), Some(4), "the first main library in the list is folder 1 here");
        assert_eq!(cat.track_of(2, "Third/1.flac"), Some(4));
    }

    /// CR18: main, then someone's own music, then Incoming, whatever the list's order and whichever came first.
    #[test]
    fn own_music_comes_after_a_main_library_and_before_incoming() {
        let cat = tracks(6);
        let (waiting, own) = (three(&cat, 1, [1, 2, 3], "In", "pid"), three(&cat, 2, [4, 5, 6], "Own", "pid"));
        cat.first_records(found(vec![waiting, own]), &[(1, Kind::Incoming), (2, Kind::Own)]).unwrap();
        assert_eq!(cat.track_of(1, "In/1.flac"), Some(4));
        let cat = tracks(6);
        let (own, kept) = (three(&cat, 2, [1, 2, 3], "Own", "pid"), three(&cat, 0, [4, 5, 6], "Kept", "pid"));
        cat.first_records(found(vec![own, kept]), &[(2, Kind::Own), (0, Kind::Main)]).unwrap();
        assert_eq!(cat.track_of(2, "Own/1.flac"), Some(4));
    }

    /// One base, two records: the same tags on other tracks. Tags and words kept by the base go to both.
    #[test]
    fn one_base_may_name_two_records() {
        let cat = tracks(7);
        let plain = three(&cat, 0, [1, 2, 3], "Plain", "pid");
        let extra = copy(None, "A", "Three", &[(4, "One", 200_000), (5, "Two", 200_000), (6, "Three", 200_000), (7, "Four", 200_000)]);
        let extra = of(extra, 1, "pid");
        let done = cat.first_records(found(vec![plain.clone(), extra.clone()]), &[MAIN, INCOMING]).unwrap();
        assert_eq!((done.records, done.bases_on_two_records), (2, 1));
        let rows = map(&cat);
        let bases: Vec<i64> = rows.iter().filter(|(old, form, _)| *old == plain.old_base && form == "base").map(|r| r.2).collect();
        assert_eq!(bases, [record_of(&cat, 1).unwrap(), record_of(&cat, 4).unwrap()], "main library's first");
        let moved = cat.albums_moved().unwrap();
        assert_eq!(moved.bases.len(), 2);
        let ids: Vec<&str> = moved.copies.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids.iter().filter(|id| **id == plain.old_id).count(), 1);
        assert_eq!(ids.iter().filter(|id| **id == extra.old_id).count(), 1, "an id stays many to one");
        assert_eq!(moved.copies[0].record, ids::record(record_of(&cat, 1).unwrap()));
    }

    #[test]
    fn first_records_is_safe_to_repeat() {
        let cat = tracks(6);
        let (kept, waiting) = (three(&cat, 0, [1, 2, 3], "Kept", "pid"), three(&cat, 1, [4, 5, 6], "In", "pid"));
        let all = || found(vec![kept.clone(), waiting.clone()]);
        assert_eq!(cat.first_records_done().unwrap(), None);
        let first = cat.first_records(all(), &[MAIN, INCOMING]).unwrap();
        let (rows, moved) = (map(&cat), cat.moved_tracks().unwrap());
        assert_eq!(cat.first_records_done().unwrap().as_ref(), Some(&first));
        assert_eq!(cat.first_records(all(), &[MAIN, INCOMING]).unwrap(), first, "what it found stands");
        assert_eq!((map(&cat), cat.moved_tracks().unwrap()), (rows, moved), "and nothing moved again");
        // Even handed what it never saw: a step done is done.
        let more = copy(None, "B", "Other", &[(7, "One", 200_000)]);
        assert_eq!(cat.first_records(found(vec![of(more, 0, "other")]), &[MAIN]).unwrap(), first);
        assert_eq!(cat.first_records_done().unwrap().unwrap().records, 1);
    }

    /// A Keep moved the album into Music and left its plays under the id it had in Incoming: an id no album has now, found by
    /// working it out from the tags.
    #[test]
    fn a_keeps_old_id_finds_its_record() {
        let cat = tracks(3);
        let kept = three(&cat, 0, [1, 2, 3], "Kept", "pid");
        cat.first_records(found(vec![kept.clone()]), &[MAIN, INCOMING, (2, Kind::Incoming)]).unwrap();
        let record = ids::record(record_of(&cat, 1).unwrap());
        for folder in [1, 2] {
            assert_eq!(cat.named_now(&ids::album_at("pid", folder)), Some(record.clone()), "from folder {folder}");
        }
        assert_eq!(cat.named_now(&kept.old_id), Some(record.clone()), "its own");
        let moved = cat.albums_moved().unwrap();
        assert_eq!(moved.copies.len(), 3);
        assert_eq!(moved.copies[0].id, kept.old_id, "its own id is met first, and a row of it stands");
        let folder_of = |folder: i64| moved.copies.iter().find(|c| c.id == ids::album_at("pid", folder)).map(|c| c.folder);
        assert_eq!((folder_of(1), folder_of(2)), (Some(0), Some(0)), "of the copy there is: a new one in Incoming was reviewed by nobody");
    }

    /// A device's fresh start asks many at once: those that moved come back as ids, and nothing else does.
    #[test]
    fn ids_asked_together_answer_those_that_moved_and_nothing_else() {
        let cat = tracks(3);
        let kept = three(&cat, 0, [1, 2, 3], "Kept", "pid");
        cat.first_records(found(vec![kept.clone()]), &[MAIN, INCOMING, (2, Kind::Incoming)]).unwrap();
        let record = ids::record(record_of(&cat, 1).unwrap());
        let asked = [kept.old_id.clone(), "al-nothing".to_string(), ids::song(1), record.clone(), "Kept".to_string()];
        assert_eq!(cat.named_now_all(&asked), std::collections::BTreeMap::from([(kept.old_id.clone(), record)]));
    }

    /// A copy kept in Music left, and the same release waits in Incoming: what went by the id Music gave it is this album's, as a
    /// Keep would have given it that id back. Music's own album holds its id all the same.
    #[test]
    fn an_id_music_gave_an_album_finds_it_in_incoming() {
        let cat = tracks(3);
        let waiting = three(&cat, 1, [1, 2, 3], "In", "pid");
        cat.first_records(found(vec![waiting]), &[MAIN, INCOMING]).unwrap();
        assert_eq!(cat.named_now(&ids::album_at("pid", 0)), Some(ids::record(record_of(&cat, 1).unwrap())));

        let cat = tracks(7);
        let plain = three(&cat, 0, [1, 2, 3], "Plain", "pid");
        let extra = of(copy(None, "A", "Three", &[(4, "One", 200_000), (5, "Two", 200_000), (6, "Three", 200_000), (7, "Four", 200_000)]), 1, "pid");
        cat.first_records(found(vec![extra, plain]), &[MAIN, INCOMING]).unwrap();
        assert_eq!(cat.named_now(&ids::album_at("pid", 0)), Some(ids::record(record_of(&cat, 1).unwrap())), "Music's own");
    }

    /// 0.15's folder ids were places in a sorted list, so a library that came in front moved the next one along, and one that went
    /// moved it back: its albums' rows lie under the place it had, and they are of the copy where it lies, or a card heard reads
    /// unheard. Either way round.
    #[test]
    fn what_went_by_the_place_a_library_had_is_of_the_copy_where_it_lies() {
        for (had, has) in [(1, 2), (2, 1)] {
            let cat = tracks(3);
            let moved_along = three(&cat, has, [1, 2, 3], "In", "pid");
            cat.first_records(found(vec![moved_along]), &[MAIN, INCOMING, (2, Kind::Incoming)]).unwrap();
            let moved = cat.albums_moved().unwrap();
            let old = moved.copies.iter().find(|c| c.id == ids::album_at("pid", had)).expect("the place it had");
            assert_eq!((old.record.as_str(), old.folder), (&*ids::record(record_of(&cat, 1).unwrap()), has));
        }
    }

    /// One release on two records, Music's and Incoming's. The id of a place in Incoming was the copy's that lay there, whichever
    /// place that is now: Music's has an id of its own, and a Keep's album on two records is the rarer of the two.
    #[test]
    fn of_two_records_the_id_of_a_place_in_incoming_is_the_copy_in_incoming() {
        let cat = tracks(7);
        let plain = three(&cat, 0, [1, 2, 3], "Plain", "pid");
        let extra = of(copy(None, "A", "Three", &[(4, "One", 200_000), (5, "Two", 200_000), (6, "Three", 200_000), (7, "Four", 200_000)]), 2, "pid");
        cat.first_records(found(vec![plain, extra]), &[MAIN, INCOMING, (2, Kind::Incoming)]).unwrap();
        assert_eq!(cat.named_now(&ids::album_at("pid", 1)), Some(ids::record(record_of(&cat, 4).unwrap())));
    }

    /// One place, two libraries: the album there now has the id an older album's rows went by while its library stood there. Where
    /// both hold a release on two records, the album that holds the id stands: what 0.15 shows on it, and what one row holds
    /// nothing can part. Either way round, as a library can go back.
    #[test]
    fn of_two_libraries_on_one_place_the_album_that_holds_the_id_stands() {
        for (here, there) in [(1, 2), (2, 1)] {
            let cat = tracks(7);
            let there_now = three(&cat, here, [1, 2, 3], "New", "pid");
            let older = of(copy(None, "A", "Three", &[(4, "One", 200_000), (5, "Two", 200_000), (6, "Three", 200_000), (7, "Four", 200_000)]), there, "pid");
            cat.first_records(found(vec![older, there_now]), &[MAIN, INCOMING, (2, Kind::Incoming)]).unwrap();
            let (now, moved_along) = (record_of(&cat, 1).unwrap(), record_of(&cat, 4).unwrap());
            assert_ne!(now, moved_along, "two records");
            assert_eq!(cat.named_now(&ids::album_at("pid", here)), Some(ids::record(now)), "the real id");
            assert_eq!(cat.named_now(&ids::album_at("pid", there)), Some(ids::record(moved_along)));
            let first = if here == 1 { now } else { moved_along };
            assert_eq!(cat.named_now(&ids::album_at("pid", 0)), Some(ids::record(first)), "what no album holds: the first by rank");
        }
    }

    /// Smaller's C30: the plain album in Music and a deluxe in Incoming with the same tags. The id worked out for the plain one
    /// is the deluxe's real id, and it must lead to the deluxe, or plays under it would land on another record.
    #[test]
    fn an_id_a_real_album_holds_is_not_worked_out_for_another() {
        let cat = tracks(7);
        let plain = three(&cat, 0, [1, 2, 3], "Plain", "pid");
        let extra = of(copy(None, "A", "Three", &[(4, "One", 200_000), (5, "Two", 200_000), (6, "Three", 200_000), (7, "Four", 200_000)]), 1, "pid");
        cat.first_records(found(vec![plain.clone(), extra.clone()]), &[MAIN, INCOMING]).unwrap();
        assert_eq!(extra.old_id, ids::album_at("pid", 1), "the two collide");
        assert_eq!(cat.named_now(&extra.old_id), Some(ids::record(record_of(&cat, 4).unwrap())));
        let moved = cat.albums_moved().unwrap();
        let ids: HashSet<&str> = moved.copies.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids.len(), moved.copies.len(), "one id, one row: `fold_pairs` would count it twice");
    }

    /// CR13: an album in a trash, gone a while, or in a parked folder gets its record and its ids, or its return is a new
    /// album, and what names it stays behind.
    #[test]
    fn what_no_list_shows_gets_its_record_and_keeps_its_ids() {
        let cat = tracks(6);
        let served = three(&cat, 0, [1, 2, 3], "Here", "here");
        let mut unserved = three(&cat, 0, [4, 5, 6], "Away", "away");
        unserved.facts.title = "away".to_string();
        let first = FirstAlbums { served: vec![served], unserved: vec![unserved.clone()], names: BTreeSet::new() };
        assert_eq!(cat.first_records(first, &[MAIN]).unwrap().records, 2);
        let record = record_of(&cat, 4).expect("its tracks hold a record: the album comes back to it");
        assert_ne!(Some(record), record_of(&cat, 1));
        assert_eq!(cat.named_now(&unserved.old_id), Some(ids::record(record)));
        assert!(cat.albums_moved().unwrap().bases.contains(&(unserved.old_base, ids::record(record))));
        assert!(!cat.records_unsettled(), "nothing is left for every later pass to go over");
    }

    /// A copy no list shows of what one does is a copy like any: on the record's tracks, and the one served decides.
    #[test]
    fn a_copy_no_list_shows_takes_the_records_tracks_and_never_gives_its_own() {
        let cat = tracks(6);
        let trashed = three(&cat, 0, [1, 2, 3], "Trash", "pid");
        let waiting = three(&cat, 1, [4, 5, 6], "In", "pid");
        let found = FirstAlbums { served: vec![waiting], unserved: vec![trashed], names: BTreeSet::new() };
        cat.first_records(found, &[MAIN, INCOMING]).unwrap();
        assert_eq!(cat.moved_tracks().unwrap(), HashMap::from([(1, 4), (2, 5), (3, 6)]), "a file that went decides nothing, even in Music");
    }

    /// A track deleted from an album still listed: the same hash, a part of the album. It takes the record and no id of its own.
    #[test]
    fn a_piece_of_a_listed_album_joins_its_record() {
        let cat = tracks(4);
        let kept = three(&cat, 0, [1, 2, 3], "Kept", "pid");
        let mut piece = of(copy(None, "A", "Three", &[(4, "Four", 200_000)]), 0, "pid");
        piece.files = vec![(0, "Trash/4.flac".to_string())];
        let found = FirstAlbums { served: vec![kept.clone()], unserved: vec![piece], names: BTreeSet::new() };
        let done = cat.first_records(found, &[MAIN]).unwrap();
        assert_eq!((done.records, done.copies), (1, 1));
        assert_eq!(record_of(&cat, 4), record_of(&cat, 1));
        assert_eq!(map(&cat).iter().filter(|(old, form, _)| *old == kept.old_id && form == "id").count(), 1);
    }

    #[test]
    fn every_name_gets_its_id_and_the_map_keeps_the_hash_it_had() {
        let cat = tracks(3);
        let kept = three(&cat, 0, [1, 2, 3], "Kept", "pid");
        let names = BTreeSet::from(["The Orb".to_string(), "Björk".to_string()]);
        let done = cat.first_records(FirstAlbums { served: vec![kept], unserved: Vec::new(), names }, &[MAIN]).unwrap();
        assert_eq!(done.artists, 2);
        let bjork = cat.artist_of("Björk").unwrap().unwrap();
        assert_eq!(cat.named_now("ar-5vcUvYmuWJAgyxLXEUmaPz"), Some(ids::artist(bjork)), "the hash 0.15 served");
        assert_eq!(cat.moved().unwrap().ids["ar-5vcUvYmuWJAgyxLXEUmaPz"], ids::artist(bjork));
    }

    /// CR19 lets a young record, a folder still arriving, join another. One given here holds years of plays, and is no such thing.
    #[test]
    fn a_first_record_is_as_old_as_its_oldest_track() {
        let cat = tracks(3);
        cat.db().execute_batch("UPDATE tracks SET born = 700; UPDATE tracks SET born = 500 WHERE id = 2;").unwrap();
        let kept = three(&cat, 0, [1, 2, 3], "Kept", "pid");
        cat.first_records(found(vec![kept]), &[MAIN]).unwrap();
        assert_eq!(cat.db().query_row("SELECT born FROM records", [], |r| r.get::<_, i64>(0)).unwrap(), 500);
    }

    /// A file being handed a track that went is handed what took it, gone rows and trashed ones following too.
    #[test]
    fn a_hand_over_follows_the_merge() {
        let cat = tracks(6);
        let (kept, waiting) = (three(&cat, 0, [1, 2, 3], "Kept", "pid"), three(&cat, 1, [4, 5, 6], "In", "pid"));
        cat.db().execute("INSERT INTO handovers (folder, rel, track, at) VALUES (1, 'New/4.flac', 4, 1)", []).unwrap();
        cat.first_records(found(vec![kept, waiting]), &[MAIN, INCOMING]).unwrap();
        let track: i64 = cat.db().query_row("SELECT track FROM handovers", [], |r| r.get(0)).unwrap();
        assert_eq!(track, 1);
    }

    /// What the stores move by, and what the albums move by, are one list: a queued Keep's job and its review part otherwise.
    #[test]
    fn the_stores_and_the_user_data_move_by_the_same_album_ids() {
        let cat = tracks(6);
        let (kept, waiting) = (three(&cat, 0, [1, 2, 3], "Kept", "pid"), three(&cat, 1, [4, 5, 6], "In", "pid"));
        cat.first_records(found(vec![kept, waiting]), &[MAIN, INCOMING]).unwrap();
        let moved = cat.moved().unwrap();
        let albums: HashMap<String, String> = cat.albums_moved().unwrap().copies.into_iter().map(|c| (c.id, c.record)).collect();
        let stores: HashMap<String, String> = moved.ids.iter().filter(|(id, _)| id.starts_with(ids::ALBUM)).map(|(a, b)| (a.clone(), b.clone())).collect();
        assert_eq!(stores, albums);
        assert_eq!(moved.ids["tr-4"], "tr-1");
    }

    /// Every file's line, whichever library it lies in, to the track it is on after the merge.
    #[test]
    fn a_line_leads_to_the_track_after_the_merge() {
        let cat = Catalogue::open(None, vec![PathBuf::from("/music"), PathBuf::from("/in")]).unwrap();
        cat.db().execute_batch("INSERT INTO tracks (id, born) VALUES (1, 1), (2, 1);").unwrap();
        let (kept, waiting) = (copy(None, "A", "One", &[(1, "One", 200_000)]), copy(None, "A", "One", &[(2, "One", 200_000)]));
        let (kept, mut waiting) = (of(kept, 0, "pid"), of(waiting, 1, "pid"));
        for (folder, rel, track) in [(0, "A/1.flac", 1), (1, "A/1.mp3", 2), (1, "Gone/1.mp3", 2)] {
            cat.db().execute("INSERT INTO files (folder, rel, size, mtime, track, state) VALUES (?1, ?2, 1, 1, ?3, 1)", params![folder, rel, track]).unwrap();
        }
        cat.db().execute("UPDATE files SET gone_at = 5 WHERE rel = 'Gone/1.mp3'", []).unwrap();
        waiting.files = vec![(1, "A/1.mp3".to_string())];
        cat.first_records(found(vec![kept, waiting]), &[(0, Kind::Main), (1, Kind::Incoming)]).unwrap();
        let lines = cat.moved().unwrap().lines;
        assert_eq!(lines["A/1.flac"], "tr-1");
        assert_eq!(lines["../in/A/1.mp3"], "tr-1", "taken");
        assert_eq!(lines["../in/Gone/1.mp3"], "tr-1", "a file gone a while follows its track");
    }

    #[test]
    fn a_folder_that_shows_nothing_is_named_and_one_with_no_files_is_not() {
        let root = std::env::temp_dir().join(format!("cratarr-first-{}", std::process::id()));
        let (up, down, new) = (root.join("up"), root.join("down"), root.join("new"));
        for folder in [&up, &down, &new] {
            std::fs::create_dir_all(folder).unwrap();
        }
        std::fs::write(up.join("a.flac"), b"x").unwrap();
        let cat = Catalogue::open(None, vec![up.clone(), down.clone(), new]).unwrap();
        cat.db().execute_batch("INSERT INTO files (folder, rel, size, mtime) VALUES (0, 'a.flac', 1, 1), (1, 'b.flac', 1, 1);").unwrap();
        assert_eq!(cat.folders_not_there().unwrap(), [down], "its files are in the catalogue and not there; the third never had any");
        std::fs::remove_dir_all(&root).ok();
    }

    /// A folder that is gone reads as out of use, its rows parked, and its id for life says this open did it. One 0.15 parked has
    /// none, and one with no files has nothing to miss: neither is a reason to stop.
    #[test]
    fn a_library_parked_as_it_is_gone_is_named_and_one_0_15_parked_is_not() {
        let cat = Catalogue::open(None, vec![PathBuf::from("/kept")]).unwrap();
        cat.db().execute_batch("INSERT INTO folders (id, path, home) VALUES (-1, '/gone', 1), (-2, '/left', NULL), (-3, '/nothing', 3);
             INSERT INTO files (folder, rel, size, mtime) VALUES (-1, 'a.flac', 1, 1), (-2, 'b.flac', 1, 1);").unwrap();
        assert_eq!(cat.folders_not_there().unwrap(), [PathBuf::from("/gone")]);
    }

    /// A server says a library's path as it sees it, and a source that cannot be placed says nothing, so the catalogue alone can
    /// tell what this start parked: a folder with an id for life, which no folder parked by 0.15 has.
    #[test]
    fn a_library_a_server_names_by_its_own_path_is_missed_all_the_same() {
        let cat = Catalogue::open(None, vec![PathBuf::from("/kept")]).unwrap();
        cat.db().execute_batch("INSERT INTO folders (id, path, home) VALUES (-1, '/media/x', 1); INSERT INTO files (folder, rel, size, mtime) VALUES (-1, 'a.flac', 1, 1);").unwrap();
        assert_eq!(cat.folders_not_there().unwrap(), [PathBuf::from("/media/x")]);
    }

    /// Where each table's rows lie, by folder and path.
    fn rows_of(cat: &Catalogue, table: &str) -> Vec<(i64, String)> {
        let db = cat.db();
        let mut st = db.prepare(&format!("SELECT folder, rel FROM {table} ORDER BY folder, rel")).unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    }

    /// Three libraries, the shapes and beats of these paths as 0.15 left them.
    fn left_behind(sql: &str) -> Catalogue {
        let cat = Catalogue::open(None, ["/music", "/first", "/second"].map(PathBuf::from).to_vec()).unwrap();
        cat.db().execute_batch(sql).unwrap();
        cat
    }

    /// m8: 0.15's `set_folders` took `analysis` along and left these behind, so `analysis` says a file is done while its waveform,
    /// gating blocks and beats lie at the place its library had, read by nothing. Either way round, as a library can go back.
    #[test]
    fn a_files_shape_and_beats_follow_it_to_the_place_its_library_has() {
        let cat = left_behind(
            "INSERT INTO files (folder, rel, size, mtime) VALUES (2, 'A/1.flac', 1, 1), (1, 'B/1.flac', 1, 1), (1, 'C/1.flac', 1, 1);
             INSERT INTO analysis_shape (folder, rel) VALUES (1, 'A/1.flac'), (1, 'B/1.flac'), (2, 'C/1.flac'), (1, 'Nowhere/1.flac');
             INSERT INTO beat_this (folder, rel, size, mtime, beats, downbeats, at) VALUES (1, 'A/1.flac', 1, 1, x'00', x'00', 1), (2, 'C/1.flac', 1, 1, x'00', x'00', 1);",
        );
        let done = cat.first_records(FirstAlbums::default(), &[MAIN, INCOMING, (2, Kind::Incoming)]).unwrap();
        let at = |folder: i64, rel: &str| (folder, rel.to_string());
        assert_eq!(rows_of(&cat, "analysis_shape"), [at(1, "B/1.flac"), at(1, "C/1.flac"), at(1, "Nowhere/1.flac"), at(2, "A/1.flac")], "what is at its file stays, and what has none has nowhere to go");
        assert_eq!(rows_of(&cat, "beat_this"), [at(1, "C/1.flac"), at(2, "A/1.flac")]);
        assert_eq!((done.shapes_moved, done.shapes_left, done.beats_moved, done.beats_left), (2, 0, 2, 0));
    }

    /// A path two libraries hold says nothing of which file a row is of: it stays and is counted, for a person to read. A file
    /// with a row of its own has lost nothing, and Music's place never moved.
    #[test]
    fn a_shape_on_a_path_two_libraries_hold_stays_and_is_counted() {
        let cat = left_behind(
            "INSERT INTO files (folder, rel, size, mtime) VALUES (1, 'Both/1.flac', 1, 1), (2, 'Both/1.flac', 1, 1), (0, 'Two/1.flac', 1, 1), (2, 'Two/1.flac', 1, 1), (2, 'Own/1.flac', 1, 1), (0, 'Kept/1.flac', 1, 1), (2, 'Kept/1.flac', 1, 1);
             INSERT INTO analysis_shape (folder, rel) VALUES (1, 'Both/1.flac'), (1, 'Two/1.flac'), (1, 'Own/1.flac'), (2, 'Own/1.flac'), (0, 'Kept/1.flac');",
        );
        let before = rows_of(&cat, "analysis_shape");
        let done = cat.first_records(FirstAlbums::default(), &[MAIN, INCOMING, (2, Kind::Incoming)]).unwrap();
        assert_eq!(rows_of(&cat, "analysis_shape"), before, "none of them can say where it goes");
        assert_eq!((done.shapes_moved, done.shapes_left), (0, 2), "one beside a file in the other Incoming, one with two files to choose from");
    }

    /// CR13's other half: what `unserved_rows` reads, and only that.
    #[test]
    fn what_no_list_serves_is_read_by_the_id_its_folder_had() {
        let cat = Catalogue::open(None, vec![PathBuf::from("/music")]).unwrap();
        cat.db()
            .execute_batch(
                "INSERT INTO tracks (id, born) VALUES (1, 1), (2, 1), (3, 1), (4, 1);
                 INSERT INTO folders (id, path, home) VALUES (-1, '/parked', 7), (-2, '/older', NULL);
                 INSERT INTO files (folder, rel, size, mtime, track, state, format) VALUES (0, 'Here.flac', 1, 1, 1, 1, 'FLAC');
                 INSERT INTO files (folder, rel, size, mtime, track, state, format, gone_at) VALUES (0, 'Gone.flac', 1, 1, 2, 1, 'FLAC', 5);
                 INSERT INTO files (folder, rel, size, mtime, track, state, format, trashed) VALUES (0, '.cratarr-trash/t/Trash.flac', 1, 1, 3, 1, 'FLAC', 't');
                 INSERT INTO files (folder, rel, size, mtime, track, state, format) VALUES (-1, 'Parked.flac', 1, 1, 4, 1, 'FLAC');
                 INSERT INTO files (folder, rel, size, mtime, state, format) VALUES (0, 'NoTrack.flac', 1, 1, 1, 'FLAC');",
            )
            .unwrap();
        let read: Vec<(i64, i64, Vec<String>)> = cat.unserved_rows().unwrap().into_iter().map(|(id, home, rows)| (id, home, rows.into_iter().map(|r| r.rel).collect())).collect();
        assert_eq!(read, [(-2, -2, vec![]), (-1, 7, vec!["Parked.flac".to_string()]), (0, 0, vec![".cratarr-trash/t/Trash.flac".to_string(), "Gone.flac".to_string()])]);
    }
}
