//! `file_runs`' proposal and its field compare, in base: what a writer below Incoming (a word's, the tempo's) proposes
//! for the one way to change the library's files (decision 0092). `file_runs` re-exports them.

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
