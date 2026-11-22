//! A record's credit, worked out (SCENARIOS-NAMES NAMES.18, NAMES.20, NAMES.22, NAMES.24; SCENARIOS-DC3 F54; SCENARIOS-CRED
//! CRED.25, CRED.27): one rule over what the files and the sources say, decided once by the record pass and printed by the one
//! formatter. Pure: the store hands it the deciding copy's files and the record's sources' credits.
//!
//! Not DC3c's store: DC3c keeps each source's own list (`credits::files_list`, `credits::source_list`, per record and track); this is
//! the rule's decision over those lists, the record's printed credit.

use serde::{Deserialize, Serialize};

use cratarr_rules::names::{Part, credit_parts};

use crate::credits::{Entry, ListRole, Listed, files_list};

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

/// One entry of a record's printed credit as its answer carries it (CRED.24's list: the head's chips, drawn by the page lane): an artist
/// here by id, else a Listed one by its source's id, its name, its role and the phrase printed after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct CreditEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub listed: Option<Listed>,
    pub name: String,
    pub role: ListRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub phrase: Option<String>,
}

/// What a record's credit prints beside its main artists, as the record pass kept it: its own featuring after the main artist at
/// `after` (an artist here by id, else Listed by its source's id), or the whole credit's text (`printed`, T-DC3-31).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Printed {
    pub after: usize,
    pub role: String,
    pub artist: Option<String>,
    pub listed: Option<Listed>,
    pub name: String,
    pub phrase: Option<String>,
}

/// A record's printed credit as entries: its main artists (id, name) each with its phrase, its own featuring after the artist each
/// follows.
pub fn entries_of(artists: &[(String, String)], phrases: &[Option<String>], printed: &[Printed]) -> Vec<CreditEntry> {
    let featuring = |at: usize| {
        printed.iter().filter(move |p| p.role == "featuring" && p.after == at).map(|p| CreditEntry {
            id: p.artist.clone(),
            listed: p.listed.clone(),
            name: p.name.clone(),
            role: ListRole::Featuring,
            phrase: p.phrase.clone(),
        })
    };
    let mut out = Vec::new();
    for (i, (id, name)) in artists.iter().enumerate() {
        out.push(CreditEntry { id: Some(id.clone()), listed: None, name: name.clone(), role: ListRole::Main, phrase: phrases.get(i).cloned().flatten() });
        out.extend(featuring(i));
    }
    out.extend(printed.iter().filter(|p| p.role == "featuring" && p.after >= artists.len()).map(|p| CreditEntry {
        id: p.artist.clone(),
        listed: p.listed.clone(),
        name: p.name.clone(),
        role: ListRole::Featuring,
        phrase: p.phrase.clone(),
    }));
    out
}

/// What a record's credit prints (NAMES.24, CRED.27): the whole line a placeholder keeps (`printed`), else the one formatter over its
/// entries and their phrases.
pub fn text_of(entries: &[CreditEntry], printed: &[Printed]) -> String {
    match printed.iter().find(|p| p.role == "printed") {
        Some(p) => p.name.clone(),
        None => cratarr_rules::names::credit_text(&entries.iter().map(|e| (e.name.as_str(), e.phrase.as_deref())).collect::<Vec<_>>()),
    }
}

/// MusicBrainz's id of Various Artists, as a release credits a compilation.
pub const VARIOUS_MBID: &str = "89ad4ac3-39f7-470e-963a-56509c546377";

/// A name as credits compare it within one record.
fn key(name: &str) -> String {
    crate::credits::name_key(name)
}

fn part_of(e: &Entry) -> Part {
    Part { name: e.name.clone(), id: e.person.as_ref().map(|p| p.id.clone()), phrase: e.phrase.clone() }
}

