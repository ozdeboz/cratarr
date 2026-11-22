//! A record's credits (SCENARIOS-CRED; DC3c): one store of facts with their sources, the files', MusicBrainz's and Discogs', merged
//! by id, no source ranked above another, shown to every account. Pure: the store reads what each source said, the artists here
//! and the record's tracks, and hands them here.
//!
//! - **One roles table** ([`read_role`], CRED.1, T-CRED-1): every source's word for what someone did, one role each, in
//!   MusicBrainz's vocabulary; an instrument the same from every source; a compound Discogs string split on its commas outside
//!   brackets, a bracket read as the role's own word, an instrument's qualifier or the instruments themselves (T-CRED-27);
//!   MusicBrainz's "guest", "additional", "solo" and "lead" kept apart from the instrument as qualifiers. A word no row names is
//!   kept under its source's own word, after the table's roles, never dropped.
//! - **Who a credit names** (T-CRED-4): the artist here holding its reference; else, for a name with no reference, the one
//!   person a source credits on the same record by that name; else the one artist here of that shown name, never one among
//!   namesakes; else Listed by its reference, or a name alone. Two sources' persons are one only where one artist here holds
//!   both references (T-CRED-5): never by a name.
//! - **One merge** ([`credits_of`], CRED.2): one credit per role, detail and person, its tracks the union of its sources'
//!   (every track: the record's, no tracks named; T-CRED-6, T-CRED-20), each source that names it in `by`. Credits are a set:
//!   none is a version of another (T-CRED-18).
//! - **One order** (T-CRED-7, CRED.18): the roles in the table's order, then the words no row names in first-track order;
//!   within a role by first track, then shown name. `by` in the fixed source order (the files, then the services in the order
//!   asked), which ranks nothing. Nothing depends on the order the sources answered.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::facts::asked_at;
use crate::view::ActionView;

/// The roles in the one order (T-CRED-7) and their shown words *(new words)*. A word no row names comes after them, under its
/// source's own word.
pub const ROLES: [(&str, &str); 17] = [
    ("artist", "Artist"),
    ("featuring", "Featuring"),
    ("composer", "Composer"),
    ("lyricist", "Lyricist"),
    ("writer", "Writer"),
    ("arranger", "Arranger"),
    ("conductor", "Conductor"),
    ("chorus master", "Chorus master"),
    ("orchestra", "Orchestra"),
    ("performer", "Performer"),
    ("producer", "Producer"),
    ("mixer", "Mixer"),
    ("engineer", "Engineer"),
    ("mastering", "Mastering"),
    ("remixer", "Remixer"),
    ("dj mix", "DJ mix"),
    ("programming", "Programming"),
];

/// A role's place in the one order; a word no row names after every role.
pub fn role_at(role: &str) -> usize {
    ROLES.iter().position(|(r, _)| *r == role).unwrap_or(ROLES.len())
}

/// A role's shown word; a word no row names as its source gives it.
pub fn role_word(role: &str) -> String {
    ROLES.iter().find(|(r, _)| *r == role).map_or_else(|| role.to_string(), |(_, w)| (*w).to_string())
}

