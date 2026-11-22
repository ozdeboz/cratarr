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
    let kind = source_kind(source);
    let name = value.trim().to_string();
    let one = |role: &str, detail: Option<String>, qualifiers: Vec<String>, name: String| Reading { role: role.to_string(), detail, qualifiers, name };
    match kind {
        "files" => {
            let w = folded(word);
            if let Some(instrument) = w.strip_prefix("performer:") {
                return vec![one("performer", Some(instrument.trim().to_string()).filter(|i| !i.is_empty()), Vec::new(), name)];
            }
            if w == "performer" {
                let (name, instrument) = performer_value(value);
                return vec![one("performer", instrument, Vec::new(), name)];
            }
            match role_of(kind, &w) {
                Some(role) => vec![one(role, None, Vec::new(), name)],
                // An ID3v2.3 TIPL or IPLS pair naming an instrument (Picard's performer pairs): a performer.
                None if is_instrument(&w) => vec![one("performer", Some(w), Vec::new(), name)],
                None => vec![one(word.trim(), None, Vec::new(), name)],
            }
        }
        "musicbrainz" => {
            let w = folded(word);
            let qualifiers: Vec<String> = attributes.iter().map(|a| folded(a)).filter(|a| QUALIFIERS.contains(&a.as_str())).collect();
            let named: Vec<String> = attributes.iter().map(|a| folded(a)).filter(|a| !QUALIFIERS.contains(&a.as_str())).collect();
            match w.as_str() {
                "instrument" if !named.is_empty() => named.into_iter().map(|i| one("performer", Some(i), qualifiers.clone(), name.clone())).collect(),
                "vocal" => {
                    let detail = named.first().cloned().unwrap_or_else(|| "vocals".to_string());
                    vec![one("performer", Some(detail), qualifiers, name)]
                }
                _ => match role_of(kind, &w) {
                    Some(role) => vec![one(role, None, qualifiers, name)],
                    None => vec![one(word.trim(), None, qualifiers, name)],
                },
            }
        }
        "discogs" => {
            let mut out = Vec::new();
            for part in discogs_parts(word) {
                let (base, bracket) = bracketed(&part);
                let b = folded(&base);
                if b == "instruments" {
                    for i in bracket.iter().flat_map(|x| x.split(',')).map(folded).filter(|i| !i.is_empty()) {
                        out.push(one("performer", Some(i), Vec::new(), name.clone()));
                    }
                } else if let Some(role) = role_of(kind, &b) {
                    match bracket.as_deref().map(folded) {
                        Some(x) if BRACKET_ROLES.iter().any(|(w, _)| *w == x) => {
                            let role = BRACKET_ROLES.iter().find(|(w, _)| *w == x).map_or(role, |(_, r)| *r);
                            out.push(one(role, None, Vec::new(), name.clone()));
                        }
                        Some(x) => out.push(one(role, None, x.split(',').map(folded).filter(|q| !q.is_empty()).collect(), name.clone())),
                        None => out.push(one(role, None, Vec::new(), name.clone())),
                    }
                } else if is_instrument(&b) {
                    let qualifiers = bracket.iter().flat_map(|x| x.split(',')).map(folded).filter(|q| !q.is_empty()).collect();
                    out.push(one("performer", Some(b), qualifiers, name.clone()));
                } else {
                    out.push(one(part.trim(), None, Vec::new(), name.clone()));
                }
            }
            out
        }
        _ => match role_of(kind, word) {
            Some(role) => vec![one(role, None, Vec::new(), name)],
            None => vec![one(word.trim(), None, Vec::new(), name)],
        },
    }
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
    // The same record's evidence first (T-CRED-4): who each name is credited as, by reference, by any source on this record.
    let mut by_name: BTreeMap<String, BTreeSet<Who>> = BTreeMap::new();
    for s in said {
        if let Some(r) = &s.person {
            let who = match here.holder(r) {
                Some(a) => Who::Here(a.id.clone()),
                None => Who::Listed(r.clone()),
            };
            by_name.entry(name_key(&s.name)).or_default().insert(who.clone());
            if let Some(c) = &s.credited_as {
                by_name.entry(name_key(c)).or_default().insert(who);
            }
        }
    }
    let who_of = |s: &Said, name: &str| -> Who {
        if let Some(r) = &s.person {
            return match here.holder(r) {
                Some(a) => Who::Here(a.id.clone()),
                None => Who::Listed(r.clone()),
            };
        }
        if let Some(ws) = by_name.get(&name_key(name))
            && ws.len() == 1
        {
            return ws.iter().next().cloned().unwrap_or_else(|| Who::Name(name_key(name)));
        }
        match here.named(name).as_slice() {
            [one] => Who::Here(one.id.clone()),
            _ => Who::Name(name_key(name)),
        }
    };
    let all_set: BTreeSet<u32> = all.iter().copied().collect();
    let mut gathered: BTreeMap<(String, Option<String>, Who), Gathered> = BTreeMap::new();
    for s in said {
        for reading in read_role(&s.source, &s.word, &s.attributes, &s.name) {
            let who = who_of(s, &reading.name);
            if reading.role == "artist"
                && let Who::Here(id) = &who
                && here.record_artists.contains(id)
            {
                continue;
            }
            let g = gathered.entry((reading.role.clone(), reading.detail.clone(), who)).or_default();
            match &s.on {
                On::Every => g.every = true,
                On::Tracks(t) => g.tracks.extend(t.iter().copied()),
                On::Unplaced(at) => {
                    if !g.unplaced.contains(at) {
                        g.unplaced.push(at.clone());
                    }
                }
            }
            g.qualifiers.extend(reading.qualifiers.iter().cloned());
            let at = source_at(&s.source);
            g.by.insert(at.clone());
            g.names.entry(at).or_insert_with(|| reading.name.clone());
        }
    }
    let mut first_of_role: BTreeMap<String, u32> = BTreeMap::new();
    let mut people: Vec<(String, u32, String, Credited)> = Vec::new();
    for ((role, detail, who), g) in gathered {
        let every = g.every || (!all_set.is_empty() && g.tracks == all_set);
        let tracks = (!every && !g.tracks.is_empty()).then(|| g.tracks.iter().copied().collect::<Vec<_>>());
        let at = (!every && g.tracks.is_empty() && !g.unplaced.is_empty()).then(|| g.unplaced.join(", "));
        let first = if every || tracks.is_none() { 0 } else { g.tracks.iter().next().copied().unwrap_or(0) };
        let first_name = g.names.values().next().cloned().unwrap_or_default();
        let (person, name, number) = match &who {
            Who::Here(id) => {
                let a = here.by_ref.iter().map(|(_, a)| a).chain(&here.by_name).find(|a| &a.id == id);
                (Some(Person::Here { id: id.clone() }), a.map_or(first_name, |a| a.name.clone()), a.and_then(|a| a.number))
            }
            Who::Listed(l) => (Some(Person::Listed { listed: l.clone() }), first_name, None),
            Who::Name(_) => (None, first_name, None),
        };
        let credited = Credited {
            person,
            name: name.clone(),
            number,
            detail,
            qualifiers: Some(g.qualifiers.into_iter().collect::<Vec<_>>()).filter(|q| !q.is_empty()),
            tracks,
            at,
            by: g.by.into_iter().map(|(_, s)| s).collect(),
            open: None,
        };
        let f = first_of_role.entry(role.clone()).or_insert(first);
        *f = (*f).min(first);
        people.push((role, first, name, credited));
    }
    // The table's roles in its order; the words no row names after them, in first-track order, then by the word.
    let mut roles: BTreeMap<(usize, u32, String), Vec<(u32, String, Credited)>> = BTreeMap::new();
    for (role, first, name, credited) in people {
        let at = role_at(&role);
        let place = if at < ROLES.len() { (at, 0, role) } else { (at, first_of_role[&role], role) };
        roles.entry(place).or_default().push((first, name_key(&name), credited));
    }
    roles
        .into_iter()
        .map(|((_, _, role), mut people)| {
            // Within a role: by first track, then shown name (CRED.18); a detail or a position apart, by it; two persons of one name
            // that nothing ties (OQ-CRED-7 (a)) in the fixed source order of their first source, which ranks nothing.
            let tie = |c: &Credited| c.by.first().map(|s| source_at(s));
            people.sort_by(|a, b| (a.0, &a.1, &a.2.detail, &a.2.at, tie(&a.2)).cmp(&(b.0, &b.1, &b.2.detail, &b.2.at, tie(&b.2))));
            RoleView { word: role_word(&role), role, people: people.into_iter().map(|(_, _, c)| c).collect() }
        })
        .collect()
}

