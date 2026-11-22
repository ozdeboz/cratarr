//! A record's copies as the holding reads them (SCENARIOS-DC1 HD2, HD8; SCENARIOS-PAIR01 .18-.20): the one reader of a
//! record's copies by the present rule, for `holding()` (rules) to read.
//!
//! - **A copy** is one catalogue folder's present files of the record's live tracks (A3.1: a file gone, trashed or in a
//!   parked library is none). `present` counts its tracks with such a file.
//! - **Paired** (HD2, PAIR01.18): each copy alone, its files against each list that judges, by `assign` (rules) with the one
//!   title rule (the domain's, X3) and the one length rule; never two copies' files in one pairing (HD3). Which lists judge is
//!   the domain's `judging` (VR12 rule 1): the administrator's pick alone (VR14); else every kept list that is no guess (VR8; a
//!   guess today's pairing confirms, a copy in a main library or own music whose files and its list pair one for one, is none:
//!   RV-22, C15), and of those the timed alone while one is (VR21, RV-21). A copy whole by any of them is whole (OQ-VR-1 (b));
//!   otherwise its pairs are the most it makes with any of the shortest length (VR13 (c): no tie broken by a source's name).
//!   With no list kept, `paired` is `present` (OQ-VR-6 (c)); with only guesses kept, not known whole (VR12 rule 6). A file's
//!   length is the one the library serves: its header's, or a whole decode's where the two differ by more than 1.5 s
//!   (`served_rows`).
//! - **Went** (HD8, HD9): a folder whose files all went (gone, or into a trash) is a copy of none present, with when the last
//!   went and from where, as `went_of` reads it. A parked library's copy is none and gives none: while its files lie in a
//!   parked library, the record is not retired (they come back with it).
//! - **A record whose tracks have no file at all** (as tests make them) is one whole main copy, as the had rule's clause has it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::Result;
use cratarr_domain::facts::{List, confirms, judging, only_guesses, pairs_with};
use cratarr_rules::holding::{CopyFacts, Kept, Library, Went};
use rusqlite::{Connection, OptionalExtension, params};

use super::Catalogue;

/// A record's copies, and the length of the shortest list that judges them (none where none does); `only_guesses` where the only
/// lists it keeps are guesses' (VR12 rule 6: not known whole); `picked` where an administrator's pick judges them (VR14): only
/// then does a had record show a count (OQ-VR-3 (b), HD12).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Copies {
    pub copies: Vec<CopyFacts>,
    pub shortest: Option<u16>,
    pub only_guesses: bool,
    pub picked: bool,
    /// The guesses' references today's pairing confirms (RV-22): references now, worked out at this read, never stored.
    pub confirmed: Vec<cratarr_domain::facts::Reference>,
}

impl Copies {
    /// What judges them whole, as the holding reads it (`holding_of`).
    pub fn kept(&self) -> Kept {
        if self.only_guesses { Kept::OnlyGuesses } else { Kept::of(self.shortest) }
    }
}

/// How far a file's header may be from a whole decode's length before the decode's is the one served (`served_rows`).
const DECODE_SLACK_MS: i64 = 1_500;

/// The catalogue folder a record of tracks with no file at all counts as: the first main library's.
const FILELESS: i64 = 0;

fn count(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// One present file of a copy, as `assign` reads it: the domain's file (its place, key, title and served length), and its track.
struct Present {
    track: i64,
    file: cratarr_domain::facts::Present,
}

/// A copy's files as the domain's pairing reads them.
fn sides(files: &[Present]) -> Vec<cratarr_domain::facts::Present> {
    files.iter().map(|f| f.file.clone()).collect()
}

/// The length the library serves (`served_rows`): a whole decode's where the header says otherwise by more than 1.5 s.
fn served_ms(header: Option<i64>, decoded_secs: Option<f64>, complete: Option<bool>) -> Option<i64> {
    let header = header.filter(|ms| *ms > 0);
    let decoded = decoded_secs.filter(|s| *s > 0.0 && complete == Some(true)).map(|s| (s * 1000.0).round() as i64);
    match decoded {
        Some(d) if header.is_none_or(|h| (h - d).abs() > DECODE_SLACK_MS) => Some(d),
        _ => header,
    }
}

/// `Catalogue::copies_of` on the caller's connection: each record's own copies, paired with the lists it keeps.
pub fn copies_in(db: &Connection, records: &[i64], library: &dyn Fn(i64) -> Library) -> Result<HashMap<i64, Copies>> {
    held_in(db, &records.iter().map(|&r| (r, r)).collect::<Vec<_>>(), library)
}

/// A record's present files by catalogue folder, each as the one pairing reads it (`facts::Present`: the tag rule's title, else its
/// name's, the length the library serves, its told place while it is as placed), with its track. The holding's and the arrival
/// row's one reader of a copy (F53.10: never two pairings of one copy).
fn present_in(db: &Connection, holder: i64) -> Result<BTreeMap<i64, Vec<Present>>> {
    let mut files_of = db.prepare_cached(&format!(
        "SELECT f.folder, f.rel, f.track, f.duration_ms, a.duration_secs, a.complete, f.size, f.mtime FROM files f JOIN tracks t ON t.id = f.track
           LEFT JOIN analysis a ON a.folder = f.folder AND a.rel = f.rel AND a.size = f.size AND a.mtime = f.mtime
         WHERE t.record = ?1 AND t.retired_at IS NULL AND {} ORDER BY f.folder, f.rel",
        super::PRESENT_F
    ))?;
    let mut tags = db.prepare_cached("SELECT name, value FROM tags WHERE folder = ?1 AND rel = ?2 AND name IS NOT NULL ORDER BY n")?;
    let mut folders: BTreeMap<i64, Vec<Present>> = BTreeMap::new();
    let rows = files_of.query_map([holder], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, Option<i64>>(3)?,
            r.get::<_, Option<f64>>(4)?,
            r.get::<_, Option<bool>>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
        ))
    })?;
    // The files Cratarr placed for it (COU.18): each pairs at its told place while it is as placed (WM.25 (e)).
    let told = super::provenance::told_files_in(db, holder)?;
    for row in rows {
        let (folder, rel, track, header, decoded, complete, size, mtime) = row?;
        let read: Vec<(String, String)> = tags.query_map(params![folder, rel], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let facts = crate::incoming::TrackFacts::of(&crate::naming::Tags(read), &rel, None);
        let ms = served_ms(header, decoded, complete);
        let mut file = facts.side(rel, ms);
        file.told = told.get(&(folder, file.key.clone())).and_then(|t| super::provenance::as_placed(t, size, mtime));
        folders.entry(folder).or_default().push(Present { track, file });
    }
    Ok(folders)
}