/// The table's rows: a source's word and the role it is (CRED.1). `files` words are the reader's names (Picard's vocabulary,
/// decision 0008) and ID3v2's TIPL role words as Picard writes them; `musicbrainz` its relationship types; `discogs` its role
/// strings. An instrument is no row: it is the performer's detail ([`INSTRUMENTS`]).
pub const TABLE: [(&str, &str, &str); 51] = [
    ("files", "artist", "artist"),
    ("files", "artists", "artist"),
    ("files", "featuring", "featuring"),
    ("files", "composer", "composer"),
    ("files", "lyricist", "lyricist"),
    ("files", "writer", "writer"),
    ("files", "arranger", "arranger"),
    ("files", "conductor", "conductor"),
    ("files", "performer", "performer"),
    ("files", "producer", "producer"),
    ("files", "engineer", "engineer"),
    ("files", "mixer", "mixer"),
    ("files", "mix", "mixer"),
    ("files", "djmixer", "dj mix"),
    ("files", "dj-mix", "dj mix"),
    ("files", "remixer", "remixer"),
    ("musicbrainz", "artist", "artist"),
    ("musicbrainz", "featuring", "featuring"),
    ("musicbrainz", "composer", "composer"),
    ("musicbrainz", "lyricist", "lyricist"),
    ("musicbrainz", "writer", "writer"),
    ("musicbrainz", "arranger", "arranger"),
    ("musicbrainz", "conductor", "conductor"),
    ("musicbrainz", "chorus master", "chorus master"),
    ("musicbrainz", "performing orchestra", "orchestra"),
    ("musicbrainz", "instrument", "performer"),
    ("musicbrainz", "vocal", "performer"),
    ("musicbrainz", "performer", "performer"),
    ("musicbrainz", "producer", "producer"),
    ("musicbrainz", "mix", "mixer"),
    ("musicbrainz", "engineer", "engineer"),
    ("musicbrainz", "audio", "engineer"),
    ("musicbrainz", "sound", "engineer"),
    ("musicbrainz", "recording", "engineer"),
    ("musicbrainz", "mastering", "mastering"),
    ("musicbrainz", "remixer", "remixer"),
    ("musicbrainz", "mix-dj", "dj mix"),
    ("musicbrainz", "programming", "programming"),
    ("discogs", "artist", "artist"),
    ("discogs", "featuring", "featuring"),
    ("discogs", "producer", "producer"),
    ("discogs", "mixed by", "mixer"),
    ("discogs", "mastered by", "mastering"),
    ("discogs", "written-by", "writer"),
    ("discogs", "composed by", "composer"),
    ("discogs", "lyrics by", "lyricist"),
    ("discogs", "arranged by", "arranger"),
    ("discogs", "conductor", "conductor"),
    ("discogs", "engineer", "engineer"),
    ("discogs", "recorded by", "engineer"),
    ("discogs", "remix", "remixer"),
];

/// Words that name a role inside a Discogs bracket ("Engineer [Mastering]"): the role they are.
const BRACKET_ROLES: [(&str, &str); 4] = [("mastering", "mastering"), ("mixing", "mixer"), ("mix", "mixer"), ("recording", "engineer")];

/// MusicBrainz's attributes that qualify an instrument or a role, never one themselves (T-CRED-27): kept apart, shown beside it.
pub const QUALIFIERS: [&str; 9] = ["guest", "additional", "solo", "lead", "assistant", "associate", "co", "executive", "minor"];

/// Instruments a Discogs role string or a file's pair names (the performer's detail), lower case, MusicBrainz's words. A Discogs
/// word neither here nor in [`TABLE`] is a role of its own word ("Artwork", "Liner Notes").
pub const INSTRUMENTS: [&str; 96] = [
    "vocals", "lead vocals", "backing vocals", "background vocals", "voice", "choir", "chorus", "rap", "spoken word", "whistling",
    "piano", "grand piano", "electric piano", "keyboards", "synthesizer", "organ", "harpsichord", "clavinet", "rhodes", "mellotron",
    "celesta", "accordion", "harmonium", "melodica", "bass", "double bass", "bass guitar", "electric bass", "upright bass",
    "contrabass", "guitar", "acoustic guitar", "electric guitar", "classical guitar", "12-string guitar", "slide guitar",
    "lap steel guitar", "pedal steel guitar", "drums", "drum machine", "percussion", "timpani", "congas", "bongos", "tambourine",
    "shaker", "cymbal", "triangle", "handclaps", "cajón", "djembe", "tabla", "vibraphone", "marimba", "xylophone", "glockenspiel",
    "bells", "chimes", "gong", "violin", "viola", "cello", "strings", "fiddle", "harp", "flute", "piccolo", "recorder", "clarinet",
    "bass clarinet", "oboe", "bassoon", "saxophone", "alto saxophone", "tenor saxophone", "baritone saxophone", "soprano saxophone",
    "trumpet", "cornet", "flugelhorn", "trombone", "horn", "french horn", "tuba", "euphonium", "brass", "woodwind", "harmonica",
    "banjo", "mandolin", "ukulele", "sitar", "oud", "bouzouki", "theremin", "turntables",
];

/// A word as the table reads it: trimmed, lower case.
fn folded(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Whether a word is an instrument.
pub fn is_instrument(word: &str) -> bool {
    INSTRUMENTS.contains(&folded(word).as_str())
}

/// The role a source's word is, by the table; none for a word no row names.
pub fn role_of(source: &str, word: &str) -> Option<&'static str> {
    let w = folded(word);
    TABLE.iter().find(|(s, x, _)| *s == source_kind(source) && *x == w).map(|(_, _, r)| *r)
}