/// A file's credit as its ordered list (DC3c's `files_list`; an id it gives as '' none), with F54's rule over its main artists
/// (`credit_parts`): `whole`, the names the same record's files name whole; `source`, a source's credit for the record that may name
/// each part. With the text it prints where the placeholder rule keeps the files' line whole over a part's id (T-DC3-31).
pub fn worked(f: &FilesCredit, keep: &[String], whole: &[&str], source: Option<&SourceCredit>) -> (Vec<Entry>, Option<String>) {
    let mut entries = files_list(&f.line, &f.names, &f.ids, f.title.as_deref(), keep, &cratarr_rules::crates::placeholder);
    // R6: the files' ids pair with their names one for one, or with the entries the names make (a featuring marker splits one), else
    // with none.
    let pairs = f.ids.len() == f.names.len() || f.ids.len() == entries.len();
    for e in &mut entries {
        if !pairs || e.person.as_ref().is_some_and(|p| p.id.trim().is_empty()) {
            e.person = None;
        }
    }
    let mains: Vec<Part> = entries.iter().filter(|e| e.role == ListRole::Main).map(part_of).collect();
    let featuring: Vec<Entry> = entries.iter().filter(|e| e.role == ListRole::Featuring).cloned().collect();
    // The marker before the featured artists is the last main's phrase: it stays after whatever the mains become.
    let marker = if featuring.is_empty() { None } else { mains.last().and_then(|m| m.phrase.clone()) };
    let from_source: Vec<Part> = source.map(|s| s.entries.iter().filter(|e| e.role == ListRole::Main).map(part_of).collect()).unwrap_or_default();
    let parted = credit_parts(&mains, whole, &from_source);
    // Each id as its source gives it: the files' (MusicBrainz's), or the confirming source's own.
    let person = |id: String| {
        source
            .and_then(|s| s.entries.iter().filter_map(|e| e.person.clone()).find(|p| p.id == id))
            .or_else(|| entries.iter().filter_map(|e| e.person.clone()).find(|p| p.id == id))
            .unwrap_or(Listed { source: "musicbrainz".into(), id })
    };
    let mut out: Vec<Entry> = parted.parts.into_iter().map(|p| Entry { person: p.id.map(&person), name: p.name, role: ListRole::Main, phrase: p.phrase }).collect();
    if let Some(last) = out.last_mut() {
        last.phrase = marker;
    }
    out.extend(featuring);
    (out, parted.text)
}

/// Whether a source's credit is the record's: the same main artists and the same featured ones, by name (NAMES.22).
pub fn agrees(record: &[Entry], source: &[Entry]) -> bool {
    let of = |es: &[Entry], role: ListRole| -> Vec<String> {
        let mut k: Vec<String> = es.iter().filter(|e| e.role == role).map(|e| key(&e.name)).collect();
        k.sort();
        k
    };
    of(record, ListRole::Main) == of(source, ListRole::Main) && of(record, ListRole::Featuring) == of(source, ListRole::Featuring)
}

/// A source's credit of Various Artists: by MusicBrainz's id for it, else by its name (`various()`).
fn various_credit(s: &SourceCredit) -> bool {
    let mains: Vec<&Entry> = s.entries.iter().filter(|e| e.role == ListRole::Main).collect();
    mains.len() == 1 && (mains[0].person.as_ref().is_some_and(|p| p.id == VARIOUS_MBID) || cratarr_rules::crates::various(&mains[0].name))
}