/// What a record's credits are worked out from (the record's facts' part): every source's credits as said, the artists here they
/// may name, and the record's track numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Credits {
    pub said: Vec<Said>,
    pub here: Here,
    pub all: Vec<u32>,
}

/// The record's credits as this viewer reads them (CRED.3, CRED.12, T-CRED-9): `credits_of`, the same for every account, and each
/// Listed person's open where the viewer may make their artist: every account a stored credit names them to, but a reading key,
/// whose key only reads (CN1's H1), so the person is text there. By a MusicBrainz id, as `/api/artists/open` opens on trunk; a
/// Discogs id's open is DC3a's (M4).
pub fn credits_for(c: &Credits, viewer: &crate::can::Viewer) -> Vec<RoleView> {
    let mut roles = credits_of(&c.said, &c.here, &c.all);
    if viewer.scope != crate::can::Scope::Read {
        for p in roles.iter_mut().flat_map(|r| r.people.iter_mut()) {
            if let Some(Person::Listed { listed }) = &p.person
                && listed.source == "musicbrainz"
            {
                p.open = Some(open_artist(&listed.id, &p.name));
            }
        }
    }
    roles
}

/// A Listed person's open: `POST /api/artists/open` with their MusicBrainz id and name (T-CRED-9), made there, kept at open (Q33).
fn open_artist(mbid: &str, name: &str) -> ActionView {
    use crate::view::{Fields, Group, Method};
    ActionView {
        name: "open-artist".into(),
        title: name.to_string(),
        icon: "person".into(),
        group: Group::Go,
        section: crate::when::Section::Head,
        method: Method::Post,
        href: "/api/artists/open".into(),
        fields: Some(Fields { mbid: Some(mbid.to_string()), name: Some(name.to_string()), ..Default::default() }),
        confirm: None,
        away: false,
    }
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
    let mut out = Vec::new();
    let mut featuring = false;
    for (name, person, join) in entries {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let phrase = (!is_separator(join)).then(|| join.trim().to_string());
        out.push(Entry { name: name.to_string(), person: person.clone(), role: if featuring { ListRole::Featuring } else { ListRole::Main }, phrase });
        if is_marker(join) {
            featuring = true;
        }
    }
    if let Some(last) = out.last_mut() {
        last.phrase = None;
    }
    out
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
    let kept = |name: &str| whole.iter().any(|w| name_key(w) == name_key(name));
    // Each part, with a featuring marker inside it split (where the files give no list naming each).
    let mut names: Vec<(String, ListRole, Option<String>)> = Vec::new();
    let many = parts.len() > 1;
    for part in parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        match marker_in(part).filter(|_| !kept(part)) {
            Some((i, end)) => {
                let (main, featured) = (part[..i].trim(), part[end..].trim());
                names.push((main.to_string(), ListRole::Main, Some(part[i..end].to_string())));
                if !featured.is_empty() && !placeholder(featured) {
                    names.push((featured.to_string(), ListRole::Featuring, None));
                }
            }
            None => names.push((part.to_string(), ListRole::Main, None)),
        }
    }
    // The phrases between the names, as the line prints them, where the files list each name apart.
    if many {
        let lower = line.to_lowercase();
        let mut at = 0usize;
        let mut found: Vec<Option<(usize, usize)>> = Vec::new();
        for (name, _, _) in &names {
            let key = name.to_lowercase();
            match lower[at..].find(&key) {
                Some(i) => {
                    found.push(Some((at + i, at + i + key.len())));
                    at += i + key.len();
                }
                None => found.push(None),
            }
        }
        let mut featuring = false;
        for k in 0..names.len() {
            if featuring {
                names[k].1 = ListRole::Featuring;
            }
            if k + 1 < names.len()
                && let (Some((_, end)), Some((start, _))) = (found[k], found[k + 1])
                && start >= end
            {
                let join = line[end..start].trim();
                if names[k].2.is_none() && !is_separator(join) {
                    names[k].2 = Some(join.to_string());
                }
                if is_marker(join) {
                    featuring = true;
                }
            }
            if names[k].2.as_deref().is_some_and(is_marker) {
                featuring = true;
            }
        }
    }
    let mut out: Vec<Entry> = names
        .into_iter()
        .enumerate()
        .map(|(i, (name, role, phrase))| Entry { person: ids.get(i).map(|id| Listed { source: "musicbrainz".into(), id: id.trim().to_string() }), name, role, phrase })
        .collect();
    // A title's guest, where the list doesn't already name them (its marker the phrase of the last main artist before it).
    if let Some((guest, marker)) = title.and_then(title_guest)
        && !placeholder(&guest)
        && !out.iter().any(|e| name_key(&e.name) == name_key(&guest))
    {
        if let Some(last) = out.last_mut()
            && last.phrase.is_none()
        {
            last.phrase = Some(marker);
        }
        out.push(Entry { name: guest, person: None, role: ListRole::Featuring, phrase: None });
    }
    if let Some(last) = out.last_mut() {
        last.phrase = None;
    }
    out
}

#[cfg(test)]
mod tests;