/// A source as the table names it: a copy's files (`files@MUSIC`) are the files.
pub fn source_kind(source: &str) -> &str {
    if source.starts_with("files") { "files" } else { source }
}

/// One reading of a source's word (CRED.1): the role, the instrument or other detail, its qualifiers, and the person's name as
/// the source gives it without what the word took from it (a file's "Kerys Lae (vocals)" is Kerys Lae, vocals).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reading {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub qualifiers: Vec<String>,
    pub name: String,
}

/// Splits a Discogs role string on its commas outside brackets ("Producer, Mixed By"; "Instruments [Synthesizer, Drum Machine]"
/// is one part).
fn discogs_parts(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut depth, mut part) = (0u32, String::new());
    for c in s.chars() {
        match c {
            '[' => {
                depth += 1;
                part.push(c);
            }
            ']' => {
                depth = depth.saturating_sub(1);
                part.push(c);
            }
            ',' if depth == 0 => out.push(std::mem::take(&mut part)),
            c => part.push(c),
        }
    }
    out.push(part);
    out.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

/// A Discogs part and its bracket: ("Engineer", Some("Mastering")).
fn bracketed(part: &str) -> (String, Option<String>) {
    match part.split_once('[') {
        Some((base, rest)) => (base.trim().to_string(), Some(rest.trim_end_matches(']').trim().to_string()).filter(|b| !b.is_empty())),
        None => (part.trim().to_string(), None),
    }
}

/// A file's performer value as Picard writes it, "Name (instrument)": the name and its instrument.
fn performer_value(value: &str) -> (String, Option<String>) {
    let v = value.trim();
    if let Some(open) = v.rfind(" (")
        && v.ends_with(')')
    {
        let instrument = folded(&v[open + 2..v.len() - 1]);
        if !instrument.is_empty() {
            return (v[..open].trim().to_string(), Some(instrument));
        }
    }
    (v.to_string(), None)
}

/// What a source's word says of a person named `value` (CRED.1, T-CRED-27): one reading, or several where one word names several
/// roles or instruments ("Producer, Mixed By"; "Bass, Vocals"; "Instruments [Synthesizer, Drum Machine]"; an instrument relation
/// with two instruments). `attributes` are MusicBrainz's on the relationship. A file's "performer:<instrument>" (an ID3v2 TMCL,
/// TIPL or IPLS pair, Cratarr's reader's name for it) is a performer on that instrument. A word no row names is a role of its own
/// word, never dropped.
pub fn read_role(source: &str, word: &str, attributes: &[String], value: &str) -> Vec<Reading> {
    // Red first (SCENARIOS-CRED CRED.1): no table reads a word yet.
    let _ = (source, word, attributes, value, discogs_parts, bracketed, performer_value, BRACKET_ROLES);
    Vec::new()
}

/// A source's own id for a person: `musicbrainz` or `discogs`, and the id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CreditListed"))]
#[cfg_attr(test, ts(export))]
pub struct Listed {
    pub source: String,
    pub id: String,
}

/// Where a source places a credit on the record: on every track the record has (a release's credit, a credit with no tracks), on
/// some of the record's tracks (its track numbers, through the source's list's pairing), or at positions the pairing doesn't
/// give, as the source writes them ("A1 to A3": shown so, placed on no track, T-CRED-6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum On {
    Every,
    Tracks(Vec<u32>),
    Unplaced(String),
}

/// One credit as a source said it, for one record (the store's row, read).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Said {
    /// `files@<library>`, `musicbrainz`, `discogs`.
    pub source: String,
    /// The person by the source's own id, where it gives one.
    #[serde(default)]
    pub person: Option<Listed>,
    /// The person as the source names them (Discogs' artist name, MusicBrainz's, the file's value).
    pub name: String,
    /// The name as credited on this record (Discogs' `anv`, MusicBrainz's `target-credit`): the source's words (T-CRED-17).
    #[serde(default)]
    pub credited_as: Option<String>,
    /// The source's word for what they did.
    pub word: String,
    #[serde(default)]
    pub attributes: Vec<String>,
    pub on: On,
}

