//! `file_runs`' proposal and its field compare, in base: what a writer below Incoming (a word's, the tempo's) proposes
//! for the one way to change the library's files (decision 0092). `file_runs` re-exports them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::journal::{FileWrite, RunFailed, RunRecord};

/// What a writer proposes: its kind and title, its records, each file's change.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Proposal {
    pub kind: String,
    pub title: String,
    pub records: Vec<RunRecord>,
    pub files: Vec<FileWrite>,
    /// Records set aside before the run (their sheet's stamp no longer matching, `ticked`).
    pub failed: Vec<RunFailed>,
}

/// A field's values as compared: each item split where several are kept in one (ID3v2's NUL, a "; "), trimmed, a
/// byte-order mark left off, in lower case, in order; a number as its number ("03" is 3, "3/12" is 3).
pub fn flat(values: &[String]) -> Vec<String> {
    values
        .iter()
        .flat_map(|v| v.split(['\0', ';']))
        .map(|v| v.trim().trim_start_matches('\u{feff}').to_lowercase())
        .filter(|v| !v.is_empty())
        .map(|v| match v.split('/').next().and_then(|n| n.trim().parse::<u32>().ok()) {
            Some(n) if v.chars().all(|c| c.is_ascii_digit() || c == '/' || c == ' ') => n.to_string(),
            _ => v,
        })
        .collect()
}

/// A run's status, the same for every writer (0092).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    pub id: String,
    pub what: String,
    pub title: String,
    /// waiting, writing, adding (an album action's files moved, its library reading it) or done.
    pub state: &'static str,
    /// Records done of all.
    pub done: usize,
    pub of: usize,
    pub failed: Vec<RunFailed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo: Option<String>,
    pub undone: bool,
    pub albums: usize,
}

/// A record's stamp: what the sheet showed of it (each file's place and fields before and after), so an apply can tell
/// whether that is still what would be written.
pub fn stamp(files: &[FileWrite], album: &str) -> String {
    let said: String = files
        .iter()
        .filter(|f| f.album == album)
        .map(|f| format!("{}\u{1}{:?}\u{1}{:?}\n", f.path.display(), f.before, f.after))
        .collect();
    format!("{:016x}", crate::journal::fingerprint(&said))
}

/// A proposal kept to the records ticked on the sheet, each set aside where its stamp no longer matches what would be
/// written now ("changed since it was planned"); without stamps, the records ticked.
pub fn ticked(mut p: Proposal, albums: &[String], stamps: Option<&HashMap<String, String>>) -> Proposal {
    let stale: Vec<String> = albums
        .iter()
        .filter(|a| stamps.and_then(|s| s.get(*a)).is_some_and(|s| *s != stamp(&p.files, a)))
        .cloned()
        .collect();
    p.files.retain(|f| albums.contains(&f.album) && !stale.contains(&f.album));
    let named = |album: &str| p.records.iter().find(|r| r.album == album).map(|r| (r.title.clone(), r.artist.clone()));
    let set_aside: Vec<RunFailed> = stale
        .iter()
        .map(|album| {
            let n = named(album);
            RunFailed { album: album.clone(), title: n.as_ref().map(|n| n.0.clone()), artist: n.map(|n| n.1), file: None, why: "changed since it was planned".into() }
        })
        .collect();
    p.failed.extend(set_aside);
    p
}

/// The review sheet: per record its changes, old → new and where the new comes from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanView {
    pub kind: String,
    pub title: String,
    pub records: Vec<RecordView>,
    /// Still being made (a match looked for): the sheet asks again.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub making: bool,
}

/// A record on the sheet (and in a run as written: `written` of `of` files).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecordView {
    pub album: String,
    pub title: String,
    pub artist: String,
    /// How many of its files change.
    pub files: usize,
    pub fields: Vec<FieldView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub written: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undone: Option<bool>,
    /// On a sheet: what it showed, sent back with the apply (`ticked`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stamp: Option<String>,
}

/// A field's change: in every file of the record (no `track`), or in one track.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldView {
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    /// The library it is in, as a record's `copies` name it: only where a track's copies change differently, so it shows once
    /// a copy, and a cover's where its copies hold different ones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    pub before: Vec<String>,
    pub after: Vec<String>,
    pub from: String,
    /// How sure what is proposed is (certain ones come ticked, likely ones not: decision 0092, the owner's choice 2);
    /// none where the sheet ticks per record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certainty: Option<&'static str>,
    /// What may be chosen instead (a genre's words found).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<FieldOption>,
}

