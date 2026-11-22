//! A record's credits as the services said them (SCENARIOS-CRED; DC3c): MusicBrainz's and Discogs' credits, one row per credit, by
//! record, source and the reference they were read for, in the source's own order (`n`), each with where the source places it, who
//! by the source's own id, the name and the credited-as name it gives, its word for the role, MusicBrainz's attributes, and the
//! phrase printed after it in an artist credit (CRED.24). A typed table, since it joins ids (G6, as REL's `artist_relations`). The
//! files' credits are not kept here: their home is the catalogue's tags, read with the record (`credits_read_in`). What a record's
//! credits are, merged by id with no source ranked, is the domain's (`cratarr_domain::credits::credits_of`).
//!
//! - **One source's answer at a time** (CRED.7): its rows for a reference replace what it said before, only where they differ, so
//!   the same answer writes nothing and owes nothing; a credit it no longer names goes. "Asked, none" is FA15's facts row (`credits`,
//!   value none), and takes that source's rows for the reference away.
//! - **A guess's credits** (T-CRED-13): kept, unshown (`guess`), merged with nothing, until the pairing or a hand confirms the match.
//! - **Every write owes the record's change-log row** (LV1): the triggers in `record_log`, on this table too; and a reference an
//!   artist comes to hold owes every record whose credits name it (CRED.8 (b): one credit once both ids are one artist's).

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::Catalogue;

/// The domain's credit lists, for the crates above that read a source's artist credit into one (CRED.24).
pub use cratarr_domain::credits::{Entry, ListRole, Listed, source_list};

pub(super) const SCHEMA: &str = "
-- CRED (DC3c): a record's credits as each service said them, by the reference read for (`kind:value`), in the source's order (`n`).
-- `track`: where the source's list places it, `disc-position` items joined by ','; NULL on every track. `tracks_said`: the
-- source's own words for it (Discogs' \"A4, B3\"). `person`: the source's own id. `guess` 1: kept unshown (T-CRED-13).
CREATE TABLE IF NOT EXISTS credits (
  record INTEGER NOT NULL, source TEXT NOT NULL, ref TEXT NOT NULL DEFAULT '', n INTEGER NOT NULL,
  track TEXT, tracks_said TEXT, person TEXT, name TEXT NOT NULL, credited_as TEXT, word TEXT NOT NULL, attributes TEXT,
  phrase TEXT, guess INTEGER NOT NULL DEFAULT 0, at INTEGER NOT NULL,
  PRIMARY KEY (record, source, ref, n)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS credits_by_person ON credits (source, person) WHERE person IS NOT NULL;
";

/// One credit as a service said it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreditRow {
    /// Where the source's list places it: (disc, position) items; empty on every track.
    pub track: Vec<(u32, u32)>,
    /// The source's own words for where (Discogs' "A4, B3", "A1 to A3").
    pub tracks_said: Option<String>,
    pub person: Option<String>,
    pub name: String,
    pub credited_as: Option<String>,
    pub word: String,
    pub attributes: Vec<String>,
    pub phrase: Option<String>,
}

/// A credit row as kept, with its source and reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub source: String,
    pub of: String,
    pub row: CreditRow,
    pub guess: bool,
}

fn track_text(track: &[(u32, u32)]) -> Option<String> {
    (!track.is_empty()).then(|| track.iter().map(|(d, p)| format!("{d}-{p}")).collect::<Vec<_>>().join(","))
}

/// `disc-position` items read back.
pub fn track_of(text: Option<&str>) -> Vec<(u32, u32)> {
    text.unwrap_or_default()
        .split(',')
        .filter_map(|x| {
            let (d, p) = x.trim().split_once('-')?;
            Some((d.parse().ok()?, p.parse().ok()?))
        })
        .collect()
}

fn attributes_text(a: &[String]) -> Option<String> {
    (!a.is_empty()).then(|| a.join("\u{1f}"))
}

fn attributes_of(text: Option<String>) -> Vec<String> {
    text.map(|t| t.split('\u{1f}').map(str::to_string).filter(|a| !a.is_empty()).collect()).unwrap_or_default()
}