/// An artist here, as a credit names them: by id, their one name (NAMES.1) and their number among namesakes (C55).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub number: Option<i64>,
}

/// The artists here a record's credits may name: by the references they hold, and by their shown names' keys (`name_key`); and
/// the record's own artists (never repeated under "artist": T-CRED-7).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Here {
    #[serde(default)]
    pub by_ref: Vec<(Listed, Artist)>,
    #[serde(default)]
    pub by_name: Vec<Artist>,
    #[serde(default)]
    pub record_artists: Vec<String>,
}

impl Here {
    fn holder(&self, r: &Listed) -> Option<&Artist> {
        self.by_ref.iter().find(|(x, _)| x == r).map(|(_, a)| a)
    }

    /// The artists here of this shown name.
    fn named(&self, name: &str) -> Vec<&Artist> {
        let key = name_key(name);
        let mut out: Vec<&Artist> = self.by_name.iter().filter(|a| name_key(&a.name) == key).collect();
        out.dedup_by(|a, b| a.id == b.id);
        out
    }
}

/// A name as credits compare it within one record: lower case, the common accents off (rules' `name_key`).
pub fn name_key(s: &str) -> String {
    cratarr_rules::names::name_key(s.trim()).into_iter().collect()
}

/// Who a credit names, worked out (T-CRED-4).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Who {
    Here(String),
    Listed(Listed),
    Name(String),
}

/// A credited person as the answer carries them: an artist here by id, or Listed by its source's id; none for a name alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CreditedPerson"))]
#[cfg_attr(test, ts(export))]
#[serde(untagged)]
pub enum Person {
    Here { id: String },
    Listed { listed: Listed },
}

/// One credited person under a role (CRED.2, the answer's `credits`): who, their shown name (and number among namesakes), the
/// instrument or other detail and its qualifiers, the record's tracks (none: every track), positions the pairing doesn't place as
/// the source wrote them, and every source naming it. A Listed person's open, for the viewer, where `decide()` allows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct Credited {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub person: Option<Person>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub qualifiers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub tracks: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub at: Option<String>,
    pub by: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub open: Option<ActionView>,
}

/// One role of the Credits section: its key (a word no row names: the source's own), its shown word, its people in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CreditRole"))]
#[cfg_attr(test, ts(export))]
pub struct RoleView {
    pub role: String,
    pub word: String,
    pub people: Vec<Credited>,
}

/// The fixed order of `by` (DOMAIN-CORE's Source order, ranking nothing): the files first, then the services in the order asked.
fn source_at(source: &str) -> (usize, String) {
    if source.starts_with("files") { (0, source.to_string()) } else { (1 + asked_at(source), source.to_string()) }
}

/// One credit, gathered.
#[derive(Debug, Default)]
struct Gathered {
    every: bool,
    tracks: BTreeSet<u32>,
    unplaced: Vec<String>,
    qualifiers: BTreeSet<String>,
    by: BTreeSet<(usize, String)>,
    /// The first source's name for a Listed id or a name alone (in the fixed source order).
    names: BTreeMap<(usize, String), String>,
}

/// The record's credits by role (CRED.2, CRED.18): every source's credits merged by who they name, one credit per role, detail
/// and person, in the one order. `all` is the record's track numbers: a credit on each of them is the record's, no tracks named.
pub fn credits_of(said: &[Said], here: &Here, all: &[u32]) -> Vec<RoleView> {
    // Red first (SCENARIOS-CRED CRED.2, CRED.18): no merge yet.
    let _ = (said, here, all, source_at, Gathered::default, Here::holder, Here::named);
    Vec::new()
}

/// What an entry of a credit's ordered list is (CRED.24): a main artist, or a featured one (the Featuring role).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CreditListRole"))]
#[cfg_attr(test, ts(export))]
#[serde(rename_all = "lowercase")]
pub enum ListRole {
    Main,
    Featuring,
}

/// One entry of a source's artist credit as an ordered list (CRED.24; the owner's credit model, 2026-11-21 14:24): the artist (by
/// the source's id where it gives one), its role, and the phrase printed after it (none for a separator or the last).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<Listed>,
    pub role: ListRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phrase: Option<String>,
}