/// One value a field may be given instead, where it was found and how sure.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldOption {
    pub value: String,
    /// genre or style, as the sources have it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
    pub from: String,
    pub certainty: &'static str,
}

/// A field's rows that are not the same in every file of the record, by track: what a track's copies change alike is said
/// once, and where they differ each row says its library.
fn by_track(rows: Vec<(FieldView, Option<String>)>) -> Vec<FieldView> {
    let same = |a: &FieldView, b: &FieldView| (&a.track, &a.before, &a.after, &a.from) == (&b.track, &b.before, &b.after, &b.from);
    let mut kept: Vec<(FieldView, Option<String>)> = Vec::new();
    for (row, library) in rows {
        if !kept.iter().any(|(k, _)| same(k, &row)) {
            kept.push((row, library));
        }
    }
    let told: Vec<bool> = kept.iter().map(|(row, library)| kept.iter().any(|(other, theirs)| other.track == row.track && theirs != library)).collect();
    kept.into_iter().zip(told).map(|((row, library), told)| FieldView { library: library.filter(|_| told), ..row }).collect()
}

/// The review sheet of a proposal (or of a run, `written` per record).
pub fn view(kind: &str, title: &str, records: &[RunRecord], files: &[FileWrite], written: bool) -> PlanView {
    let roots: Vec<PathBuf> = crate::serve::view::get().filter(|s| s.has_catalogue()).map_or_else(Vec::new, |s| s.catalogue().folders().iter().map(|(_, root)| root.to_path_buf()).collect());
    view_in(kind, title, records, files, written, &roots)
}

/// `view`, its libraries these folders.
pub fn view_in(kind: &str, title: &str, records: &[RunRecord], files: &[FileWrite], written: bool, roots: &[PathBuf]) -> PlanView {
    let library_of = |path: &Path| roots.iter().find(|r| path.starts_with(r)).map(|r| r.display().to_string());
    let records = records
        .iter()
        .filter_map(|r| {
            let mine: Vec<&FileWrite> = files.iter().filter(|f| f.album == r.album).collect();
            // On a sheet, a record with nothing to write is left out; a run lists every record it was given, those
            // it wrote nothing of (each set aside) with none written.
            if mine.is_empty() && !written {
                return None;
            }
            let mut fields: Vec<FieldView> = Vec::new();
            let names: Vec<&String> = mine.iter().flat_map(|f| f.after.iter().map(|(n, _)| n)).fold(Vec::new(), |mut v, n| {
                if !v.contains(&n) {
                    v.push(n);
                }
                v
            });
            for name in names {
                let rows: Vec<(FieldView, Option<String>)> = mine
                    .iter()
                    .filter_map(|f| {
                        let after = f.after.iter().find(|(n, _)| n == name)?.1.clone();
                        let before = f.before.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()).unwrap_or_default();
                        let from = f.from.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()).unwrap_or_default();
                        Some((FieldView { field: name.clone(), track: f.track.clone(), library: None, before, after, from, certainty: None, options: Vec::new() }, library_of(&f.path)))
                    })
                    .collect();
                // The same change in every file of the record: said once.
                let same = rows.len() == mine.len() && rows.windows(2).all(|w| (&w[0].0.before, &w[0].0.after, &w[0].0.from) == (&w[1].0.before, &w[1].0.after, &w[1].0.from));
                match (same, rows.first()) {
                    (true, Some((first, _))) => fields.push(FieldView { track: None, ..first.clone() }),
                    _ => fields.extend(by_track(rows)),
                }
            }
            Some(RecordView {
                album: r.album.clone(),
                title: r.title.clone(),
                artist: r.artist.clone(),
                files: mine.len(),
                fields,
                written: written.then(|| mine.len()),
                undone: written.then(|| mine.iter().all(|f| f.undone)),
                stamp: (!written).then(|| stamp(files, &r.album)),
            })
        })
        .collect();
    PlanView { kind: kind.to_string(), title: title.to_string(), records, making: false }
}
