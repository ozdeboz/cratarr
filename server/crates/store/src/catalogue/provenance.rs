//! WM's layer 2 in the record pass (SCENARIOS-WM WM.8-WM.13, WM.17, WM.23; SCENARIOS-COUPLING COU.17, COU.18): an arrival Cratarr
//! placed joins the record its placement names, by the domain's one judgement (`identity::judge`), read from the catalogue's rows:
//! the placement (the strongest evidence, kept for good), its files as the pairing reads them, the record's judging lists and copies.

use std::collections::HashMap;

use anyhow::Result;
use cratarr_domain::facts::{Present, RefTrack, pairs_with};
use cratarr_domain::identity::{Arrival, Candidate, Verdict, judge};
use rusqlite::{Connection, OptionalExtension, params};

use crate::serve::ids;

/// What the record pass does with an arrival its placement names a record for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Placed {
    /// It is the record's copy: each of its files, by its place in the album, takes the record's track it pairs with (none: a track
    /// the record had no file of, given the record as a new one).
    Copy { record: i64, takes: Vec<(usize, i64)> },
    /// Another record, linked to `of` as its edition, named by `of`'s names (OQ-WM-3 (a): "fetched for …").
    Edition { of: i64, title: String, artist: String },
}

/// A placed file's told place while it is as placed (T-COU-17): its place on its record's list, where its size and time are the ones
/// first read (none read yet: as placed).
pub(super) fn told_of(db: &Connection, folder: i64, rel: &str, size: i64, mtime: i64) -> Result<Option<(u32, u32)>> {
    let row: Option<(Option<u32>, Option<u32>, Option<i64>, Option<i64>)> = db
        .prepare_cached("SELECT disc, number, size, mtime FROM placements WHERE folder = ?1 AND path = ?2 AND number IS NOT NULL")?
        .query_row(params![folder, rel], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .optional()?;
    Ok(row.and_then(|(disc, number, s, m)| {
        let as_placed = s.is_none_or(|s| s == size) && m.is_none_or(|m| m == mtime);
        as_placed.then_some((disc.unwrap_or(1), number?))
    }))
}

/// One file as the pairing reads it, from the catalogue's rows: its tags through `TrackFacts::of` (its name where it has no title,
/// WM.25), its length as the library serves it, its told place while as placed.
pub(super) fn present_in(db: &Connection, folder: i64, rel: &str) -> Result<Present> {
    let read: Vec<(String, String)> =
        db.prepare_cached("SELECT name, value FROM tags WHERE folder = ?1 AND rel = ?2 AND name IS NOT NULL ORDER BY n")?.query_map(params![folder, rel], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    let (ms, size, mtime): (Option<i64>, i64, i64) =
        db.prepare_cached("SELECT duration_ms, size, mtime FROM files WHERE folder = ?1 AND rel = ?2")?.query_row(params![folder, rel], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?.unwrap_or((None, 0, 0));
    let mut p = crate::incoming::TrackFacts::of(&crate::naming::Tags(read), rel, None).side(rel.to_string(), ms.filter(|m| *m > 0));
    p.told = told_of(db, folder, rel, size, mtime)?;
    Ok(p)
}

/// The record an arrival of these files was placed for, where a placement names one above them (`expected_of`'s walk up the
/// folders, over `placed_in`'s copies).
pub(super) fn placed_for(placed: &HashMap<(i64, String), i64>, files: &[(i64, String)]) -> Option<i64> {
    super::records::expected_of(placed, files).map(|(_, record)| record)
}

/// The judgement of an arrival of `files` (an album's, in its order) placed for `record` (WM.8): its copy, each file to the record's
/// track it pairs with; or its edition; none where the record is gone, or retired (C50: a retired record is never joined, OQ16).
/// In the record pass's transaction.
pub(super) fn judged_in(tx: &Connection, files: &[(i64, String)], release: Option<&str>, artist: &str, record: i64) -> Result<Option<Placed>> {
    let Some((title, credit)): Option<(String, String)> =
        tx.prepare_cached("SELECT COALESCE(title, ''), COALESCE(artist, '') FROM records WHERE id = ?1 AND merged_into IS NULL AND retired_at IS NULL")?.query_row([record], |r| Ok((r.get(0)?, r.get(1)?))).optional()?
    else {
        return Ok(None);
    };
    let arrival: Vec<Present> = files.iter().map(|(folder, rel)| present_in(tx, *folder, rel)).collect::<Result<_>>()?;
    // The lists that judge (no guess: VR8), and the record's own copies, each a folder's present files of its live tracks.
    let lists: Vec<Vec<RefTrack>> = super::lists::lists_in(tx, record)?.into_iter().filter(|l| !l.guessed && !l.tracks.is_empty()).map(|l| l.tracks).collect();
    let held: Vec<(i64, String, i64, Option<u32>, Option<u32>)> = tx
        .prepare_cached(&format!(
            "SELECT f.folder, f.rel, t.id, t.disc, t.number FROM files f JOIN tracks t ON t.id = f.track
             WHERE t.record = ?1 AND t.retired_at IS NULL AND {} ORDER BY f.folder, f.rel",
            super::PRESENT_F
        ))?
        .query_map([record], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let ours: std::collections::HashSet<(i64, &str)> = files.iter().map(|(f, r)| (*f, r.as_str())).collect();
    let mut copies: std::collections::BTreeMap<i64, Vec<Present>> = std::collections::BTreeMap::new();
    // The record's tracks as a list to pair the arrival's files with: each live track once, by its first file.
    let mut tracks: Vec<(i64, RefTrack)> = Vec::new();
    for (folder, rel, track, disc, number) in held.iter().filter(|(f, r, ..)| !ours.contains(&(*f, r.as_str()))) {
        let p = present_in(tx, *folder, rel)?;
        if !tracks.iter().any(|(t, _)| t == track) {
            let title = if p.title.is_empty() { p.named.first().cloned().unwrap_or_default() } else { p.title.clone() };
            let place = (disc.or(p.disc).unwrap_or(1), number.or(p.number).unwrap_or(u32::try_from(tracks.len() + 1).unwrap_or(u32::MAX)));
            tracks.push((*track, RefTrack { disc: place.0, position: place.1, title, length_ms: p.ms, recording: None }));
        }
        copies.entry(*folder).or_default().push(p);
    }
    let copies: Vec<Vec<Present>> = copies.into_values().collect();
    let releases: Vec<String> = tx
        .prepare_cached("SELECT lower(value) FROM \"references\" WHERE subject = ?1 AND source = 'musicbrainz' AND kind = 'release'")?
        .query_map([ids::record(record)], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let folded = super::identity::fold(&credit);
    let candidate = Candidate { id: record, releases: &releases, artist: &folded, lists: &lists, copies: &copies };
    let verdict = judge(&Arrival { placed_for: Some(record), release, artist, files: &arrival }, &candidate);
    Ok(Some(match verdict {
        Verdict::Same(_) => {
            let list: Vec<RefTrack> = tracks.iter().map(|(_, t)| t.clone()).collect();
            let takes = pairs_with(&list, &arrival).into_iter().map(|(t, f)| (f, tracks[t].0)).collect();
            Placed::Copy { record, takes }
        }
        Verdict::Edition(_) | Verdict::Apart(_) => Placed::Edition { of: record, title, artist: credit },
    }))
}

/// A placed file's told place (disc, number) and its size and time as first read.
pub(super) type Told = (u32, u32, Option<i64>, Option<i64>);

/// The files placed for `record` (COU.18), by (catalogue folder, path): each its told place and its size and time as first read.
pub(super) fn told_files_in(db: &Connection, record: i64) -> Result<HashMap<(i64, String), Told>> {
    let mut st = db.prepare_cached("SELECT folder, path, disc, number, size, mtime FROM placements WHERE record = ?1 AND number IS NOT NULL")?;
    let rows = st.query_map([record], |r| {
        Ok(((r.get::<_, i64>(0)?, r.get::<_, String>(1)?), (r.get::<_, Option<u32>>(2)?.unwrap_or(1), r.get::<_, u32>(3)?, r.get::<_, Option<i64>>(4)?, r.get::<_, Option<i64>>(5)?)))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Its told place while the file is as placed: its size and time the ones first read (none read yet: as placed). Changed outside
/// Cratarr and read again, it pairs as any file.
pub(super) fn as_placed(t: &Told, size: i64, mtime: i64) -> Option<(u32, u32)> {
    (t.2.is_none_or(|s| s == size) && t.3.is_none_or(|m| m == mtime)).then_some((t.0, t.1))
}