/// Whether a joining text is a list separator, never a phrase: "/" and ";" (and nothing at all).
pub fn is_separator(join: &str) -> bool {
    matches!(join.trim(), "" | "/" | ";")
}

/// Whether a joining text marks the Featuring role (T-CRED-30): "feat.", "ft." or "featuring", whole, case folded. "Feat" with no
/// dot is no marker.
pub fn is_marker(join: &str) -> bool {
    matches!(join.trim().to_lowercase().as_str(), "feat." | "ft." | "featuring")
}

/// A featuring marker as a whole word inside a line, case folded: where it starts and ends, the first one.
fn marker_in(line: &str) -> Option<(usize, usize)> {
    let lower = line.to_lowercase();
    ["feat.", "ft.", "featuring"]
        .iter()
        .filter_map(|m| {
            let mut from = 0;
            while let Some(i) = lower[from..].find(m).map(|i| i + from) {
                let end = i + m.len();
                let before = lower[..i].chars().next_back();
                let after = lower[end..].chars().next();
                // A whole word: a space before it (it never opens a line) and a space after it.
                if before.is_some_and(char::is_whitespace) && after.is_some_and(char::is_whitespace) {
                    return Some((i, end));
                }
                from = end;
            }
            None
        })
        .min()
}

/// A title's guest: "Harbour (feat. Tamsin Kell)", "(ft. …)", "(featuring …)" in round or square brackets, case folded. The
/// guest's name whole (never split on a bare "&": F54) and the marker as the title spells it.
pub fn title_guest(title: &str) -> Option<(String, String)> {
    let lower = title.to_lowercase();
    for (open, close) in [('(', ')'), ('[', ']')] {
        let mut from = 0;
        while let Some(i) = lower[from..].find(open).map(|i| i + from) {
            let inner_start = i + open.len_utf8();
            let Some(j) = lower[inner_start..].find(close).map(|j| j + inner_start) else { break };
            let inner = &title[inner_start..j];
            let folded = inner.trim_start().to_lowercase();
            for m in ["featuring ", "feat. ", "ft. "] {
                if folded.starts_with(m) {
                    let lead = inner.len() - inner.trim_start().len();
                    let marker = inner[lead..lead + m.len()].trim().to_string();
                    let guest = inner[lead + m.len()..].trim().to_string();
                    if !guest.is_empty() {
                        return Some((guest, marker));
                    }
                }
            }
            from = j + close.len_utf8();
        }
    }
    None
}

/// The ordered list of one artist credit a source gives as entries (MusicBrainz's artist credit, Discogs' `artists` with their
/// `join`): each entry's name, its id, and the joining text after it. A separator is no phrase; a featuring marker is the phrase of
/// the artist before it, and the artists after it feature (CRED.24 (p9)–(p13)).
pub fn source_list(entries: &[(String, Option<Listed>, String)]) -> Vec<Entry> {
    // Red first (SCENARIOS-CRED CRED.24): no list is kept yet.
    let _ = entries;
    Vec::new()
}

/// A file's artist credit as an ordered list (CRED.24 (p1)–(p8), (p14)–(p18)): `line` its ARTIST (or ALBUMARTIST) as written; `parts`
/// its ARTISTS list, else the line as the library's separators split it ("/" and ";", its keep list protecting a name: the
/// library's rule, `Rules::values`); `ids` its MusicBrainz artist ids in order; `title` its TITLE (none for a record's line);
/// `whole` the names the library keeps whole. The phrases are the line's text between the names, never a separator; a featuring
/// marker splits a part into its main artist and the featured one, whole words only, never inside a kept name ("Little Feat",
/// "Feat. of Clay"), the featured part one name (F54: no split on a bare "&"). A title's "(feat. B)" features B where the list
/// doesn't name B already; the title stays as written. `placeholder` is F54's placeholder rule: a featured placeholder ("Friends")
/// credits no one.
pub fn files_list(line: &str, parts: &[String], ids: &[String], title: Option<&str>, whole: &[String], placeholder: &dyn Fn(&str) -> bool) -> Vec<Entry> {
    // Red first (SCENARIOS-CRED CRED.24): no list is kept yet.
    let _ = (line, parts, ids, title, whole, placeholder, marker_in, title_guest);
    Vec::new()
}

#[cfg(test)]
mod tests;