/// NAMES.20's one rule for a record, in order (T-NAMES-11):
/// 1. the files' album artist names someone: its artists (F54's rule over its line, the record's files or a source on the record
///    naming each part), then each artist every track credits as a main artist, in order of their first track; its own featuring (a
///    single's "A feat. B") with it, a track's never;
/// 2. "Various Artists", or no album artist (COU.10), and a source's credit naming the record's artists: the source's (Various Artists
///    where it says so, a compilation);
/// 3. "Various Artists", no such source, 1 to 4 main track artists: those, in track order, no compilation;
/// 4. 5 or more: Various Artists, a compilation (and with no track naming anyone, Various Artists stays, no compilation).
///
/// Each source's credit is kept beside it: one agreeing confirms it (`by`), one that differs is a version (NAMES.22), none chosen,
/// none dropping or adding an artist. The phrase after each entry (CRED.27): the files' where their line names the same artists as the
/// credit, then the first confirming source's, else none (`credit_text`'s own).
pub fn credited_artists(said: &RecordSaid) -> RecordCredit {
    let placeholder = cratarr_rules::crates::placeholder;
    // Every track's credit as its files give it, and the names the record's files name whole (T-DC3-9).
    let named: Vec<Vec<Entry>> = said.tracks.iter().map(|t| worked(t, &said.whole, &[], None).0).collect();
    let mut whole: Vec<String> = named.iter().flatten().filter(|e| e.role == ListRole::Main).map(|e| e.name.clone()).collect();
    whole.sort();
    whole.dedup();
    let whole: Vec<&str> = whole.iter().map(String::as_str).collect();
    let tracks: Vec<Vec<Entry>> = said.tracks.iter().map(|t| worked(t, &said.whole, &whole, None).0).collect();
    let line = said.album.as_ref().map(|a| a.line.trim().to_string()).filter(|l| !l.is_empty());
    let mut credit = RecordCredit { tracks, rule: 1, by: vec!["files".into()], ..RecordCredit::default() };
    match &said.album {
        Some(album) if line.as_deref().is_some_and(|l| !cratarr_rules::crates::various(l)) => {
            // Row 1: the album artist, F54's rule over its line with the record's files, then each source, as evidence.
            let (mut entries, mut text) = worked(album, &said.whole, &whole, None);
            let mains = |es: &[Entry]| es.iter().filter(|e| e.role == ListRole::Main).count();
            if mains(&entries) == 1 {
                for s in &said.sources {
                    let (e, t) = worked(album, &said.whole, &whole, Some(s));
                    if mains(&e) > 1 {
                        (entries, text) = (e, t);
                        // The source names each part of the line: a track naming the line whole names them too.
                        credit.tracks = said.tracks.iter().map(|t| worked(t, &said.whole, &whole, Some(s)).0).collect();
                        break;
                    }
                }
            }
            let own: Vec<String> = entries.iter().map(|e| key(&e.name)).collect();
            // Each artist every track credits as a main artist, in order of their first track, after the album's own, with the first
            // id a file pairs it with.
            let mut everywhere: Vec<Entry> = Vec::new();
            if let Some(first) = credit.tracks.first() {
                for e in first.iter().filter(|e| e.role == ListRole::Main && !placeholder(&e.name)) {
                    let k = key(&e.name);
                    let on_every = credit.tracks.iter().all(|t| t.iter().any(|x| x.role == ListRole::Main && key(&x.name) == k));
                    if on_every && !own.contains(&k) && !everywhere.iter().any(|x| key(&x.name) == k) {
                        let person = credit.tracks.iter().flatten().find(|x| key(&x.name) == k && x.person.is_some()).and_then(|x| x.person.clone());
                        everywhere.push(Entry { person, phrase: None, ..e.clone() });
                    }
                }
            }
            if !everywhere.is_empty() {
                // The files' line names fewer artists than the credit: its phrases aren't the credit's (CRED.27 (q6)).
                for e in entries.iter_mut().filter(|e| e.role == ListRole::Main) {
                    e.phrase = None;
                }
                text = None;
                let at = entries.iter().rposition(|e| e.role == ListRole::Main).map_or(0, |i| i + 1);
                entries.splice(at..at, everywhere);
            }
            credit.entries = entries;
            credit.text = text;
        }
        _ => {
            let various = Entry {
                name: line.clone().filter(|l| cratarr_rules::crates::various(l)).unwrap_or_else(|| "Various Artists".into()),
                person: None,
                role: ListRole::Main,
                phrase: None,
            };
            if let Some(s) = said.sources.iter().find(|s| s.entries.iter().any(|e| e.role == ListRole::Main)) {
                // Row 2: the source's credit.
                credit.rule = 2;
                credit.by = vec![s.source.clone()];
                if various_credit(s) {
                    (credit.entries, credit.compilation) = (vec![various], true);
                } else {
                    credit.entries = s.entries.clone();
                }
            } else {
                // Rows 3 and 4: the tracks' main artists, in track order.
                let mut mains: Vec<Entry> = Vec::new();
                for e in credit.tracks.iter().flatten().filter(|e| e.role == ListRole::Main && !placeholder(&e.name)) {
                    if !mains.iter().any(|x| key(&x.name) == key(&e.name)) {
                        mains.push(Entry { phrase: None, ..e.clone() });
                    }
                }
                (credit.rule, credit.entries, credit.compilation) = match mains.len() {
                    0 => (4, vec![various], false),
                    1..=4 => (3, mains, false),
                    _ => (4, vec![various], true),
                };
            }
        }
    }
    // Each source beside it: a confirmation (its phrases where the files give none, T-NAMES-13), or a version that differs.
    for s in &said.sources {
        if credit.rule == 2 && credit.by.first() == Some(&s.source) {
            continue;
        }
        if agrees(&credit.entries, &s.entries) {
            if !credit.by.contains(&s.source) {
                credit.by.push(s.source.clone());
            }
            for (e, x) in credit.entries.iter_mut().zip(&s.entries) {
                if key(&e.name) != key(&x.name) {
                    continue;
                }
                if e.phrase.is_none() {
                    e.phrase = x.phrase.clone().filter(|p| !cratarr_rules::names::is_separator(p));
                }
                // A featured artist the files name with no id: the confirming source's (its open, CRED.12); never a main artist's,
                // whose id is its files' (A2.1).
                if e.role == ListRole::Featuring && e.person.is_none() {
                    e.person = x.person.clone();
                }
            }
        } else if !credit.versions.contains(&s.source) {
            credit.versions.push(s.source.clone());
        }
    }
    if let Some(last) = credit.entries.last_mut() {
        last.phrase = None;
    }
    credit
}

#[cfg(test)]
mod tests;
