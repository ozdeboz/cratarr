//! A record's credit, worked out (SCENARIOS-NAMES NAMES.18, NAMES.20, NAMES.22, NAMES.24; SCENARIOS-DC3 F54; SCENARIOS-CRED
//! CRED.25, CRED.27): one rule over what the files and the sources say, decided once by the record pass and printed by the one
//! formatter. Pure: the store hands it the deciding copy's files and the record's sources' credits.
//!
//! Not DC3c's store: DC3c keeps each source's own list (`credits::files_list`, `credits::source_list`, per record and track); this is
//! the rule's decision over those lists, the record's printed credit.

use serde::{Deserialize, Serialize};

use crate::credits::{Entry, Listed, ListRole, files_list};

/// One file's credit as it gives it: its line as written (ARTIST, ALBUMARTIST), its names apart (its list tag, else the line as its
/// library's separators split it), its MusicBrainz ids in order ('' where it gives none), and its title (a track's).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesCredit {
    pub line: String,
    pub names: Vec<String>,
    #[serde(default)]
    pub ids: Vec<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// A source's credit for the record (its release's), as an ordered list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCredit {
    pub source: String,
    pub entries: Vec<Entry>,
}

/// What the record's credit is worked out from: the deciding copy's album artist (none where no file names one) and its tracks'
/// credits in track order, each source's credit of the record in the order they answered (a guess's never), and the names the library
/// keeps whole.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordSaid {
    pub album: Option<FilesCredit>,
    pub tracks: Vec<FilesCredit>,
    #[serde(default)]
    pub sources: Vec<SourceCredit>,
    #[serde(default)]
    pub whole: Vec<String>,
}

/// The record's credit: its artists and its own featuring in order, each with its phrase; the text it prints where that is not
/// its entries' own (T-DC3-31); NAMES.20's row that decided it; whether it is a compilation (rows 2 and 4); the sources it agrees
/// with and those whose credit differs (NAMES.22's versions); each track's credit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordCredit {
    pub entries: Vec<Entry>,
    pub text: Option<String>,
    pub rule: u8,
    pub compilation: bool,
    pub by: Vec<String>,
    pub versions: Vec<String>,
    pub tracks: Vec<Vec<Entry>>,
}

impl RecordCredit {
    /// The record's artists: its main entries.
    pub fn mains(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.role == ListRole::Main)
    }

    /// What it prints (NAMES.24, CRED.27): the files' whole line where it is kept so, else the one formatter over its entries.
    pub fn shown(&self) -> String {
        self.text.clone().unwrap_or_else(|| cratarr_rules::names::credit_text(&self.entries.iter().map(|e| (e.name.as_str(), e.phrase.as_deref())).collect::<Vec<_>>()))
    }
}

fn entries_of(f: &FilesCredit, whole: &[String]) -> Vec<Entry> {
    files_list(&f.line, &f.names, &f.ids, f.title.as_deref(), whole, &cratarr_rules::crates::placeholder)
}

/// NAMES.20's one rule for a record, in order. Red first (3a-1): today's, the album artist's names as the files split them (else
/// the first track's), each with its id where the counts pair.
pub fn credited_artists(said: &RecordSaid) -> RecordCredit {
    let tracks: Vec<Vec<Entry>> = said.tracks.iter().map(|t| entries_of(t, &said.whole)).collect();
    let from = said.album.as_ref().or(said.tracks.first());
    let entries: Vec<Entry> = from
        .map(|f| {
            f.names
                .iter()
                .enumerate()
                .map(|(i, n)| Entry {
                    name: n.clone(),
                    person: f.ids.get(i).filter(|id| !id.is_empty() && f.ids.len() == f.names.len()).map(|id| Listed { source: "musicbrainz".into(), id: id.clone() }),
                    role: ListRole::Main,
                    phrase: None,
                })
                .collect()
        })
        .unwrap_or_default();
    RecordCredit { entries, text: None, rule: 1, compilation: false, by: vec!["files".into()], versions: Vec::new(), tracks }
}

#[cfg(test)]
mod tests;