/// The rows a record keeps for a source and reference, in order.
fn kept_in(tx: &Connection, record: i64, source: &str, of: &str) -> Result<Vec<(CreditRow, bool)>> {
    Ok(tx
        .prepare_cached(
            "SELECT track, tracks_said, person, name, credited_as, word, attributes, phrase, guess FROM credits WHERE record = ?1 AND source = ?2 AND ref = ?3 ORDER BY n",
        )?
        .query_map(params![record, source, of], |r| {
            Ok((
                CreditRow {
                    track: track_of(r.get::<_, Option<String>>(0)?.as_deref()),
                    tracks_said: r.get(1)?,
                    person: r.get(2)?,
                    name: r.get(3)?,
                    credited_as: r.get(4)?,
                    word: r.get(5)?,
                    attributes: attributes_of(r.get(6)?),
                    phrase: r.get(7)?,
                },
                r.get::<_, bool>(8)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?)
}

/// `Catalogue::keep_credits` in the caller's transaction: whether anything changed.
pub(super) fn keep_credits_in(tx: &Connection, record: i64, source: &str, of: &str, rows: Option<&[CreditRow]>, guess: bool, at: i64) -> Result<bool> {
    let now: Vec<(CreditRow, bool)> = rows.unwrap_or_default().iter().map(|r| (r.clone(), guess)).collect();
    let changed = kept_in(tx, record, source, of)? != now;
    if changed {
        tx.prepare_cached("DELETE FROM credits WHERE record = ?1 AND source = ?2 AND ref = ?3")?.execute(params![record, source, of])?;
        let mut ins = tx.prepare_cached(
            "INSERT INTO credits (record, source, ref, n, track, tracks_said, person, name, credited_as, word, attributes, phrase, guess, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )?;
        for (n, (r, g)) in now.iter().enumerate() {
            ins.execute(params![
                record,
                source,
                of,
                n as i64,
                track_text(&r.track),
                r.tracks_said,
                r.person,
                r.name,
                r.credited_as,
                r.word,
                attributes_text(&r.attributes),
                r.phrase,
                g,
                at
            ])?;
        }
    }
    // FA15: the source asked, with its time; none where it found none (VR40). Its value names how many it gave.
    super::facts::said_in(tx, record, "credits", source, of, rows.map(|r| r.len().to_string()).as_deref(), at)?;
    Ok(changed)
}

/// A merged record's credits onto the record it went into (FA16): a source's rows for a reference the other holds too give way to
/// the later read. None left on `from`. In the caller's transaction.
pub(super) fn merge_in(tx: &Connection, from: i64, into: i64) -> Result<()> {
    tx.execute(
        "DELETE FROM credits WHERE record = ?2 AND EXISTS (SELECT 1 FROM credits c WHERE c.record = ?1 AND c.source = credits.source AND c.ref = credits.ref
           AND c.at > (SELECT max(at) FROM credits d WHERE d.record = ?2 AND d.source = credits.source AND d.ref = credits.ref))",
        params![from, into],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO credits (record, source, ref, n, track, tracks_said, person, name, credited_as, word, attributes, phrase, guess, at)
         SELECT ?2, source, ref, n, track, tracks_said, person, name, credited_as, word, attributes, phrase, guess, at FROM credits c WHERE record = ?1
           AND NOT EXISTS (SELECT 1 FROM credits d WHERE d.record = ?2 AND d.source = c.source AND d.ref = c.ref)",
        params![from, into],
    )?;
    tx.execute("DELETE FROM credits WHERE record = ?1", [from])?;
    Ok(())
}

/// Every credit a record keeps, every source's, in order. On the caller's connection.
pub fn credits_in(db: &Connection, record: i64) -> Result<Vec<Kept>> {
    Ok(db
        .prepare_cached(
            "SELECT source, ref, track, tracks_said, person, name, credited_as, word, attributes, phrase, guess FROM credits WHERE record = ?1 ORDER BY source, ref, n",
        )?
        .query_map([record], |r| {
            Ok(Kept {
                source: r.get(0)?,
                of: r.get(1)?,
                row: CreditRow {
                    track: track_of(r.get::<_, Option<String>>(2)?.as_deref()),
                    tracks_said: r.get(3)?,
                    person: r.get(4)?,
                    name: r.get(5)?,
                    credited_as: r.get(6)?,
                    word: r.get(7)?,
                    attributes: attributes_of(r.get(8)?),
                    phrase: r.get(9)?,
                },
                guess: r.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

/// Whether a stored credit names this source's id (T-CRED-9: an open of a Listed person a credit names; T-CRED-10: an artist a
/// credit names is kept). On the caller's connection.
pub fn credited_in(db: &Connection, source: &str, person: &str) -> Result<bool> {
    Ok(db.prepare_cached("SELECT 1 FROM credits WHERE source = ?1 AND person = ?2 AND guess = 0 LIMIT 1")?.query_row(params![source, person], |_| Ok(())).optional()?.is_some())
}

impl Catalogue {
    /// What `source` says of `record`'s credits now, read for the reference `of` (`kind:value`): its rows in its order, replacing
    /// what it said for that reference before where they differ; none: asked, and it names none (FA15). `guess`: the reference is a
    /// name-found match the pairing doesn't confirm, so its credits are kept unshown (T-CRED-13). Whether anything changed.
    /// Blocking.
    pub fn keep_credits(&self, record: i64, source: &str, of: &str, rows: Option<&[CreditRow]>, guess: bool, at: i64) -> Result<bool> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let changed = keep_credits_in(&tx, record, source, of, rows, guess, at)?;
        tx.commit()?;
        Ok(changed)
    }

    /// Every credit a record keeps (the services'), every source's, in order. On a reader. Blocking.
    pub fn credits_kept(&self, record: i64) -> Result<Vec<Kept>> {
        match self.reader() {
            Some(c) => credits_in(&c, record),
            None => credits_in(&self.db(), record),
        }
    }

    /// The MusicBrainz releases had records hold where their files name none (T-CRED-24: a match's, a hand's pick, a list's): the
    /// works pass asks them too, with its one question, at its pace. Blocking.
    pub fn held_releases(&self) -> Result<Vec<String>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT DISTINCT f.value FROM \"references\" f JOIN records r ON 'al-' || r.id = f.subject
             WHERE f.source = 'musicbrainz' AND f.kind = 'release' AND f.how <> 'files' AND r.merged_into IS NULL AND r.retired_at IS NULL
               AND EXISTS (SELECT 1 FROM tracks t JOIN files x ON x.track = t.id WHERE t.record = r.id AND t.retired_at IS NULL AND x.gone_at IS NULL)
             ORDER BY f.value",
        )?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What a record's credits are worked out from (`credits_read_in`), on one reader in one read transaction: the record it is now
    /// (a merged one's end, RV3). The view's read beside `record_read` (its rev read before both, LV2). Blocking.
    pub fn credits_read(&self, record: i64, library: &dyn Fn(i64) -> cratarr_domain::holding::Library) -> Result<cratarr_domain::credits::Credits> {
        let rules = self.folder_rules();
        let read = |c: &mut Connection| -> Result<cratarr_domain::credits::Credits> {
            let tx = c.transaction()?;
            let end = super::records::merged_end(&tx, record)?.unwrap_or(record);
            let out = credits_read_in(&tx, end, library, &rules)?;
            tx.commit()?;
            Ok(out)
        };
        match self.reader() {
            Some(mut c) => read(&mut c),
            None => read(&mut self.db()),
        }
    }

    /// How many artists here have this name's key (`ids::artist_key`): X8, a name two share is never searched by. Blocking.
    pub fn artists_of_key(&self, key: &str) -> Result<usize> {
        Ok(self.db().prepare_cached("SELECT count(*) FROM artists WHERE key = ?1")?.query_row([key], |r| r.get::<_, i64>(0))? as usize)
    }

    /// Whether a stored credit (no guess's) names this source's id. On a reader. Blocking.
    pub fn credited(&self, source: &str, person: &str) -> Result<bool> {
        match self.reader() {
            Some(c) => credited_in(&c, source, person),
            None => credited_in(&self.db(), source, person),
        }
    }
}

/// The role tags a file's credits are read from (CRED.4: the reader's names, Picard's vocabulary).
const ROLE_TAGS: [&str; 11] = ["composer", "lyricist", "writer", "arranger", "conductor", "performer", "producer", "engineer", "mixer", "djmixer", "remixer"];

type Presents = std::collections::BTreeMap<i64, Vec<(i64, cratarr_domain::facts::Present)>>;

/// The copy whose files speak for a record's credits (FA2, T-CRED-3): a main library's, then Incoming's, then someone's own music
/// (where no other names the record), the first added within each; none where it has no present file.
fn deciding(present: &Presents, added: &std::collections::HashMap<i64, i64>, library: &dyn Fn(i64) -> cratarr_domain::holding::Library) -> Option<i64> {
    use cratarr_domain::holding::Library;
    present
        .keys()
        .filter(|f| **f >= 0)
        .min_by_key(|f| {
            let rank = match library(**f) {
                Library::Main => 0,
                Library::Incoming { .. } => 1,
                Library::Own(_) => 2,
            };
            (rank, added.get(f).copied().unwrap_or(i64::MAX), **f)
        })
        .copied()
}

/// The deciding copy's files' credits, track by track (CRED.4): each role tag's value as the file gives it, named `files@<folder>`
/// (an ID3v2 pair's instrument from its raw key: T-CRED-25), and its artist credit as an ordered list (CRED.24: ARTISTS, else ARTIST
/// as the library's separators split it, its ids, its title's guest, F54's placeholder). One query for every file's tags (CRED.22).
fn files_said(
    folder: i64,
    files: &[(i64, cratarr_domain::facts::Present)],
    by_rel: &std::collections::HashMap<String, Vec<(String, String, String)>>,
    number_of: &std::collections::HashMap<i64, u32>,
    rules: &super::normalise::FolderRules,
) -> Vec<cratarr_domain::credits::Said> {
    use cratarr_domain::credits::{ListRole, On, Said, files_list};
    let source = format!("files@{folder}");
    let keep = rules.encoding(folder).artist_keep.clone();
    let mut out = Vec::new();
    for (track, file) in files {
        let Some(&n) = number_of.get(track) else { continue };
        let tags = by_rel.get(&file.key).map(Vec::as_slice).unwrap_or_default();
        let all_of = |name: &str| tags.iter().filter(|(_, n, _)| n == name).map(|(_, _, v)| v.clone()).collect::<Vec<_>>();
        let on = On::Tracks(vec![n]);
        for (raw, name, value) in tags.iter().filter(|(_, n, _)| ROLE_TAGS.contains(&n.as_str())) {
            let word = match raw.split_once(':') {
                Some(("TIPL" | "TMCL", key)) if name == "performer" => format!("performer:{}", key.trim().to_lowercase()),
                _ => name.clone(),
            };
            out.push(Said { source: source.clone(), person: None, name: value.clone(), credited_as: None, word, attributes: Vec::new(), on: on.clone() });
        }
        let line = all_of("artist").into_iter().next().unwrap_or_default();
        let listed = all_of("artists");
        let parts = if listed.is_empty() { rules.values(folder, "artist", &all_of("artist")) } else { listed };
        let ids: Vec<String> = all_of("musicbrainz_artistid").iter().flat_map(|v| v.split(['/', ';']).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect::<Vec<_>>()).collect();
        let title = all_of("title").into_iter().next();
        for e in files_list(&line, &parts, &ids, title.as_deref(), &keep, &cratarr_rules::crates::placeholder) {
            let word = if e.role == ListRole::Featuring { "featuring" } else { "artist" };
            out.push(Said { source: source.clone(), person: e.person, name: e.name, credited_as: None, word: word.into(), attributes: Vec::new(), on: on.clone() });
        }
    }
    out
}

/// `?1, ?2, …` for `n` values.
fn marks(n: usize) -> String {
    (1..=n).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ")
}

/// The artists here a record's credits may name (T-CRED-4): those holding the ids they name, those of the names they give (each with
/// C55's number where another has the name), and the record's own artists. A query for the ids and one for the names, never one per
/// person (CRED.22).
fn here_in(tx: &Connection, record: i64, said: &[cratarr_domain::credits::Said]) -> Result<cratarr_domain::credits::Here> {
    use cratarr_domain::credits::{Artist, Here, Listed};
    let mut here = Here::default();
    const ARTIST: &str = "SELECT a.id, a.name, CASE WHEN EXISTS (SELECT 1 FROM artists b WHERE b.key = a.key AND b.id <> a.id) THEN n.number END
        FROM artists a LEFT JOIN artist_numbers n ON n.artist = a.id";
    let artist = |r: &rusqlite::Row, at: usize| -> rusqlite::Result<Artist> { Ok(Artist { id: crate::serve::ids::artist(r.get(at)?), name: r.get(at + 1)?, number: r.get(at + 2)? }) };
    let persons: Vec<Listed> = said.iter().filter_map(|s| s.person.clone()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    if !persons.is_empty() {
        let values: Vec<String> = persons.iter().map(|p| p.id.clone()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        let sql = format!(
            "SELECT f.source, f.value, x.* FROM \"references\" f JOIN ({ARTIST}) x ON 'ar-' || x.id = f.subject WHERE f.kind = 'artist' AND f.value IN ({})",
            marks(values.len())
        );
        let mut st = tx.prepare(&sql)?;
        let found: Vec<(Listed, Artist)> = st
            .query_map(rusqlite::params_from_iter(values.iter()), |r| Ok((Listed { source: r.get(0)?, id: r.get(1)? }, artist(r, 2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        here.by_ref = found.into_iter().filter(|(l, _)| persons.contains(l)).collect();
    }
    let keys: Vec<String> = said
        .iter()
        .flat_map(|s| std::iter::once(crate::serve::ids::artist_key(&s.name)).chain(s.credited_as.as_deref().map(crate::serve::ids::artist_key)))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if !keys.is_empty() {
        let sql = format!("SELECT x.* FROM ({ARTIST}) x JOIN artists k ON k.id = x.id WHERE k.key IN ({}) ORDER BY x.id", marks(keys.len()));
        let mut st = tx.prepare(&sql)?;
        here.by_name = st.query_map(rusqlite::params_from_iter(keys.iter()), |r| artist(r, 0))?.collect::<rusqlite::Result<_>>()?;
    }
    here.record_artists = tx
        .prepare_cached("SELECT artist FROM record_artists WHERE record = ?1 ORDER BY position")?
        .query_map([record], |r| r.get::<_, i64>(0))?
        .map(|r| r.map(crate::serve::ids::artist))
        .collect::<rusqlite::Result<_>>()?;
    Ok(here)
}

/// What a record's credits are worked out from, in the view's one read (HD14; CRED.2, CRED.22: one read per answer, never a query
/// per track or per person): the deciding copy's files' credits (`files_said`), every service's credits kept (a guess's left out:
/// T-CRED-13), each placed on the record's tracks by its list's pairing with the deciding copy (VR01, PAIR01; a MusicBrainz
/// release with no list kept, by disc and position), else at its own positions as written (T-CRED-6); the artists here
/// (`here_in`); the record's track numbers, in disc and place order (T-CRED-20).
pub fn credits_read_in(
    tx: &Connection,
    record: i64,
    library: &dyn Fn(i64) -> cratarr_domain::holding::Library,
    rules: &super::normalise::FolderRules,
) -> Result<cratarr_domain::credits::Credits> {
    use cratarr_domain::credits::{Credits, Listed, On, Said};
    use std::collections::{BTreeSet, HashMap};
    let tracks: Vec<(i64, i64, i64)> = tx
        .prepare_cached("SELECT id, COALESCE(disc, 1), COALESCE(number, 0) FROM tracks WHERE record = ?1 AND retired_at IS NULL ORDER BY COALESCE(disc, 1), COALESCE(number, 0), id")?
        .query_map([record], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let number_of: HashMap<i64, u32> = tracks.iter().enumerate().map(|(i, (id, _, _))| (*id, i as u32 + 1)).collect();
    let mut by_place: HashMap<(u32, u32), u32> = tracks.iter().enumerate().map(|(i, (_, d, n))| ((*d as u32, *n as u32), i as u32 + 1)).collect();
    let mut all: Vec<u32> = (1..=tracks.len() as u32).collect();
    let lists = super::lists::lists_in(tx, record)?;
    // A record not had has no tracks: its numbers are its first kept list's (the sources in the order asked).
    let not_had = tracks.is_empty();
    if not_had && let Some(l) = lists.iter().filter(|l| !l.tracks.is_empty() && !l.guessed).min_by_key(|l| cratarr_domain::facts::asked_at(&l.reference.source)) {
        by_place = l.tracks.iter().enumerate().map(|(i, t)| ((t.disc, t.position), i as u32 + 1)).collect();
        all = (1..=l.tracks.len() as u32).collect();
    }
    // The record's present files by folder (one query), and the deciding copy's named tags (one query): each of its files as the
    // pairing reads it (`TrackFacts::of`, its header's length), never a query per file (CRED.22).
    let mut present: Presents = Presents::new();
    let mut added: HashMap<i64, i64> = HashMap::new();
    let mut rels: HashMap<(i64, String), (i64, Option<i64>)> = HashMap::new();
    let mut st = tx.prepare_cached(&format!(
        "SELECT f.folder, f.rel, f.track, f.duration_ms, f.mtime FROM files f JOIN tracks t ON t.id = f.track WHERE t.record = ?1 AND t.retired_at IS NULL AND {} ORDER BY f.folder, f.rel",
        super::PRESENT_F
    ))?;
    for row in st.query_map([record], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, i64>(4)?)))? {
        let (folder, rel, track, ms, mtime) = row?;
        let first = added.entry(folder).or_insert(mtime);
        *first = (*first).min(mtime);
        present.entry(folder).or_default().push((track, cratarr_domain::facts::Present { key: rel.clone(), ..Default::default() }));
        rels.insert((folder, rel), (track, ms));
    }
    let decides = deciding(&present, &added, library);
    let mut by_rel: HashMap<String, Vec<(String, String, String)>> = HashMap::new();
    if let Some(folder) = decides {
        let mut st = tx.prepare_cached(
            "SELECT t.rel, t.raw, t.name, t.value FROM tags t JOIN files f ON f.folder = t.folder AND f.rel = t.rel JOIN tracks k ON k.id = f.track
             WHERE k.record = ?1 AND t.folder = ?2 AND t.name IS NOT NULL ORDER BY t.rel, t.n",
        )?;
        for row in st.query_map(params![record, folder], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))? {
            let (rel, raw, name, value) = row?;
            by_rel.entry(rel).or_default().push((raw, name, value));
        }
    }
    let files: Vec<(i64, cratarr_domain::facts::Present)> = decides
        .and_then(|f| present.get(&f).map(|v| (f, v)))
        .map(|(folder, v)| {
            v.iter()
                .map(|(track, p)| {
                    let read: Vec<(String, String)> = by_rel.get(&p.key).into_iter().flatten().map(|(_, n, v)| (n.clone(), v.clone())).collect();
                    let ms = rels.get(&(folder, p.key.clone())).and_then(|(_, ms)| *ms);
                    (*track, crate::incoming::TrackFacts::of(&crate::naming::Tags(read), &p.key, None).side(p.key.clone(), ms))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut said: Vec<Said> = match decides {
        Some(folder) => files_said(folder, &files, &by_rel, &number_of, rules),
        None => Vec::new(),
    };
    let sides: Vec<cratarr_domain::facts::Present> = files.iter().map(|(_, p)| p.clone()).collect();
    let mut placing: std::collections::BTreeMap<(String, String), HashMap<(u32, u32), u32>> = std::collections::BTreeMap::new();
    for kept in credits_in(tx, record)?.into_iter().filter(|k| !k.guess) {
        let place = placing.entry((kept.source.clone(), kept.of.clone())).or_insert_with(|| {
            let (kind, value) = kept.of.split_once(':').unwrap_or(("", ""));
            let reference = cratarr_domain::facts::Reference::new(&kept.source, kind, value);
            match lists.iter().find(|l| l.reference == reference && !l.tracks.is_empty()) {
                // A record not had: its source's own list's places, as the record's numbers read them.
                Some(l) if not_had => l.tracks.iter().enumerate().map(|(i, t)| ((t.disc, t.position), i as u32 + 1)).collect(),
                Some(l) => cratarr_domain::facts::pairs_with(&l.tracks, &sides)
                    .into_iter()
                    .filter_map(|(t, f)| Some(((l.tracks[t].disc, l.tracks[t].position), *number_of.get(&files[f].0)?)))
                    .collect(),
                None if kept.source == "musicbrainz" => by_place.clone(),
                None => HashMap::new(),
            }
        });
        let on = if kept.row.track.is_empty() {
            On::Every
        } else {
            let placed: BTreeSet<u32> = kept.row.track.iter().filter_map(|p| place.get(p).copied()).collect();
            if placed.is_empty() {
                On::Unplaced(kept.row.tracks_said.clone().unwrap_or_else(|| kept.row.track.iter().map(|(d, p)| format!("{d}-{p}")).collect::<Vec<_>>().join(", ")))
            } else {
                On::Tracks(placed.into_iter().collect())
            }
        };
        said.push(Said {
            source: kept.source.clone(),
            person: kept.row.person.clone().map(|id| Listed { source: kept.source.clone(), id }),
            name: kept.row.name,
            credited_as: kept.row.credited_as,
            word: kept.row.word,
            attributes: kept.row.attributes,
            on,
        });
    }
    let here = here_in(tx, record, &said)?;
    Ok(Credits { said, here, all })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cratarr_domain::credits::{On, Said};
    use cratarr_domain::holding::{Fetcher, Library};

    const MUSIC: i64 = 0;
    const IN: i64 = 1;
    const OWN_A: i64 = 2;

    fn library(folder: i64) -> Library {
        match folder {
            IN => Library::Incoming { fetcher: Fetcher::Lidarr, uploader: None },
            OWN_A => Library::Own("alice".to_string()),
            _ => Library::Main,
        }
    }

    /// A record of `n` tracks, each with a file in every folder of `copies`, each file's tags `tags(folder)`: its id.
    fn record(cat: &Catalogue, n: u32, copies: &[(i64, Vec<(&str, &str)>)]) -> i64 {
        let db = cat.db();
        db.execute("INSERT INTO records (born, title, artist) VALUES (1, 'ALL', 'Yann Tiersen')", []).unwrap();
        let record = db.last_insert_rowid();
        for t in 1..=n {
            db.execute("INSERT INTO tracks (born, record, disc, number) VALUES (1, ?1, 1, ?2)", params![record, t]).unwrap();
            let track = db.last_insert_rowid();
            for (folder, tags) in copies {
                let rel = format!("r{record}/{folder}/{t:02}.flac");
                db.execute("INSERT INTO files (folder, rel, size, mtime, track, duration_ms, state) VALUES (?1, ?2, 1, ?3, ?4, 200000, 1)", params![folder, rel, 100 + folder, track]).unwrap();
                let title = format!("Track {t}");
                let mut all: Vec<(&str, &str)> = vec![("title", title.as_str())];
                all.extend(tags.iter().copied());
                for (i, (name, value)) in all.iter().enumerate() {
                    db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (?1, ?2, ?3, ?4, ?4, ?5)", params![folder, rel, i as i64, name, value]).unwrap();
                }
            }
        }
        record
    }

    fn said(cat: &Catalogue, record: i64) -> Vec<Said> {
        cat.credits_read(record, &library).unwrap().said
    }

    fn mixers(s: &[Said]) -> Vec<(String, String)> {
        s.iter().filter(|x| x.word == "mixer").map(|x| (x.source.clone(), x.name.clone())).collect::<std::collections::BTreeSet<_>>().into_iter().collect()
    }

    /// CRED.4 (the app's half), T-CRED-3: the files' credits are the deciding copy's, track by track. (c) alice's own copy says
    /// PRODUCER "G. Arzel": it changes nothing anyone sees, MUSIC's copy decides; (d) a copy in IN says MIXER "Tom Lerat" while
    /// MUSIC's says "Gwen Arzel": MUSIC's while it is present, IN's once it is gone. Never an own copy's role tags deciding for others;
    /// never the files' credits read from whichever copy was read last. Red first: no reader reads a role tag.
    #[test]
    fn the_deciding_copys_files_are_the_files_credits() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let record = record(
            &cat,
            3,
            &[(OWN_A, vec![("producer", "G. Arzel"), ("mixer", "Gwen Arzel")]), (IN, vec![("mixer", "Tom Lerat")]), (MUSIC, vec![("mixer", "Gwen Arzel"), ("composer", "Yann Tiersen")])],
        );
        let s = said(&cat, record);
        assert_eq!(mixers(&s), [("files@0".to_string(), "Gwen Arzel".to_string())], "(d) MUSIC's copy decides while present");
        assert!(!s.iter().any(|x| x.name == "G. Arzel"), "(c) an own copy speaks for no one while a main copy names the record");
        assert!(s.iter().filter(|x| x.word == "composer").all(|x| matches!(&x.on, On::Tracks(t) if t.len() == 1)), "track by track");
        cat.db().execute("UPDATE files SET gone_at = 5 WHERE folder = ?1", [MUSIC]).unwrap();
        assert_eq!(mixers(&said(&cat, record)), [("files@1".to_string(), "Tom Lerat".to_string())], "(d) IN's copy decides once MUSIC's is gone");
    }

    /// CRED.19 (the read's half, through F54's `credit_parts`): a role value splits only where the files or a source name each
    /// person. Track 1's COMPOSER "Kerys Lae & Yann Tiersen", no source naming the composers: one name-only credit (a); track 2's
    /// "Kerys Lae / Yann Tiersen" (the library's spaced slash): two (c); track 3's "Kerys Lae & Yann Tiersen" with MusicBrainz's work
    /// composers Kerys Lae and Yann Tiersen by id, their names joined the files' value: two, by MusicBrainz's ids (b); track 4's
    /// PERFORMER "Carlos Niño & Friends (percussion)": one performer, percussion, "Friends" no person (d). Never a credit cut on a bare
    /// "&"; never a split list of CRED's own. Red first: role values are kept as written, so (c) is one name and (b) stays one beside
    /// MusicBrainz's two.
    #[test]
    fn role_credits_split_by_f54_alone_in_the_read() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let record = {
            let db = cat.db();
            db.execute("INSERT INTO records (born, title, artist) VALUES (1, 'ALL', 'Yann Tiersen')", []).unwrap();
            let record = db.last_insert_rowid();
            let roles = [("composer", "Kerys Lae & Yann Tiersen"), ("composer", "Kerys Lae / Yann Tiersen"), ("composer", "Kerys Lae & Yann Tiersen"), ("performer", "Carlos Niño & Friends (percussion)")];
            for (t, (role, value)) in roles.iter().enumerate() {
                let t = t as u32 + 1;
                db.execute("INSERT INTO tracks (born, record, disc, number) VALUES (1, ?1, 1, ?2)", params![record, t]).unwrap();
                let track = db.last_insert_rowid();
                let rel = format!("all/{t:02}.flac");
                db.execute("INSERT INTO files (folder, rel, size, mtime, track, duration_ms, state) VALUES (0, ?1, 1, 1, ?2, 200000, 1)", params![rel, track]).unwrap();
                for (i, (name, value)) in [("title", format!("Track {t}")), ("tracknumber", t.to_string()), (*role, value.to_string())].iter().enumerate() {
                    db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, ?1, ?2, ?3, ?3, ?4)", params![rel, i as i64, name, value]).unwrap();
                }
            }
            record
        };
        let composer = |id: &str, name: &str| CreditRow { track: vec![(1, 3)], person: Some(id.into()), name: name.into(), word: "composer".into(), ..Default::default() };
        cat.keep_credits(record, "musicbrainz", "release:all", Some(&[composer("mb-kl", "Kerys Lae"), composer("mb-yt", "Yann Tiersen")]), false, 1).unwrap();
        let c = cat.credits_read(record, &library).unwrap();
        let roles = cratarr_domain::credits::credits_of(&c.said, &c.here, &c.all);
        let people = |role: &str| -> Vec<(String, Option<Vec<u32>>, bool)> {
            roles.iter().filter(|r| r.role == role).flat_map(|r| r.people.iter()).map(|p| (p.name.clone(), p.tracks.clone(), p.person.is_some())).collect()
        };
        let composers = people("composer");
        assert!(composers.contains(&("Kerys Lae & Yann Tiersen".to_string(), Some(vec![1]), false)), "(a) one name-only credit on track 1 alone: {composers:?}");
        assert!(!composers.iter().any(|(n, _, _)| n.contains('/')), "(c) never the slash value whole: {composers:?}");
        for name in ["Kerys Lae", "Yann Tiersen"] {
            let one: Vec<&(String, Option<Vec<u32>>, bool)> = composers.iter().filter(|(n, _, _)| n == name).collect();
            assert_eq!(one.len(), 1, "{name}: one credit: {composers:?}");
            assert_eq!((one[0].1.clone(), one[0].2), (Some(vec![2, 3]), true), "(b), (c) {name}: tracks 2 and 3, by MusicBrainz's id: {composers:?}");
        }
        let performers = people("performer");
        assert_eq!(performers.iter().map(|(n, _, _)| n.as_str()).collect::<Vec<_>>(), ["Carlos Niño & Friends"], "(d) one performer, never \"Friends\"");
    }

    /// CRED.22 (kind 8), the limits a record's read holds: a box set of 3 discs and 60 tracks, MusicBrainz naming 140 players on each
    /// recording, a Discogs credit on "1 to 60" placed through its kept list, a COMPOSER of 193 characters. The answer's credits come
    /// grouped, each person once per role and detail, on every track with no tracks named ("1 to 60" reads as all of them), the long
    /// value kept whole as the files' words. One read: `credits_read` is one reader's transaction of a fixed set of queries, whatever
    /// the tracks or the people (by its shape: the store counts no statements). Never a name cut short in the store.
    #[test]
    fn a_records_credits_come_in_one_read() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let long = format!("{} / Yann Tiersen", "Kerys Lae ".repeat(18).trim());
        assert!(long.len() >= 193);
        let record = {
            let db = cat.db();
            db.execute("INSERT INTO records (born, title, artist) VALUES (1, 'Box', 'Royal Concertgebouw Orchestra')", []).unwrap();
            let record = db.last_insert_rowid();
            for disc in 1..=3u32 {
                for t in 1..=20u32 {
                    db.execute("INSERT INTO tracks (born, record, disc, number) VALUES (1, ?1, ?2, ?3)", params![record, disc, t]).unwrap();
                    let track = db.last_insert_rowid();
                    let rel = format!("box/{disc}-{t:02}.flac");
                    db.execute("INSERT INTO files (folder, rel, size, mtime, track, duration_ms, state) VALUES (0, ?1, 1, 1, ?2, 200000, 1)", params![rel, track]).unwrap();
                    for (i, (name, value)) in [("title", format!("Part {disc}.{t}")), ("tracknumber", t.to_string()), ("discnumber", disc.to_string()), ("composer", long.clone())].iter().enumerate() {
                        db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, ?1, ?2, ?3, ?3, ?4)", params![rel, i as i64, name, value]).unwrap();
                    }
                }
            }
            let tracks: Vec<serde_json::Value> =
                (1..=3u32).flat_map(|d| (1..=20u32).map(move |t| serde_json::json!({ "disc": 1, "position": (d - 1) * 20 + t, "title": format!("Part {d}.{t}"), "length_ms": 200_000 }))).collect();
            db.execute("INSERT INTO reference_tracks (record, source, kind, value, tracks, at) VALUES (?1, 'discogs', 'release', '4510', ?2, 1)", params![record, serde_json::Value::Array(tracks).to_string()]).unwrap();
            record
        };
        let players: Vec<CreditRow> = (1..=3u32)
            .flat_map(|d| (1..=20u32).map(move |t| (d, t)))
            .flat_map(|(d, t)| (0..140).map(move |p| CreditRow { track: vec![(d, t)], person: Some(format!("p{p}")), name: format!("Player {p}"), word: "instrument".into(), attributes: vec!["violin".into()], ..Default::default() }))
            .collect();
        cat.keep_credits(record, "musicbrainz", "release:box", Some(&players), false, 1).unwrap();
        let all: Vec<(u32, u32)> = (1..=60).map(|p| (1, p)).collect();
        cat.keep_credits(record, "discogs", "release:4510", Some(&[CreditRow { track: all, tracks_said: Some("1 to 60".into()), person: Some("9".into()), name: "Bernard Haitink".into(), word: "Conductor".into(), ..Default::default() }]), false, 1).unwrap();
        let c = cat.credits_read(record, &library).unwrap();
        assert_eq!(c.all.len(), 60);
        let roles = cratarr_domain::credits::credits_of(&c.said, &c.here, &c.all);
        let performers = roles.iter().find(|r| r.role == "performer").expect("the players");
        assert_eq!(performers.people.len(), 140, "each player once, grouped");
        assert!(performers.people.iter().all(|p| p.tracks.is_none() && p.detail.as_deref() == Some("violin")), "on every track: no tracks named");
        let conductor = roles.iter().find(|r| r.role == "conductor").expect("the conductor");
        assert!(conductor.people[0].tracks.is_none() && conductor.people[0].at.is_none(), "\"1 to 60\" reads as all of them: {:?}", conductor.people[0]);
        let composer = roles.iter().find(|r| r.role == "composer").expect("the long composer value");
        assert_eq!(composer.people[0].name, long, "the long value kept whole as the files' words");
    }

    /// CRED.17's re-read, T-CRED-26: reader 10 marks every read file due a re-read apart from its state (no column a change log reads),
    /// and a file whose tags read the same owes nothing: no `record` row, no `index_log` row; it is due no more. A file that gained a
    /// role tag owes its record once. Red first: a reader's bump set every file PENDING (an `index_log` column), and each batch deleted
    /// and re-inserted its tag rows whatever changed.
    #[test]
    fn a_reread_owes_nothing_for_an_unchanged_file() {
        use super::super::tags::fixtures;
        use lofty::prelude::AudioFile;
        let d = fixtures::dir("cred-reread");
        let album = d.join("Yann Tiersen - ALL");
        std::fs::create_dir_all(&album).unwrap();
        for name in ["one", "two"] {
            let f = fixtures::flac(&album, name);
            let mut file = lofty::flac::FlacFile::read_from(&mut std::fs::File::open(&f).unwrap(), lofty::config::ParseOptions::new()).unwrap();
            let mut vc = file.remove_vorbis_comments().unwrap_or_default();
            for (k, v) in [("ARTIST", "Yann Tiersen"), ("ALBUM", "ALL"), ("TITLE", name), ("COMPOSER", "Yann Tiersen")] {
                vc.push(k.into(), v.into());
            }
            file.set_vorbis_comments(vc);
            file.save_to_path(&f, lofty::config::WriteOptions::default()).unwrap();
        }
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        while cat.read_tags_batch(100).unwrap() > 0 {}
        cat.identify().unwrap();
        {
            // Its tracks a record's, as a build leaves them.
            let db = cat.db();
            db.execute("INSERT INTO records (born, title, artist) VALUES (1, 'ALL', 'Yann Tiersen')", []).unwrap();
            let record = db.last_insert_rowid();
            db.execute("UPDATE tracks SET record = ?1", [record]).unwrap();
        }
        let count = |sql: &str| -> i64 { cat.db().query_row(sql, [], |r| r.get(0)).unwrap() };
        // As reader 10's bump marks them: every read file due again.
        cat.db().execute("INSERT OR IGNORE INTO tags_due (folder, rel) SELECT folder, rel FROM files WHERE state = 1", []).unwrap();
        cat.db().execute("DELETE FROM record_owed", []).unwrap();
        let (logged, due) = (count("SELECT count(*) FROM index_log"), count("SELECT count(*) FROM tags_due"));
        assert_eq!(due, 2, "both files due");
        let index_before = count("SELECT COALESCE(max(rev), 0) FROM index_log");
        assert_eq!(cat.read_tags_batch(100).unwrap(), 2, "both read again");
        assert_eq!(count("SELECT count(*) FROM tags_due"), 0, "due no more");
        assert_eq!(count("SELECT count(*) FROM record_owed"), 0, "an unchanged file owes its record nothing");
        assert_eq!((count("SELECT count(*) FROM index_log"), count("SELECT COALESCE(max(rev), 0) FROM index_log")), (logged, index_before), "nor the library's log");
        // One file gains a role tag: owed once.
        let f = album.join("one.flac");
        let mut file = lofty::flac::FlacFile::read_from(&mut std::fs::File::open(&f).unwrap(), lofty::config::ParseOptions::new()).unwrap();
        let mut vc = file.remove_vorbis_comments().unwrap_or_default();
        vc.push("PRODUCER".into(), "Gwen Arzel".into());
        file.set_vorbis_comments(vc);
        file.save_to_path(&f, lofty::config::WriteOptions::default()).unwrap();
        cat.db().execute("INSERT OR IGNORE INTO tags_due (folder, rel) SELECT folder, rel FROM files WHERE state = 1", []).unwrap();
        cat.read_tags_batch(100).unwrap();
        assert_eq!(count("SELECT count(*) FROM record_owed"), 1, "a file that gained a role tag owes its record once");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Whether the record is owed to the change log now; the owed rows let go of after (as the served library's check moves them).
    fn owed(cat: &Catalogue, record: i64) -> bool {
        let db = cat.db();
        let n: i64 = db.query_row("SELECT count(*) FROM record_owed WHERE record = ?1", [record], |r| r.get(0)).unwrap();
        db.execute("DELETE FROM record_owed", []).unwrap();
        n > 0
    }

    fn row(person: &str, name: &str, word: &str) -> CreditRow {
        CreditRow { person: Some(person.into()), name: name.into(), word: word.into(), ..Default::default() }
    }

    /// CRED.7 (kind 3): credits follow their sources. (a) Discogs' facts land after the panel opened: the record is owed once (LV1).
    /// (b) MusicBrainz no longer names Maëlle Dour: her credit goes, owed once. (e) a source answers the same again: nothing owed.
    /// (d) MusicBrainz answers the release is gone: "asked, none" kept with its time (FA15, VR40), and its part of each credit goes; the
    /// files' and Discogs' stay. (c) Discogs switched off reads nothing, so its kept credits stay. Never a credit flapping at a
    /// re-read; never a credit kept with a source that no longer names it. Red first: no store.
    #[test]
    fn credits_follow_their_sources() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let record = record(&cat, 3, &[(MUSIC, vec![("mixer", "Gwen Arzel")])]);
        let _ = owed(&cat, record);
        let mb = "release:mb-all";
        let mut violin = row("md", "Maëlle Dour", "instrument");
        violin.attributes = vec!["guest".into(), "violin".into()];
        violin.track = vec![(1, 2), (1, 3)];
        let first = [violin.clone(), row("ga", "Gwen Arzel", "mix")];
        assert!(cat.keep_credits(record, "musicbrainz", mb, Some(&first), false, 10).unwrap());
        assert!(owed(&cat, record), "MusicBrainz's read owes the record");
        // (a) Discogs lands later: owed once.
        assert!(cat.keep_credits(record, "discogs", "release:4010", Some(&[row("90408", "Lou Kerjean", "Artwork")]), false, 11).unwrap());
        assert!(owed(&cat, record), "(a) Discogs' facts owe the record");
        // (e) The same answer again: nothing.
        assert!(!cat.keep_credits(record, "musicbrainz", mb, Some(&first), false, 12).unwrap());
        assert!(!owed(&cat, record), "(e) the same answer owes nothing");
        // (b) Maëlle Dour no longer named: her credit goes, owed once.
        assert!(cat.keep_credits(record, "musicbrainz", mb, Some(&[row("ga", "Gwen Arzel", "mix")]), false, 13).unwrap());
        assert!(owed(&cat, record), "(b) owed");
        let names = |cat: &Catalogue| said(cat, record).into_iter().map(|s| (s.source, s.name)).collect::<std::collections::BTreeSet<_>>();
        assert!(!names(&cat).iter().any(|(_, n)| n == "Maëlle Dour"), "(b) her credit went with its one source");
        // (d) MusicBrainz knows the release no more: "asked, none" with its time; its part goes, the others stay.
        assert!(cat.keep_credits(record, "musicbrainz", mb, None, false, 14).unwrap());
        let facts = cat.facts_said(record).unwrap();
        assert!(facts.iter().any(|f| f.fact == "credits" && f.source == "musicbrainz" && f.of == mb && f.value.is_none() && f.at == 14), "(d) asked, none: {facts:?}");
        let now = names(&cat);
        assert!(!now.iter().any(|(s, _)| s == "musicbrainz"), "(d) MusicBrainz's part gone: {now:?}");
        assert!(now.contains(&("discogs".to_string(), "Lou Kerjean".to_string())) && now.contains(&("files@0".to_string(), "Gwen Arzel".to_string())), "(c), (d) the others stay: {now:?}");
    }
}