/// `Catalogue::copy_files` on the caller's connection.
pub fn copy_files_in(db: &Connection, record: i64) -> Result<BTreeMap<i64, Vec<cratarr_domain::facts::Present>>> {
    Ok(present_in(db, record)?.into_iter().map(|(folder, files)| (folder, sides(&files))).collect())
}

/// `Catalogue::copies_held` on the caller's connection: one read where the caller holds a transaction (HD14).
pub fn held_in(db: &Connection, asked: &[(i64, i64)], library: &dyn Fn(i64) -> Library) -> Result<HashMap<i64, Copies>> {
    let mut out = HashMap::new();
    let holders: Vec<i64> = asked.iter().map(|(_, holder)| *holder).collect();
    let went = super::records::went_in(db, &holders)?;
    let mut parked_of = db.prepare_cached(
        "SELECT EXISTS (SELECT 1 FROM files f JOIN tracks t ON t.id = f.track
           WHERE t.record = ?1 AND t.retired_at IS NULL AND f.folder < 0 AND f.gone_at IS NULL AND f.trashed IS NULL)",
    )?;
    let mut fileless_of =
        db.prepare_cached("SELECT count(*) FROM tracks t WHERE t.record = ?1 AND t.retired_at IS NULL AND NOT EXISTS (SELECT 1 FROM files f WHERE f.track = t.id)")?;
    for &(record, holder) in asked {
        // The record's own lists and pick; where it keeps no list, its holder's (a record held by another's files: R13), a guess
        // left out there as here (VR46).
        let (mut lists, mut pick) = (super::lists::lists_in(db, record)?, super::lists::pick_in(db, record)?);
        if lists.iter().all(|l| l.tracks.is_empty()) && holder != record {
            (lists, pick) = (super::lists::lists_in(db, holder)?, super::lists::pick_in(db, holder)?);
        }
        let folders = present_in(db, holder)?;
        // A guess today's pairing confirms is none (RV-22): a copy in a main library or own music, never an Incoming arrival not
        // yet added (C15), whose files and the list pair one for one, both ways. Worked out at each read, never stored.
        let sure = |folder: i64| matches!(library(folder), Library::Main | Library::Own(_));
        let mut confirmed = Vec::new();
        for list in lists.iter_mut().filter(|l| l.guessed) {
            if folders.iter().any(|(folder, files)| sure(*folder) && confirms(&list.tracks, &sides(files))) {
                list.guessed = false;
                confirmed.push(list.reference.clone());
            }
        }
        let judges: Vec<&List> = judging(&lists, pick.as_ref());
        let shortest = judges.iter().map(|l| count(l.tracks.len())).min();
        let guesses = only_guesses(&lists, pick.as_ref());
        let mut copies = Vec::new();
        for (folder, files) in &folders {
            let present = files.iter().map(|f| f.track).collect::<BTreeSet<i64>>().len();
            let files = sides(files);
            let paired = match shortest {
                None => present,
                Some(of) if judges.iter().any(|l| pairs_with(&l.tracks, &files).len() == l.tracks.len()) => usize::from(of),
                Some(of) => judges.iter().filter(|l| count(l.tracks.len()) == of).map(|l| pairs_with(&l.tracks, &files).len()).max().unwrap_or(0),
            };
            copies.push(CopyFacts { folder: *folder, library: library(*folder), present: count(present), paired: count(paired), went: None });
        }
        // A folder whose files all went: a copy of none present, with when its last went. None while files lie parked.
        let parked: bool = parked_of.query_row([holder], |r| r.get(0))?;
        if !parked {
            let mut gone: BTreeMap<i64, i64> = BTreeMap::new();
            for w in went.get(&holder).into_iter().flatten().filter(|w| w.folder >= 0 && !folders.contains_key(&w.folder)) {
                if let Some(at) = w.gone.into_iter().chain(w.trashed).max() {
                    let latest = gone.entry(w.folder).or_insert(at);
                    *latest = (*latest).max(at);
                }
            }
            for (folder, at) in gone {
                copies.push(CopyFacts { folder, library: library(folder), present: 0, paired: 0, went: Some(Went { at, from: folder }) });
            }
        }
        if folders.is_empty() {
            let fileless: i64 = fileless_of.query_row([holder], |r| r.get(0))?;
            if fileless > 0 {
                let n = count(usize::try_from(fileless).unwrap_or(usize::MAX));
                copies.push(CopyFacts { folder: FILELESS, library: Library::Main, present: n, paired: n, went: None });
            }
        }
        let picked = pick.as_ref().is_some_and(|p| judges.iter().any(|l| &l.reference == p));
        out.insert(record, Copies { copies, shortest, only_guesses: guesses, picked, confirmed });
    }
    Ok(out)
}

impl Catalogue {
    /// Each record's copies by the present rule, paired with the lists it keeps, and its shortest list, each catalogue folder's
    /// library as `library` says. On a reader, in one read transaction (HD14). Blocking.
    pub fn copies_of(&self, records: &[i64], library: &dyn Fn(i64) -> Library) -> Result<HashMap<i64, Copies>> {
        self.copies_held(&records.iter().map(|&r| (r, r)).collect::<Vec<_>>(), library)
    }

    /// What requests read of their records (`requested`) and the copies their holders hold (`copies_held`), on one reader in one
    /// read transaction (R16: the Albums page asks for every open request while builds run; HD14). By the asked record. Blocking.
    pub fn requested_held(
        &self,
        records: &[i64],
        library: &dyn Fn(i64) -> Library,
    ) -> Result<(HashMap<i64, super::records::Requested>, HashMap<i64, Copies>)> {
        let read = |c: &mut Connection| -> Result<(HashMap<i64, super::records::Requested>, HashMap<i64, Copies>)> {
            let tx = c.transaction()?;
            let read = super::records::requested_in(&tx, records)?;
            let asked: Vec<(i64, i64)> = read.values().filter_map(|q| Some((q.row.id, q.holder?))).collect();
            let copies = held_in(&tx, &asked, library)?;
            tx.commit()?;
            Ok((read, copies))
        };
        match self.reader() {
            Some(mut c) => read(&mut c),
            None => read(&mut self.db()),
        }
    }

    /// As `copies_of`, for records asked as (the record, its holder): the holder's copies (the record whose files they are, R13),
    /// paired with the asked record's lists, else its holder's. By the asked record. Blocking.
    pub fn copies_held(&self, asked: &[(i64, i64)], library: &dyn Fn(i64) -> Library) -> Result<HashMap<i64, Copies>> {
        let read = |c: &mut Connection| -> Result<HashMap<i64, Copies>> {
            let tx = c.transaction()?;
            let out = held_in(&tx, asked, library)?;
            tx.commit()?;
            Ok(out)
        };
        match self.reader() {
            Some(mut c) => read(&mut c),
            None => read(&mut self.db()),
        }
    }

    /// The records with present files in any of these catalogue folders (an Incoming library's arrivals, F53.7's job). On a reader.
    /// Blocking.
    pub fn records_held_in(&self, folders: &[i64]) -> Result<Vec<i64>> {
        let read = |c: &Connection| -> Result<Vec<i64>> {
            let mut st = c.prepare_cached(&format!(
                "SELECT DISTINCT t.record FROM files f JOIN tracks t ON t.id = f.track WHERE f.folder = ?1 AND t.retired_at IS NULL AND {} ORDER BY t.record",
                super::PRESENT_F
            ))?;
            let mut out = Vec::new();
            for folder in folders {
                out.extend(st.query_map([folder], |r| r.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<i64>>>()?);
            }
            out.sort_unstable();
            out.dedup();
            Ok(out)
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// A record's present files by catalogue folder, as the one pairing reads them (the holding's reader): what an arrival's row
    /// pairs its copy by (F53.10). On a reader. Blocking.
    pub fn copy_files(&self, record: i64) -> Result<BTreeMap<i64, Vec<cratarr_domain::facts::Present>>> {
        match self.reader() {
            Some(c) => copy_files_in(&c, record),
            None => copy_files_in(&self.db(), record),
        }
    }

    /// What a record's views read of it (SCENARIOS-DC1 HD14: one read for one view), on one reader in one read transaction: the
    /// record it is now (a merged one's end, RV3), its credits and labels, its copies by the present rule with their quality, and
    /// its Bandcamp page. None: no such record, or one forgotten (RV4). Blocking.
    pub fn record_read(&self, id: i64, library: &dyn Fn(i64) -> Library) -> Result<Option<RecordRead>> {
        let read = |c: &mut Connection| -> Result<Option<RecordRead>> {
            let tx = c.transaction()?;
            let out = record_read_in(&tx, id, library)?;
            tx.commit()?;
            Ok(out)
        };
        match self.reader() {
            Some(mut c) => read(&mut c),
            None => read(&mut self.db()),
        }
    }
}

/// A copy's quality as its present files give it: one format for them all (none where they differ), the lowest bitrate, and when
/// its first file was written (the earliest file time), which of a record only in people's own music says whose came first (CN5).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Quality {
    pub format: Option<String>,
    pub min_kbps: Option<i64>,
    pub added: Option<i64>,
}

/// `Catalogue::record_read`'s answer.
#[derive(Debug, Clone)]
pub struct RecordRead {
    pub row: super::records::RecordRow,
    /// Its credited artists in credit order: their ids and names; the phrase its credit prints after each (CRED.27), and what it prints
    /// beside them (its own featuring, a placeholder's whole line: `record_credit_extra`).
    pub artists: Vec<(i64, String)>,
    pub phrases: Vec<Option<String>>,
    pub extras: Vec<super::records::Extra>,
    /// What its credit is by, as the record pass decided it (`files`, each source agreeing, or a source's own, NAMES.20 row 2).
    pub credit_by: Vec<String>,
    /// Its labels: each label's id, name and catalogue number here ('' none), in the order given.
    pub labels: Vec<(i64, String, String)>,
    pub copies: Copies,
    /// Each present copy's quality, by its catalogue folder.
    pub quality: HashMap<i64, Quality>,
    /// Its Bandcamp album page, where a reference names one.
    pub bandcamp: Option<String>,
    /// Every list it keeps, by reference, each a guess's or not (VR01), and the administrator's pick among them (VR5).
    pub lists: Vec<cratarr_domain::facts::List>,
    pub pick: Option<cratarr_domain::facts::Reference>,
    /// Every source's value of its facts, by fact, source and reference (FA15, VR40), and the references it holds that are guesses
    /// (VR8: a search by name alone, unconfirmed, and today's pairing confirming none), for `resolve()` (VR39).
    pub said: Vec<super::facts::Said>,
    pub guesses: Vec<cratarr_domain::facts::Reference>,
}

impl RecordRead {
    /// Each source's value of one of its facts, per reference (`kind:value`, none for a fact no reference gives), each a guess's where
    /// its reference is one (VR8): what `resolve()` reads (VR39). A value that does not read as one is none.
    pub fn said_of<T: std::str::FromStr>(&self, fact: &str) -> Vec<cratarr_domain::facts::Said<T>> {
        self.said
            .iter()
            .filter(|s| s.fact == fact)
            .map(|s| {
                let reference = s.of.split_once(':').map(|(kind, value)| cratarr_domain::facts::Reference::new(&s.source, kind, value));
                let guessed = reference.as_ref().is_some_and(|r| self.guesses.contains(r));
                cratarr_domain::facts::Said { reference, source: s.source.clone(), value: s.value.as_deref().and_then(|v| v.parse().ok()), guessed }
            })
            .collect()
    }

    /// Its year by the one rule (`facts::record_year`): a had record's (a copy present) the one it was made with, else its sources'.
    pub fn year(&self) -> cratarr_domain::facts::Resolved<i32> {
        let had = self.copies.copies.iter().any(|c| c.present > 0);
        cratarr_domain::facts::record_year(None, had, self.row.year, &self.said_of("year"), self.pick.as_ref())
    }
}

/// `Catalogue::record_read` in the caller's transaction.
pub fn record_read_in(tx: &Connection, id: i64, library: &dyn Fn(i64) -> Library) -> Result<Option<RecordRead>> {
    let Some(end) = super::records::merged_end(tx, id)? else { return Ok(None) };
    let Some(row) = super::records::record_in(tx, end)? else { return Ok(None) };
    let credited: Vec<(i64, String, Option<String>)> = tx
        .prepare_cached("SELECT a.id, a.name, r.phrase FROM record_artists r JOIN artists a ON a.id = r.artist WHERE r.record = ?1 ORDER BY r.position, a.id")?
        .query_map([end], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let phrases: Vec<Option<String>> = credited.iter().map(|(_, _, p)| p.clone()).collect();
    let artists: Vec<(i64, String)> = credited.into_iter().map(|(id, name, _)| (id, name)).collect();
    let extras = super::records::extras_in(tx, end)?;
    let credit_by: Vec<String> = tx
        .prepare_cached("SELECT credit_by FROM records WHERE id = ?1")?
        .query_row([end], |r| r.get::<_, Option<String>>(0))
        .optional()?
        .flatten()
        .map(|b| b.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
    let labels = tx
        .prepare_cached("SELECT l.id, l.name, r.catno FROM record_labels r JOIN labels l ON l.id = r.label WHERE r.record = ?1 ORDER BY l.id, r.catno")?
        .query_map([end], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let copies = held_in(tx, &[(end, end)], library)?.remove(&end).unwrap_or_default();
    let mut quality = HashMap::new();
    let mut st = tx.prepare_cached(&format!(
        "SELECT f.folder, CASE WHEN count(DISTINCT COALESCE(f.format, '')) = 1 THEN max(f.format) END, min(NULLIF(f.bitrate, 0)), min(f.mtime)
           FROM files f JOIN tracks t ON t.id = f.track WHERE t.record = ?1 AND t.retired_at IS NULL AND {} GROUP BY f.folder",
        super::PRESENT_F
    ))?;
    for q in st.query_map([end], |r| Ok((r.get::<_, i64>(0)?, Quality { format: r.get(1)?, min_kbps: r.get(2)?, added: r.get(3)? })))? {
        let (folder, q) = q?;
        quality.insert(folder, q);
    }
    let bandcamp: Option<String> = tx
        .prepare_cached("SELECT value FROM \"references\" WHERE subject = ?1 AND source = 'bandcamp' AND kind = 'album' ORDER BY value LIMIT 1")?
        .query_row([crate::serve::ids::record(end)], |r| r.get(0))
        .optional()?;
    let (lists, pick) = (super::lists::lists_in(tx, end)?, super::lists::pick_in(tx, end)?);
    let said = tx
        .prepare_cached("SELECT fact, source, ref, value, at FROM facts WHERE record = ?1 ORDER BY fact, source, ref")?
        .query_map([end], |r| Ok(super::facts::Said { fact: r.get(0)?, source: r.get(1)?, of: r.get(2)?, value: r.get(3)?, at: r.get(4)? }))?
        .collect::<rusqlite::Result<_>>()?;
    let guesses = tx
        .prepare_cached("SELECT source, kind, value FROM \"references\" WHERE subject = ?1 AND how = 'search' AND confirmed = 0")?
        .query_map([crate::serve::ids::record(end)], |r| Ok(cratarr_domain::facts::Reference::new(&r.get::<_, String>(0)?, &r.get::<_, String>(1)?, &r.get::<_, String>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|r| !copies.confirmed.contains(r))
        .collect();
    Ok(Some(RecordRead { row, artists, phrases, extras, credit_by, labels, copies, quality, bandcamp, lists, pick, said, guesses }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cratarr_rules::holding::{Fetcher, Holding, On, Went, holding};
    use rusqlite::params;

    // SCENARIOS-DC1's libraries, by their catalogue folders; a parked library's is negative.
    const MUSIC: i64 = 0;
    const IN: i64 = 1;
    const OWN_A: i64 = 2;
    const MORE: i64 = 4;
    const PARKED: i64 = -1;
    /// 2026-03-03 12:00 UTC.
    const T: i64 = 1_772_539_200;

    fn library(folder: i64) -> Library {
        match folder {
            IN => Library::Incoming { fetcher: Fetcher::Lidarr, uploader: None },
            OWN_A => Library::Own("alice".to_string()),
            _ => Library::Main,
        }
    }

    fn catalogue() -> Catalogue {
        Catalogue::open(None, Vec::new()).unwrap()
    }

    fn record(cat: &Catalogue, title: &str) -> i64 {
        let db = cat.db();
        db.execute("INSERT INTO records (born, title, artist) VALUES (1, ?1, 'Glass Harbour')", [title]).unwrap();
        db.last_insert_rowid()
    }

    /// A list kept for `record`: (disc, place, title, length in ms).
    fn list(cat: &Catalogue, record: i64, source: &str, tracks: &[(u32, u32, &str, Option<i64>)]) {
        let tracks: Vec<serde_json::Value> = tracks
            .iter()
            .map(|(disc, position, title, ms)| serde_json::json!({ "disc": disc, "position": position, "title": title, "length_ms": ms }))
            .collect();
        cat.db()
            .execute("INSERT INTO reference_tracks (record, source, tracks, at) VALUES (?1, ?2, ?3, 1)", params![record, source, serde_json::Value::Array(tracks).to_string()])
            .unwrap();
    }

    fn track(cat: &Catalogue, record: i64, number: u32) -> i64 {
        let db = cat.db();
        db.execute("INSERT INTO tracks (born, record, disc, number) VALUES (1, ?1, 1, ?2)", params![record, number]).unwrap();
        db.last_insert_rowid()
    }

    /// A file of `track` in `folder`, tagged with its title and number, its header's length `ms`.
    fn file(cat: &Catalogue, folder: i64, rel: &str, track: i64, number: u32, title: &str, ms: i64) {
        let db = cat.db();
        db.execute("INSERT INTO files (folder, rel, size, mtime, track, duration_ms) VALUES (?1, ?2, 1, 1, ?3, ?4)", params![folder, rel, track, ms]).unwrap();
        for (n, (name, value)) in [("title", title.to_string()), ("tracknumber", number.to_string())].into_iter().enumerate() {
            db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (?1, ?2, ?3, ?4, ?4, ?5)", params![folder, rel, n as i64, name, value]).unwrap();
        }
    }

    /// Night Ferry's ten (N1-N10), as MusicBrainz lists them.
    fn night_ferry() -> Vec<(u32, u32, String, Option<i64>)> {
        (1..=10).map(|n| (1, n, format!("Night Ferry {n}"), Some(200_000 + i64::from(n) * 7_000))).collect()
    }

    fn keep(cat: &Catalogue, record: i64, tracks: &[(u32, u32, String, Option<i64>)]) {
        let rows: Vec<(u32, u32, &str, Option<i64>)> = tracks.iter().map(|(d, p, t, ms)| (*d, *p, t.as_str(), *ms)).collect();
        list(cat, record, "musicbrainz", &rows);
    }

    /// A copy in `folder` of these list tracks (by place), a file for each, its length the list's.
    fn copy_of(cat: &Catalogue, record: i64, folder: i64, listed: &[(u32, u32, String, Option<i64>)], places: impl IntoIterator<Item = u32>) {
        for n in places {
            let (_, _, title, ms) = &listed[n as usize - 1];
            let t = track(cat, record, n);
            file(cat, folder, &format!("r{record}/{folder}/{n:02}.flac"), t, n, title, ms.unwrap());
        }
    }

    fn of(cat: &Catalogue, record: i64) -> Copies {
        cat.copies_of(&[record], &library).unwrap().remove(&record).unwrap_or_default()
    }

    fn held(c: &Copies) -> Holding {
        holding(&c.copies, c.shortest, "alice", &On::all())
    }

    /// HD2's store half (PAIR01.18): each copy paired with the shortest list kept, by `assign` and the one title rule. Red
    /// first against today's count (`holding_in`'s copies, `holding_of`'s `n >= l`): ten files of which eight are the list's read
    /// whole (logs/test-204010).
    #[test]
    fn a_copy_is_paired_with_the_shortest_list() {
        let cat = catalogue();
        let listed = night_ferry();
        // A longer list kept beside it (Discogs' twelve): the shortest is the ten.
        let mut twelve = listed.clone();
        twelve.extend([(1, 11, "Breakwater".to_string(), Some(300_000)), (1, 12, "Tide".to_string(), Some(310_000))]);
        let with_lists = |title: &str| {
            let r = record(&cat, title);
            keep(&cat, r, &listed);
            let rows: Vec<(u32, u32, &str, Option<i64>)> = twelve.iter().map(|(d, p, t, ms)| (*d, *p, t.as_str(), *ms)).collect();
            list(&cat, r, "discogs", &rows);
            r
        };
        // MUSIC with N1-N10.
        let whole = with_lists("Whole");
        copy_of(&cat, whole, MUSIC, &listed, 1..=10);
        // MUSIC with N1-N8 and two tracks no list has, of other titles and lengths (ten files).
        let short = with_lists("Short");
        copy_of(&cat, short, MUSIC, &listed, 1..=8);
        for (n, title, ms) in [(9, "Harbour Lights", 95_000), (10, "Low Water", 99_000)] {
            let t = track(&cat, short, n);
            file(&cat, MUSIC, &format!("r{short}/0/{n:02}.flac"), t, n, title, ms);
        }
        // MUSIC with twelve files, N1-N10 among them.
        let more = with_lists("Twelve");
        copy_of(&cat, more, MUSIC, &listed, 1..=10);
        for (n, title, ms) in [(11, "Bonus One", 100_000), (12, "Bonus Two", 110_000)] {
            let t = track(&cat, more, n);
            file(&cat, MUSIC, &format!("r{more}/0/{n:02}.flac"), t, n, title, ms);
        }
        // A record that keeps no list, its copy of nine files.
        let unlisted = record(&cat, "Unlisted");
        copy_of(&cat, unlisted, MUSIC, &listed, 1..=9);
        let (w, s, m, u) = (of(&cat, whole), of(&cat, short), of(&cat, more), of(&cat, unlisted));
        let pairs = |c: &Copies| c.copies.iter().map(|x| (x.present, x.paired)).collect::<Vec<_>>();
        assert_eq!((w.shortest, pairs(&w)), (Some(10), vec![(10, 10)]));
        assert_eq!((s.shortest, pairs(&s)), (Some(10), vec![(10, 8)]), "never paired by its count of ten files");
        assert_eq!((m.shortest, pairs(&m)), (Some(10), vec![(12, 10)]));
        assert_eq!((u.shortest, pairs(&u)), (None, vec![(9, 9)]), "no list: paired is present (OQ-VR-6 (c))");
        assert_eq!([held(&w), held(&s), held(&m), held(&u)], [Holding::Had, Holding::Partly { has: 8, of: 10 }, Holding::Had, Holding::Had]);
    }

    /// PAIR01.18's HD3 row: two copies at once, each paired alone. Red first: today's reading counts present tracks across copies
    /// for Partly's `has` (`holding_of`), and the counted list is MusicBrainz's whatever is shorter.
    #[test]
    fn each_copy_is_paired_alone() {
        let cat = catalogue();
        let listed = night_ferry();
        let r = record(&cat, "Halves");
        keep(&cat, r, &listed);
        // MUSIC holds N1-N6, IN holds N5-N10: N5 and N6 are one track each, a file in both.
        let mut tracks = HashMap::new();
        for n in 1..=10u32 {
            tracks.insert(n, track(&cat, r, n));
        }
        for (folder, places) in [(MUSIC, 1..=6u32), (IN, 5..=10u32)] {
            for n in places {
                let (_, _, title, ms) = &listed[n as usize - 1];
                file(&cat, folder, &format!("r{r}/{folder}/{n:02}.flac"), tracks[&n], n, title, ms.unwrap());
            }
        }
        let c = of(&cat, r);
        assert_eq!(c.copies.iter().map(|x| (x.folder, x.present, x.paired)).collect::<Vec<_>>(), [(MUSIC, 6, 6), (IN, 6, 6)]);
        assert_eq!(held(&c), Holding::Partly { has: 6, of: 10 }, "never Partly 10 of 10 nor Had from two halves");
    }

    /// HD8 (A3.1): present copies only. Red first: today's reading keeps no `went` per copy (the retired record says "Was in
    /// Music until" its pass's day, `retired_at`).
    #[test]
    fn copies_are_present_files_alone() {
        let cat = catalogue();
        let listed = night_ferry();
        let made = |title: &str, folder: i64| {
            let r = record(&cat, title);
            keep(&cat, r, &listed);
            copy_of(&cat, r, folder, &listed, 1..=10);
            r
        };
        // A copy in MUSIC whose files are all gone at t (its tracks within their 24 h).
        let gone = made("Gone", MUSIC);
        cat.db().execute("UPDATE files SET gone_at = ?2 WHERE track IN (SELECT id FROM tracks WHERE record = ?1)", params![gone, T]).unwrap();
        // A copy moved to the trash at t, in MORE.
        let trashed = made("Trashed", MORE);
        cat.db().execute("UPDATE files SET trashed = ?2 WHERE track IN (SELECT id FROM tracks WHERE record = ?1)", params![trashed, format!("{T}/a1")]).unwrap();
        // A copy in a parked library.
        let parked = made("Parked", PARKED);
        // A record whose tracks have no file at all, as tests make them.
        let fileless = record(&cat, "Fileless");
        for n in 1..=10 {
            track(&cat, fileless, n);
        }
        let (g, t, p, f) = (of(&cat, gone), of(&cat, trashed), of(&cat, parked), of(&cat, fileless));
        let none = |folder: i64| CopyFacts { folder, library: library(folder), present: 0, paired: 0, went: Some(Went { at: T, from: folder }) };
        assert_eq!(g.copies, [none(MUSIC)], "the gone copy is no copy; it went at t from MUSIC");
        assert_eq!(held(&g), Holding::Retired { since: T, from: MUSIC });
        assert_eq!(t.copies, [none(MORE)], "the trashed copy likewise, from MORE");
        assert_eq!(held(&t), Holding::Retired { since: T, from: MORE });
        assert!(p.copies.is_empty(), "the parked copy is no copy and no went: {p:?}");
        assert_eq!(held(&p), Holding::NotHad, "its files come back with the library");
        assert_eq!(f.copies, [CopyFacts { folder: MUSIC, library: Library::Main, present: 10, paired: 10, went: None }], "one whole Main copy");
        assert_eq!(held(&f), Holding::Had);
        // Never a gone, parked or trashed file counted as present.
        assert!([&g, &t, &p].iter().all(|c| c.copies.iter().all(|x| x.present == 0)));
    }

    /// PAIR01.20: production's real cases, their facts as audit/SAME-RECORD.md has them (the other tracks' titles and lengths
    /// are stand-ins). Red first: today's count reads each copy whole.
    #[test]
    fn the_real_cases_pair_as_the_owner_picks() {
        let cat = catalogue();
        let copy_with = |r: i64, folder: i64, files: &[(u32, String, i64)]| {
            for (n, title, ms) in files {
                let t = track(&cat, r, *n);
                file(&cat, folder, &format!("r{r}/{folder}/{n:02}.mp3"), t, *n, title, *ms);
            }
        };
        let stand_in = |n: u32| (n, format!("Track {n}"), 180_000 + i64::from(n) * 11_000);
        // Agoria, "Drift": ten tracks, four titled "Embrace (feat. Phoebe Killdeer)" in one copy against "Embrace"; ten
        // lengths agree. The list says "Embrace".
        let drift: Vec<(u32, String, i64)> = (1..=10).map(|n| if n <= 4 { (n, format!("Embrace {n}"), 240_000 + i64::from(n) * 1_000) } else { stand_in(n) }).collect();
        let agoria = |folder: i64, guest: bool| {
            let r = record(&cat, &format!("Drift {folder}"));
            keep(&cat, r, &drift.iter().map(|(n, t, ms)| (1, *n, t.clone(), Some(*ms))).collect::<Vec<_>>());
            let files: Vec<(u32, String, i64)> =
                drift.iter().map(|(n, t, ms)| (*n, if guest && *n <= 4 { format!("{t} (feat. Phoebe Killdeer)") } else { t.clone() }, *ms)).collect();
            copy_with(r, folder, &files);
            r
        };
        let (music, incoming) = (agoria(MUSIC, true), agoria(IN, false));
        assert_eq!(held(&of(&cat, music)), Holding::Had, "the guest folds: whole");
        assert_eq!(of(&cat, incoming).copies[0].paired, 10);
        // Mary Lattimore, "Hundreds Of Days": seven tracks. The MP3's track 7 says "(Bonus Track)"; its track 2's header and
        // the FLAC say 450.4 s, its whole decode 498.8 s, which is the length served.
        let days: Vec<(u32, String, i64)> = (1..=7).map(|n| if n == 2 { (2, "Track 2".to_string(), 450_400) } else { stand_in(n) }).collect();
        let lattimore = |folder: i64, mp3: bool| {
            let r = record(&cat, &format!("Hundreds Of Days {folder}"));
            keep(&cat, r, &days.iter().map(|(n, t, ms)| (1, *n, t.clone(), Some(*ms))).collect::<Vec<_>>());
            let files: Vec<(u32, String, i64)> = days.iter().map(|(n, t, ms)| (*n, if mp3 && *n == 7 { format!("{t} (Bonus Track)") } else { t.clone() }, *ms)).collect();
            copy_with(r, folder, &files);
            if mp3 {
                cat.db()
                    .execute(
                        "INSERT INTO analysis (folder, rel, size, mtime, version, duration_secs, analysed_at, complete) VALUES (?1, ?2, 1, 1, 1, 498.8, 1, 1)",
                        params![folder, format!("r{r}/{folder}/02.mp3")],
                    )
                    .unwrap();
            }
            r
        };
        let (mp3, flac) = (lattimore(MUSIC, true), lattimore(IN, false));
        assert_eq!(held(&of(&cat, mp3)), Holding::Partly { has: 6, of: 7 }, "48.4 s off by its decode: not held (OQ-VR-7 (b)); track 7 pairs (OQ-PAIR01-1 (a))");
        assert_eq!(of(&cat, flac).copies[0].paired, 7);
        // Felix Laband, "4/4 Down The Stairs": eighteen tracks; the FLAC's track 18 is exactly 120.0 s, the list's 637.8 s.
        let stairs: Vec<(u32, String, i64)> = (1..=18).map(|n| if n == 18 { (18, "Track 18".to_string(), 637_800) } else { stand_in(n) }).collect();
        let r = record(&cat, "4/4 Down The Stairs");
        keep(&cat, r, &stairs.iter().map(|(n, t, ms)| (1, *n, t.clone(), Some(*ms))).collect::<Vec<_>>());
        copy_with(r, MUSIC, &stairs.iter().map(|(n, t, ms)| (*n, t.clone(), if *n == 18 { 120_000 } else { *ms })).collect::<Vec<_>>());
        assert_eq!(held(&of(&cat, r)), Holding::Partly { has: 17, of: 18 });
    }

    /// PAIR01.19 (as amended 2026-11-21: VR01's C13, RECHECK-DC1-VR01's N3): Replace's carry-over of play history stays
    /// `incoming::pair`'s, never `assign`'s; the join, whose pairs move play history too (`taken_for_good`), is the one pairing for
    /// identity: `assign` with X3's title rule deciding each verdict (`Fit::No` where it refuses); `paired_tracks_in` is gone, not
    /// kept as dead code. A guard, re-pointed at the join's own function. Red first: the join called `paired_tracks_in` (over
    /// `incoming::pair`) at the base.
    #[test]
    fn play_history_moves_by_incoming_pair() {
        let incoming = include_str!("../incoming.rs").replace('\r', "");
        assert!(!incoming.contains("assign"), "Replace's pairing stays incoming::pair's");
        let records = include_str!("records.rs").replace('\r', "");
        assert!(!records.contains("fn paired_tracks_in("), "paired_tracks_in deleted");
        let body = |name: &str| records.split(&format!("\nfn {name}(")).nth(1).and_then(|rest| rest.split("\n}\n").next()).map(str::to_string);
        let join = body("joined_tracks_in").expect("the join's own function");
        assert!(join.contains("assign(") && join.contains("titled_as(") && join.contains("Fit::No"), "the join pairs by assign, the title rule deciding");
        assert!(!join.contains("incoming::pair("), "the join no longer pairs by Replace's passes");
        let joins = body("join_had_in").unwrap();
        assert!(joins.contains("joined_tracks_in(") && joins.contains("taken_for_good("), "the join's pairs move play history");
    }
}
