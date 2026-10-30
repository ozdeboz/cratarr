//! Classical, a kind of record, and a track's Other versions (0.10, the owner, 2026-10-30). Display only: the folders
//! on disk, Autoplay and the radio stay as they are (similarity decides there).
//!
//! The owner's rules:
//! - **A kind**, secondary like Compilation (a record can be an Album and Classical); Other never holds a classical
//!   record. Modern classical counts, composers playing their own work too.
//! - **A genre gate and one confirmation.** The gate: a genre among the owner's words (`GATE`), from the files' tags,
//!   MusicBrainz, or Discogs through a verified link (never a name search). The confirmation, any one of: (a) the
//!   release's recordings perform works with composers, and someone is credited as performing; (b) a composer of its
//!   works, or its album artist, is a composer confirmed classical on another record; (c) a mark by hand. The tag
//!   alone is never enough (Moby's Reprise, ELO's Time, Yann Tiersen, Pantha du Prince, Aphex Twin's Syro).
//! - **Marks** (classical, not classical) are library-wide, outlast every pass, and win over the rule both ways.
//! - **Composers** by MusicBrainz artist, so every spelling is one; a record under each of its composers. With no
//!   MusicBrainz works, its album artist is its composer, under (b) or (c) only, keyed `name:<loose name>`. Keys stay:
//!   a `name:` key later found to be a MusicBrainz artist leads there (`composer_keys`).
//! - **Other versions**: a track's work (MusicBrainz's), and the library's other tracks of the same work.
//! - **Works and movements** (0.12, decision 0086): a composer's works first (the most recorded first), a work's
//!   recordings each with its movements in the work's order, and an album's tracks under the work they are parts of.
//!   A work's parent is MusicBrainz's ("parts"); a work is top-level with no parent, or none known yet.
//!
//! Made here, in memory, from the library and the user data (`index`): kept until the library is built again or a
//! mark changes, and at most a minute behind the passes.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Serialize;
use ts_rs::TS;

use super::import::loose;
use super::works::{Person, Stored};
use crate::serve::read::{WorkAlbum, WorkSong};
use crate::source::ComposerRef;

/// The kind as the filter's Type axis and the page name it.
pub const CLASSICAL: &str = "Classical";
/// What the passes found counts at most this late (a mark, and a build, at once).
const FACTS_EVERY: Duration = Duration::from_secs(60);

/// The genres that open the gate (the owner's words, any case; "Neo-Classical" however it is hyphenated). A genre is
/// one of them as a whole: Indian Classical, or Classical Crossover, is not.
const GATE: [&str; 12] = [
    "classical",
    "opera",
    "modern classical",
    "contemporary classical",
    "neo classical",
    "neoclassical",
    "baroque",
    "romantic",
    "symphony",
    "concerto",
    "chamber music",
    "art song",
];

/// Whether a genre is one of the gate's.
pub fn classical_genre(genre: &str) -> bool {
    let key = crate::search::words(genre).join(" ");
    GATE.contains(&key.as_str())
}

/// Whether a record's genres (its files', MusicBrainz's, Discogs' by a verified link) open the gate: the tag alone is
/// never enough (`build`).
pub fn gate<'a>(genres: impl IntoIterator<Item = &'a str>) -> bool {
    genres.into_iter().any(classical_genre)
}

/// A record's mark by hand (the owner: library-wide, outlasting every pass).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Classical,
    Not,
}

impl Mark {
    pub fn of(s: &str) -> Option<Mark> {
        match s {
            "classical" => Some(Mark::Classical),
            "not" => Some(Mark::Not),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mark::Classical => "classical",
            Mark::Not => "not",
        }
    }
}

/// The kinds named on their own, by the owner's precedence: Classical, then Compilation (a Best Of is one, above
/// Album), then Album, EP and Single; Other is every other.
fn rank(kind: &str) -> Option<u8> {
    if kind.eq_ignore_ascii_case(CLASSICAL) {
        Some(0)
    } else if kind.eq_ignore_ascii_case("Compilation") {
        Some(1)
    } else if ["Album", "EP", "Single"].iter().any(|k| kind.eq_ignore_ascii_case(k)) {
        Some(2)
    } else {
        None
    }
}

/// A record's kinds in its primary view only (the owner: each record in one kind, on by default): the named kinds
/// below its first left out, the rest (Live, Soundtrack…) kept. So each record is in one named kind or in none (Other),
/// and the kinds' counts add up to the records.
pub fn primary_kinds(kinds: &[String]) -> Vec<String> {
    let best = kinds.iter().filter_map(|k| rank(k)).min();
    kinds.iter().filter(|k| rank(k).is_none_or(|r| Some(r) == best)).cloned().collect()
}

/// A record's kinds as the filter's Type axis names them: its release type's ("Album", "Live"), and Classical where it
/// is one.
pub fn kinds_of(release: &crate::source::ReleaseType) -> Vec<String> {
    let mut out: Vec<String> = release.kind.iter().chain(&release.secondary).map(|w| crate::serve::library::type_label(w)).collect();
    if release.classical == Some(true) {
        out.push(CLASSICAL.to_string());
    }
    out
}

/// What the index is made of: the library's albums and songs, and what the user data keeps (`Inputs::read`).
#[derive(Debug, Default)]
pub struct Inputs {
    pub albums: Vec<WorkAlbum>,
    pub songs: Vec<WorkSong>,
    /// By MusicBrainz release.
    pub releases: HashMap<String, Stored>,
    /// By MusicBrainz work: its title and composers.
    pub works: HashMap<String, (String, Vec<Person>)>,
    /// By MusicBrainz work: the work it is a part of, that work's title, and its number there (0.12).
    pub parents: HashMap<String, (String, String, Option<u32>)>,
    /// A performance name's person: alias → (person, the person's name).
    pub persons: HashMap<String, (String, String)>,
    /// An artist's Discogs artist, and a Discogs artist's other names (real name, aliases).
    pub artist_discogs: HashMap<String, u64>,
    pub discogs_names: HashMap<u64, Vec<String>>,
    /// Discogs' genres by a verified link (`release:<id>`, `master:<id>`).
    pub discogs_genres: HashMap<String, Vec<String>>,
    /// By the record's Music-form id.
    pub marks: HashMap<String, Mark>,
    /// Every composer key given out: (name, the key it is now).
    pub keys: HashMap<String, (String, Option<String>)>,
}

impl Inputs {
    /// From the library and the user data. Blocking.
    pub fn read(serve: &crate::serve::Serve) -> Result<Self> {
        let ud = &serve.userdata;
        let releases = ud.mb_releases()?.into_iter().filter_map(|(r, j)| Some((r, serde_json::from_str::<Stored>(&j).ok()?))).collect();
        let works = ud.mb_works()?.into_iter().map(|(w, (title, c))| (w, (title, serde_json::from_str::<Vec<Person>>(&c).unwrap_or_default()))).collect();
        Ok(Self {
            albums: serve.library.work_albums()?,
            songs: serve.library.work_songs()?,
            releases,
            works,
            parents: ud.mb_work_parents()?,
            persons: ud.persons()?,
            artist_discogs: ud.artist_discogs()?,
            discogs_names: ud.discogs_names()?,
            discogs_genres: ud.discogs_link_genres()?,
            marks: ud.classical_marks()?.into_iter().filter_map(|(a, m)| Some((a, Mark::of(&m)?))).collect(),
            keys: ud.composer_keys()?,
        })
    }
}

/// A record as Classical sees it: whether it is, its mark, its composers and performers (a classical one's), and what
/// a search result shows of it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Record {
    pub classical: bool,
    pub mark: Option<Mark>,
    pub composers: Vec<ComposerRef>,
    pub performers: Vec<String>,
    pub title: String,
    pub artist: String,
    pub year: Option<u32>,
}

/// A composer of the library's classical records (`GET /api/classical/composers`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Composer {
    pub key: String,
    pub name: String,
    /// MusicBrainz's ("Brahms, Johannes"); none for a `name:` composer.
    pub sort_name: Option<String>,
    /// How many classical records they are a composer of.
    pub records: usize,
    /// Their photo, where they are an album artist of the library.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo: Option<String>,
    /// Those records' ids, in the library's order.
    #[serde(skip)]
    pub albums: Vec<String>,
}

/// A track of a work, as Other versions and a work's recordings show it.
#[derive(Debug, Clone, Default, PartialEq)]
struct Line {
    title: String,
    artist: String,
    album: String,
    album_id: String,
    year: Option<u32>,
    /// Its place on its album (0.12: a track whose movement's number is not known keeps it), and its length.
    disc: u32,
    track: u32,
    duration_secs: Option<f64>,
}

/// A work as Other versions names it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct WorkRef {
    pub mbid: String,
    pub title: String,
}

/// Another track of the same work.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Version {
    pub source: String,
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_id: Option<String>,
    pub year: Option<u32>,
    /// Its record's performers, where MusicBrainz names them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub performers: Vec<String>,
}

/// A track's Other versions (`GET /api/tracks/{source}/{id}/versions`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Versions {
    pub work: WorkRef,
    pub composers: Vec<ComposerRef>,
    pub versions: Vec<Version>,
}

/// A work as a composer's page lists it (0.12, decision 0086; `GET /api/classical/composers/{key}/works`): its MusicBrainz id
/// and title, its musical key and catalogue number where its title says them, how many parts the library knows of it,
/// and how many of its recordings the library holds (an album each).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ComposerWork {
    pub work: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub catalogue: Option<String>,
    pub parts: usize,
    pub recordings: usize,
}

/// A part of a work (a movement), in the work's order: its number there where MusicBrainz gives one.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct WorkPart {
    pub mbid: String,
    pub title: String,
    pub number: Option<u32>,
}

/// A track of a work's recording: the part of the work it is (none: the whole work, or not known) and its length.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct WorkTrack {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub part: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub duration_secs: Option<f64>,
}

/// One recording of a work: an album's tracks of it, in the work's order (by their part's number; a track whose
/// number is not known after those, in the album's order), with the album's performers and year.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct WorkRecording {
    pub album_id: String,
    pub album: String,
    pub performers: Vec<String>,
    pub year: Option<u32>,
    pub tracks: Vec<WorkTrack>,
}

/// A work (0.12, decision 0086; `GET /api/works/{mbid}`): its composers, the work it is a part of, its parts in order,
/// and the library's recordings of it (the oldest first).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct WorkView {
    pub title: String,
    pub composers: Vec<ComposerRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent: Option<WorkRef>,
    pub parts: Vec<WorkPart>,
    pub recordings: Vec<WorkRecording>,
}

/// The work an album's track performs (0.12, decision 0086; an album's rows' `work`): the work that holds it and its
/// number there, where MusicBrainz says.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RowWork {
    pub mbid: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent: Option<WorkRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub part_number: Option<u32>,
}

/// A top-level work as search finds it (0.12, decision 0086: works reachable): its MusicBrainz id, title, catalogue
/// number where its title says one, composers, how many of its recordings the library holds (an album each), and the
/// oldest one's album (as `Index::work` orders them).
#[derive(Debug, Clone, PartialEq)]
pub struct TopWork {
    pub mbid: String,
    pub title: String,
    pub catalogue: Option<String>,
    pub composers: Vec<ComposerRef>,
    pub recordings: usize,
    pub oldest: Option<String>,
}

/// How far up a work's parents are followed (MusicBrainz's are a few deep at most: an aria, its act, its opera).
const DEPTH: usize = 16;

/// The catalogue numbers' names a work's title may end with ("op. 67", "BWV 1007", "K. 466", "Hob. XVI:52", "D 960").
const CATALOGUES: [&str; 31] = [
    "op", "opus", "bwv", "k", "kv", "d", "hob", "hwv", "rv", "woo", "s", "wwv", "trv", "bb", "sz", "l", "fp", "twv", "buxwv", "swv", "wq", "h", "wab",
    "mwv", "av", "b", "z", "jb", "kk", "m", "lwv",
];

/// A work's catalogue number, from its title as MusicBrainz writes classical works' ("Symphony no. 5 in C minor,
/// op. 67"): the first part after a comma that is a catalogue's name and a number; a nickname after it left out.
pub fn catalogue_of(title: &str) -> Option<String> {
    let head = title.split(": ").next().unwrap_or(title);
    head.split(", ").skip(1).find_map(|seg| {
        let seg = seg.split(|c: char| c == '"' || c == '\u{201c}' || c == '(').next().unwrap_or(seg).trim();
        let letters: String = seg.chars().take_while(|c| c.is_alphabetic()).collect();
        if letters.is_empty() || !CATALOGUES.contains(&letters.to_lowercase().as_str()) {
            return None;
        }
        let rest = seg[letters.len()..].trim_start_matches('.').trim_start();
        let first = rest.chars().next()?;
        (first.is_ascii_digit() || (letters.eq_ignore_ascii_case("hob") && first.is_ascii_uppercase())).then(|| seg.to_string())
    })
}

/// A work's musical key, from its title ("Symphony no. 5 in C minor" is in "C minor"; "in E-flat major" and "in E flat
/// major" as written); none where it names none.
pub fn musical_key(title: &str) -> Option<String> {
    let head = title.split(": ").next().unwrap_or(title);
    let words: Vec<&str> = head.split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty()).collect();
    for (n, w) in words.iter().enumerate() {
        if !w.eq_ignore_ascii_case("in") {
            continue;
        }
        let rest = &words[n + 1..];
        let Some(note) = rest.first() else { continue };
        let Some(letter) = note.chars().next() else { continue };
        let tail = &note[letter.len_utf8()..];
        if !('A'..='G').contains(&letter) || !matches!(tail, "" | "-flat" | "-sharp" | "\u{266d}" | "\u{266f}" | "#" | "b") {
            continue;
        }
        let mut said = vec![note.to_string()];
        if let Some(a) = rest.get(1).filter(|a| tail.is_empty() && (a.eq_ignore_ascii_case("flat") || a.eq_ignore_ascii_case("sharp"))) {
            said.push(a.to_lowercase());
        }
        let Some(mode) = rest.get(said.len()).filter(|m| m.eq_ignore_ascii_case("major") || m.eq_ignore_ascii_case("minor")) else { continue };
        said.push(mode.to_lowercase());
        return Some(said.join(" "));
    }
    None
}

/// Classical and Other versions over the whole library.
#[derive(Debug, Default)]
pub struct Index {
    /// Every album, by id.
    pub records: HashMap<String, Record>,
    /// The classical records' composers, by key.
    pub composers: Vec<Composer>,
    by_key: HashMap<String, usize>,
    /// Keys that lead to another (a `name:` found to be a MusicBrainz artist; a performance name to its person).
    aliases: HashMap<String, String>,
    /// A song's work, a work's songs, a work's title and composers, and the songs' lines.
    song_work: HashMap<String, String>,
    work_songs: HashMap<String, Vec<String>>,
    works: HashMap<String, (String, Vec<ComposerRef>)>,
    lines: HashMap<String, Line>,
    /// Each album's performers as MusicBrainz names them (any record's, for Other versions).
    performers: HashMap<String, Vec<String>>,
    /// Works and movements (0.12, decision 0086): a work's parent with its number there, a work's parts in order, and
    /// each composer's top-level works (by key), the most recorded first. `works` names the parents too.
    parent: HashMap<String, (String, Option<u32>)>,
    parts: HashMap<String, Vec<String>>,
    composer_works: HashMap<String, Vec<String>>,
    /// Every composer's top-level works once, for search (0.12, decision 0086: works reachable; `top_works`).
    top_works: Vec<TopWork>,
    /// To keep (`composer_keys`): the keys given out now (key, name) not kept yet, and keys found to lead to another.
    noted: (Vec<(String, String)>, Vec<(String, String)>),
}

impl Index {
    pub fn is_classical(&self, album: &str) -> bool {
        self.records.get(album).is_some_and(|r| r.classical)
    }

    /// A key as given out now: itself, or where an alias leads (a few steps at most); none where no classical record
    /// has that composer.
    pub fn resolve(&self, key: &str) -> Option<&str> {
        let mut k = key;
        for _ in 0..4 {
            if let Some((given, _)) = self.by_key.get_key_value(k) {
                return Some(given.as_str());
            }
            k = self.aliases.get(k).map(String::as_str)?;
        }
        None
    }

    /// A composer by their key, or by one that leads to them.
    pub fn composer(&self, key: &str) -> Option<&Composer> {
        self.resolve(key).and_then(|k| self.by_key.get(k)).map(|i| &self.composers[*i])
    }

    /// The composers whose name holds the words (search: "Brahms" finds the records he composed, whoever plays).
    pub fn composers_named(&self, words: &[String]) -> Vec<&Composer> {
        self.composers.iter().filter(|c| crate::search::matches(&c.name, words) || c.sort_name.as_deref().is_some_and(|s| crate::search::matches(s, words))).collect()
    }

    /// Whether the library holds another track of the song's work.
    pub fn has_versions(&self, song: &str) -> bool {
        self.song_work.get(song).and_then(|w| self.work_songs.get(w)).is_some_and(|s| s.len() > 1)
    }

    /// A classical record's track's composers (its work's); none for another's.
    pub fn song_composers(&self, song: &str) -> Vec<ComposerRef> {
        let classical = self.lines.get(song).is_some_and(|l| self.is_classical(&l.album_id));
        match self.song_work.get(song).and_then(|w| self.works.get(w)) {
            Some((_, c)) if classical => c.clone(),
            _ => Vec::new(),
        }
    }

    /// A song's work and the library's other tracks of it (`source`: the library's); none where it has no work.
    pub fn versions(&self, song: &str, source: &str) -> Option<Versions> {
        let w = self.song_work.get(song)?;
        let (title, composers) = self.works.get(w)?;
        let versions = self
            .work_songs
            .get(w)
            .into_iter()
            .flatten()
            .filter(|s| s.as_str() != song)
            .filter_map(|s| {
                let l = self.lines.get(s)?;
                Some(Version {
                    source: source.to_string(),
                    id: s.clone(),
                    title: l.title.clone(),
                    artist: Some(l.artist.clone()).filter(|a| !a.is_empty()),
                    album: Some(l.album.clone()),
                    album_id: Some(l.album_id.clone()),
                    year: l.year,
                    performers: self.performers.get(&l.album_id).cloned().unwrap_or_default(),
                })
            })
            .collect();
        Some(Versions { work: WorkRef { mbid: w.clone(), title: title.clone() }, composers: composers.clone(), versions })
    }

    /// The work at the top of a work's parents as far as they are known (itself, with none).
    fn top<'a>(&'a self, work: &'a str) -> &'a str {
        let mut at = work;
        for _ in 0..DEPTH {
            match self.parent.get(at) {
                Some((p, _)) => at = p.as_str(),
                None => break,
            }
        }
        at
    }

    /// A work and every work below it (its parts, theirs…).
    fn below(&self, work: &str) -> Vec<String> {
        let mut out: Vec<String> = vec![work.to_string()];
        let mut at = 0;
        while at < out.len() {
            for p in self.parts.get(&out[at]).into_iter().flatten() {
                if !out.contains(p) {
                    out.push(p.clone());
                }
            }
            at += 1;
        }
        out
    }

    /// The part of `work` that `of` is or is under (its movement); none where `of` is the work itself or not under it.
    fn part_under<'a>(&'a self, of: &'a str, work: &str) -> Option<&'a str> {
        let mut at = of;
        for _ in 0..DEPTH {
            let (p, _) = self.parent.get(at)?;
            if p == work {
                return Some(at);
            }
            at = p.as_str();
        }
        None
    }

    /// A work's tracks in the library, by album (each album once, the library's order): its own and its parts'.
    fn albums_of(&self, work: &str) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for w in self.below(work) {
            for s in self.work_songs.get(&w).into_iter().flatten() {
                let Some(l) = self.lines.get(s) else { continue };
                match out.iter_mut().find(|(a, _)| *a == l.album_id) {
                    Some((_, songs)) => songs.push(s.clone()),
                    None => out.push((l.album_id.clone(), vec![s.clone()])),
                }
            }
        }
        out
    }

    /// A composer's top-level works (0.12, decision 0086), by their key or one that leads to them, the most recorded
    /// first, then by title (numbers by their value); none where no classical record has that composer.
    pub fn composer_works(&self, key: &str) -> Option<Vec<ComposerWork>> {
        let c = self.composer(key)?;
        let works = self.composer_works.get(&c.key).into_iter().flatten().filter_map(|w| {
            let (title, _) = self.works.get(w)?;
            Some(ComposerWork {
                work: w.clone(),
                title: title.clone(),
                key: musical_key(title),
                catalogue: catalogue_of(title),
                parts: self.parts.get(w).map_or(0, Vec::len),
                recordings: self.albums_of(w).len(),
            })
        });
        Some(works.collect())
    }

    /// The top-level works search finds (0.12, decision 0086: works reachable): every composer's (`composer_works`),
    /// each once, only those the library holds a recording of; the most recorded first, then by title.
    pub fn top_works(&self) -> &[TopWork] {
        &self.top_works
    }

    /// `top_works` made (at the build's end).
    fn list_top_works(&self) -> Vec<TopWork> {
        let mut ids: Vec<&String> = self.composer_works.values().flatten().collect();
        ids.sort();
        ids.dedup();
        let mut out: Vec<TopWork> = ids
            .into_iter()
            .filter_map(|w| {
                let (title, composers) = self.works.get(w)?;
                let (recordings, oldest) = self.recorded(w);
                (recordings > 0).then(|| TopWork { mbid: w.clone(), title: title.clone(), catalogue: catalogue_of(title), composers: composers.clone(), recordings, oldest })
            })
            .collect();
        out.sort_by(|a, b| b.recordings.cmp(&a.recordings).then_with(|| super::playlists::natural(&a.title, &b.title)).then_with(|| a.mbid.cmp(&b.mbid)));
        out
    }

    /// How many recordings of a work the library holds (an album each, its parts' tracks too), and the oldest one's
    /// album, as `work` orders them (by year, one without last, then by the album's name).
    fn recorded(&self, work: &str) -> (usize, Option<String>) {
        let albums = self.albums_of(work);
        let first = |songs: &[String]| songs.first().and_then(|s| self.lines.get(s));
        let year = |album: &str, songs: &[String]| self.records.get(album).and_then(|r| r.year).or_else(|| first(songs).and_then(|l| l.year));
        let name = |songs: &[String]| first(songs).map(|l| l.album.clone()).unwrap_or_default();
        let oldest = albums
            .iter()
            .min_by(|a, b| {
                let (ya, yb) = (year(a.0.as_str(), a.1.as_slice()), year(b.0.as_str(), b.1.as_slice()));
                (ya.is_none(), ya).cmp(&(yb.is_none(), yb)).then_with(|| super::playlists::natural(&name(a.1.as_slice()), &name(b.1.as_slice())))
            })
            .map(|a| a.0.clone());
        (albums.len(), oldest)
    }

    /// A work (0.12, decision 0086): its composers, its parent, its parts in order, and the library's recordings of it,
    /// the oldest first, each one's tracks in the work's order; none where the library holds no track of it or of its
    /// parts.
    pub fn work(&self, mbid: &str) -> Option<WorkView> {
        let (title, composers) = self.works.get(mbid)?;
        let parent = self.parent.get(mbid).and_then(|(p, _)| Some(WorkRef { mbid: p.clone(), title: self.works.get(p)?.0.clone() }));
        let parts: Vec<WorkPart> = self
            .parts
            .get(mbid)
            .into_iter()
            .flatten()
            .filter_map(|p| Some(WorkPart { mbid: p.clone(), title: self.works.get(p)?.0.clone(), number: self.parent.get(p).and_then(|x| x.1) }))
            .collect();
        let mut recordings: Vec<WorkRecording> = self
            .albums_of(mbid)
            .into_iter()
            .filter_map(|(album, songs)| {
                let first = self.lines.get(songs.first()?)?;
                let mut tracks: Vec<(Option<u32>, u32, u32, WorkTrack)> = songs
                    .iter()
                    .filter_map(|s| {
                        let l = self.lines.get(s)?;
                        let part = self.part_under(self.song_work.get(s)?, mbid);
                        let number = part.and_then(|p| self.parent.get(p)).and_then(|x| x.1);
                        Some((number, l.disc, l.track, WorkTrack { id: s.clone(), part: part.map(str::to_string), duration_secs: l.duration_secs }))
                    })
                    .collect();
                tracks.sort_by(|a, b| (a.0.is_none(), a.0, a.1, a.2).cmp(&(b.0.is_none(), b.0, b.1, b.2)));
                Some(WorkRecording {
                    album: first.album.clone(),
                    performers: self.performers.get(&album).cloned().unwrap_or_default(),
                    year: self.records.get(&album).and_then(|r| r.year).or(first.year),
                    tracks: tracks.into_iter().map(|t| t.3).collect(),
                    album_id: album,
                })
            })
            .collect();
        recordings.sort_by(|a, b| (a.year.is_none(), a.year).cmp(&(b.year.is_none(), b.year)).then_with(|| super::playlists::natural(&a.album, &b.album)));
        Some(WorkView { title: title.clone(), composers: composers.clone(), parent, parts, recordings })
    }

    /// The work a track performs, with the work that holds it and its number there (0.12; an album's rows' `work`);
    /// none where it has no work linked.
    pub fn row_work(&self, song: &str) -> Option<RowWork> {
        let w = self.song_work.get(song)?;
        let (title, _) = self.works.get(w)?;
        let parent = self.parent.get(w).and_then(|(p, n)| Some((WorkRef { mbid: p.clone(), title: self.works.get(p)?.0.clone() }, *n)));
        Some(RowWork { mbid: w.clone(), title: title.clone(), part_number: parent.as_ref().and_then(|p| p.1), parent: parent.map(|p| p.0) })
    }
}

/// A name's loose forms to match a composer by: the name, the sort name ("Bartók, Béla" is "Bartók Béla" too), and the
/// family name (before the sort name's comma, else the name's last word).
fn name_forms(name: &str, sort: Option<&str>) -> Vec<String> {
    let family = match sort.and_then(|s| s.split_once(',')) {
        Some((f, _)) => f.to_string(),
        None => name.split_whitespace().last().unwrap_or_default().to_string(),
    };
    let mut out: Vec<String> = Vec::new();
    for n in [Some(name), sort, Some(family.as_str())].into_iter().flatten() {
        let l = loose(n);
        if !l.is_empty() && !out.contains(&l) {
            out.push(l);
        }
    }
    out
}

/// An album as the rule weighs it.
struct Weighed<'a> {
    album: &'a WorkAlbum,
    stored: Option<&'a Stored>,
    gate: bool,
    /// Its works' composers, most of its tracks first.
    works: Vec<ComposerRef>,
    performed: bool,
    mark: Option<Mark>,
    /// Its album artists as composers: by MusicBrainz artist where one is known, else by name.
    artists: Vec<ComposerRef>,
}

/// The index from what it is made of.
pub fn build(i: &Inputs) -> Index {
    let person = |m: &str| -> String { i.persons.get(m).map_or_else(|| m.to_string(), |p| p.0.clone()) };
    // Every MusicBrainz composer, by their person: the name (the person's, for a performance name) and sort name.
    let mut known: HashMap<String, (String, Option<String>)> = HashMap::new();
    let mut ids: Vec<&String> = i.works.keys().collect();
    ids.sort();
    for w in ids {
        for c in &i.works[w].1 {
            let key = person(&c.mbid);
            let aliased = key != c.mbid;
            let name = if aliased { i.persons.get(&c.mbid).map(|p| p.1.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| c.name.clone()) } else { c.name.clone() };
            let e = known.entry(key).or_insert_with(|| (name, None));
            if e.1.is_none() && !aliased {
                e.1 = c.sort_name.clone();
            }
        }
    }
    // Each loose form of their names (and Discogs' for them), to the composer it names alone.
    let mut forms: HashMap<String, HashSet<String>> = HashMap::new();
    for (key, (name, sort)) in &known {
        let other = i.artist_discogs.get(key).and_then(|d| i.discogs_names.get(d)).cloned().unwrap_or_default();
        for f in name_forms(name, sort.as_deref()).into_iter().chain(other.iter().map(|n| loose(n))) {
            forms.entry(f).or_default().insert(key.clone());
        }
    }
    let by_form = |l: &str| -> Option<&String> { forms.get(l).filter(|k| k.len() == 1).and_then(|k| k.iter().next()) };
    let named = |key: &str, fallback: &str| -> ComposerRef { ComposerRef { key: key.to_string(), name: known.get(key).map_or_else(|| fallback.to_string(), |k| k.0.clone()) } };

    let weighed: Vec<Weighed> = i
        .albums
        .iter()
        .map(|a| {
            let stored = a.release.as_ref().and_then(|r| i.releases.get(r));
            let linked = stored.into_iter().flat_map(|s| s.discogs.iter()).filter_map(|l| i.discogs_genres.get(l)).flatten();
            let gate = gate(a.genres.iter().chain(stored.into_iter().flat_map(|s| s.genres.iter())).chain(linked).map(String::as_str));
            // The works' composers, those of most tracks first.
            let mut counted: Vec<(String, usize)> = Vec::new();
            for w in stored.into_iter().flat_map(|s| s.tracks.iter()).flat_map(|t| t.works.iter()) {
                for c in i.works.get(w).map(|w| w.1.as_slice()).unwrap_or_default() {
                    let key = person(&c.mbid);
                    match counted.iter_mut().find(|(k, _)| *k == key) {
                        Some(e) => e.1 += 1,
                        None => counted.push((key, 1)),
                    }
                }
            }
            counted.sort_by(|a, b| b.1.cmp(&a.1));
            let works = counted.into_iter().map(|(k, _)| named(&k, "")).collect();
            // Its album artists: their MusicBrainz artist (the person, for a performance name); else their name, or
            // one of their Discogs names, where it names one composer alone; else the name itself.
            let artists = a
                .artists
                .iter()
                .filter(|x| !crate::crates::various(&x.name))
                .filter_map(|x| {
                    if let Some(m) = &x.mbid {
                        let p = person(m);
                        let name = i.persons.get(m).map(|p| p.1.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| x.name.clone());
                        return Some(named(&p, &name));
                    }
                    let l = loose(&x.name);
                    if l.is_empty() {
                        return None;
                    }
                    Some(match by_form(&l) {
                        Some(k) => named(k, &x.name),
                        None => ComposerRef { key: format!("name:{l}"), name: x.name.clone() },
                    })
                })
                .collect();
            Weighed {
                album: a,
                stored,
                gate,
                works,
                performed: stored.is_some_and(|s| s.performed),
                mark: i.marks.get(&a.base).copied(),
                artists,
            }
        })
        .collect();

    // (c) a mark, and (a) works with composers and someone performing: their composers are confirmed.
    let mut chosen: Vec<Option<Vec<ComposerRef>>> = weighed
        .iter()
        .map(|w| {
            let yes = match w.mark {
                Some(Mark::Classical) => true,
                Some(Mark::Not) => false,
                None => w.gate && !w.works.is_empty() && w.performed,
            };
            yes.then(|| if w.works.is_empty() { w.artists.clone() } else { w.works.clone() })
        })
        .collect();
    let mut confirmed: HashSet<String> = chosen.iter().flatten().flatten().map(|c| c.key.clone()).collect();
    // (b) a record within the gate whose works' composer, or album artist, is confirmed on another: confirmed in
    // turn, until none is added.
    loop {
        let mut more = false;
        for (n, w) in weighed.iter().enumerate() {
            if chosen[n].is_some() || w.mark.is_some() || !w.gate {
                continue;
            }
            let by_works = w.works.iter().any(|c| confirmed.contains(&c.key));
            let by_artist: Vec<ComposerRef> = w.artists.iter().filter(|c| confirmed.contains(&c.key)).cloned().collect();
            if !by_works && by_artist.is_empty() {
                continue;
            }
            let composers = if w.works.is_empty() { by_artist } else { w.works.clone() };
            for c in &composers {
                confirmed.insert(c.key.clone());
            }
            chosen[n] = Some(composers);
            more = true;
        }
        if !more {
            break;
        }
    }

    let mut index = Index::default();
    // The album artists' ids by their key and their loose name: a composer who is one has their photo.
    let mut artist_ids: HashMap<String, String> = HashMap::new();
    for w in &weighed {
        for (x, c) in w.album.artists.iter().filter(|x| !crate::crates::various(&x.name)).zip(&w.artists) {
            artist_ids.entry(c.key.clone()).or_insert_with(|| x.id.clone());
            artist_ids.entry(format!("name:{}", loose(&x.name))).or_insert_with(|| x.id.clone());
        }
    }
    for (w, composers) in weighed.iter().zip(chosen) {
        let a = w.album;
        let performers: Vec<String> = w.stored.map(|s| s.performers.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
        if !performers.is_empty() {
            index.performers.insert(a.id.clone(), performers.clone());
        }
        let classical = composers.is_some();
        let composers = composers.unwrap_or_default();
        for c in &composers {
            let at = *index.by_key.entry(c.key.clone()).or_insert_with(|| {
                let sort = known.get(&c.key).and_then(|k| k.1.clone());
                let photo = artist_ids.get(&c.key).or_else(|| artist_ids.get(&format!("name:{}", loose(&c.name)))).map(|id| format!("/api/artists/{id}/photo"));
                index.composers.push(Composer { key: c.key.clone(), name: c.name.clone(), sort_name: sort, records: 0, photo, albums: Vec::new() });
                index.composers.len() - 1
            });
            let e = &mut index.composers[at];
            if !e.albums.contains(&a.id) {
                e.albums.push(a.id.clone());
                e.records += 1;
            }
        }
        index.records.insert(
            a.id.clone(),
            Record {
                classical,
                mark: w.mark,
                performers: if classical { performers } else { Vec::new() },
                composers,
                title: a.name.clone(),
                artist: a.artist.clone(),
                year: a.year,
            },
        );
    }

    // Keys given out before: one kept as leading elsewhere still does; a `name:` one now naming one composer alone, or
    // a performance name now known to be a person, leads there from now on.
    for (key, (_, to)) in &i.keys {
        let to = match to {
            Some(t) => Some(t.clone()),
            None if index.by_key.contains_key(key) => None,
            None => match key.strip_prefix("name:") {
                Some(l) => by_form(l).filter(|k| index.by_key.contains_key(*k)).cloned(),
                None => Some(person(key)).filter(|p| p != key),
            },
        };
        if let Some(t) = to {
            if i.keys.get(key).is_some_and(|k| k.1.is_none()) {
                index.noted.1.push((key.clone(), t.clone()));
            }
            index.aliases.insert(key.clone(), t);
        }
    }
    index.noted.0 = index.composers.iter().filter(|c| !i.keys.contains_key(&c.key)).map(|c| (c.key.clone(), c.name.clone())).collect();

    // Other versions: each song's work, by its recording, else (with none) by its disc and track.
    let albums: HashMap<&str, &Weighed> = weighed.iter().map(|w| (w.album.id.as_str(), w)).collect();
    for s in &i.songs {
        let Some(w) = albums.get(s.album.as_str()) else { continue };
        let Some(stored) = w.stored else { continue };
        let track = match &s.recording {
            Some(r) => stored.tracks.iter().find(|t| t.recording.as_deref() == Some(r.as_str())),
            None => s.track.and_then(|n| stored.tracks.iter().find(|t| t.disc == s.disc.unwrap_or(1) && t.position == n)),
        };
        let Some(work) = track.and_then(|t| t.works.iter().find(|x| i.works.contains_key(*x))) else { continue };
        index.song_work.insert(s.id.clone(), work.clone());
        index.work_songs.entry(work.clone()).or_default().push(s.id.clone());
        index.lines.insert(
            s.id.clone(),
            Line {
                title: s.title.clone(),
                artist: s.artist.clone(),
                album: w.album.name.clone(),
                album_id: w.album.id.clone(),
                year: s.year.or(w.album.year),
                disc: s.disc.unwrap_or(1),
                track: s.track.unwrap_or(0),
                duration_secs: s.duration_secs,
            },
        );
    }
    let linked: Vec<String> = index.work_songs.keys().cloned().collect();
    for w in &linked {
        let (title, composers) = &i.works[w];
        let mut refs: Vec<ComposerRef> = Vec::new();
        for c in composers {
            let key = person(&c.mbid);
            let key = index.resolve(&key).map_or(key.clone(), str::to_string);
            if !refs.iter().any(|r| r.key == key) {
                refs.push(named(&key, &c.name));
            }
        }
        index.works.insert(w.clone(), (title.clone(), refs));
    }

    // Works and movements (0.12, decision 0086): each linked work's parent, and the parent's own, as far as MusicBrainz
    // names them (never round in a circle).
    let mut parent_titles: HashMap<String, String> = HashMap::new();
    // Level by level from the works that are recorded (in a fixed order): the parents nearer to what the library holds
    // are taken first, so a circle (which MusicBrainz should never hold) breaks at its far end, the same on every build.
    let mut first: Vec<String> = linked.clone();
    first.sort_unstable();
    let mut queue: std::collections::VecDeque<String> = first.into();
    while let Some(w) = queue.pop_front() {
        let Some((p, title, number)) = i.parents.get(&w) else { continue };
        if *p == w || title.trim().is_empty() || index.parent.contains_key(&w) {
            continue;
        }
        let mut up = p.as_str();
        let mut round = false;
        for _ in 0..DEPTH {
            if up == w {
                round = true;
                break;
            }
            match index.parent.get(up) {
                Some((q, _)) => up = q.as_str(),
                None => break,
            }
        }
        if round {
            continue;
        }
        index.parent.insert(w.clone(), (p.clone(), *number));
        index.parts.entry(p.clone()).or_default().push(w.clone());
        parent_titles.entry(p.clone()).or_insert_with(|| title.clone());
        queue.push_back(p.clone());
    }
    // A parent the library holds no track of itself: named by its title, its composers its parts'.
    let unnamed: Vec<String> = index.parts.keys().filter(|p| !index.works.contains_key(*p)).cloned().collect();
    for p in unnamed {
        let mut refs: Vec<ComposerRef> = Vec::new();
        for d in index.below(&p) {
            for c in index.works.get(&d).map(|w| w.1.as_slice()).unwrap_or_default() {
                if !refs.iter().any(|r| r.key == c.key) {
                    refs.push(c.clone());
                }
            }
        }
        let title = parent_titles.get(&p).cloned().unwrap_or_default();
        index.works.insert(p, (title, refs));
    }
    // A work's parts by their number (one without after), then by title.
    let mut parts = std::mem::take(&mut index.parts);
    for list in parts.values_mut() {
        list.sort_by(|a, b| {
            let n = |w: &String| index.parent.get(w).and_then(|p| p.1);
            let t = |w: &String| index.works.get(w).map_or("", |x| x.0.as_str());
            (n(a).is_none(), n(a)).cmp(&(n(b).is_none(), n(b))).then_with(|| super::playlists::natural(t(a), t(b))).then_with(|| a.cmp(b))
        });
    }
    index.parts = parts;
    // Each composer's top-level works: the top of each linked work's parents, under each of its composers that a
    // classical record has; the most recorded first, then by title.
    let mut tops: Vec<String> = index.work_songs.keys().map(|w| index.top(w).to_string()).collect();
    tops.sort();
    tops.dedup();
    let recorded: HashMap<String, usize> = tops.iter().map(|t| (t.clone(), index.albums_of(t).len())).collect();
    let mut by_composer: HashMap<String, Vec<String>> = HashMap::new();
    for t in &tops {
        for c in index.works.get(t).map(|w| w.1.as_slice()).unwrap_or_default() {
            if index.by_key.contains_key(&c.key) {
                let list = by_composer.entry(c.key.clone()).or_default();
                if !list.contains(t) {
                    list.push(t.clone());
                }
            }
        }
    }
    for list in by_composer.values_mut() {
        let t = |w: &String| index.works.get(w).map_or("", |x| x.0.as_str());
        list.sort_by(|a, b| recorded.get(b).cmp(&recorded.get(a)).then_with(|| super::playlists::natural(t(a), t(b))).then_with(|| a.cmp(b)));
    }
    index.composer_works = by_composer;
    // Search's works (0.12, decision 0086: works reachable).
    index.top_works = index.list_top_works();
    index
}

/// The index as last made, and what it was made from.
struct Kept {
    built: i64,
    marks: i64,
    facts: i64,
    at: Instant,
    index: Arc<Index>,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

/// What the index is made from, cheaply: the crates' facts read again when it moves (their Type axis has Classical).
pub fn stamp(serve: &crate::serve::Serve) -> i64 {
    let marks = serve.userdata.marks_stamp().unwrap_or(0);
    let facts = serve.userdata.works_stamp().unwrap_or(0);
    marks.wrapping_mul(1_000_003).wrapping_add(facts)
}

/// The index (kept until the library is built again or a mark changes; what the passes find counts within a minute).
/// Blocking.
pub fn index(serve: &crate::serve::Serve) -> Result<Arc<Index>> {
    let (built, marks, facts) = (serve.built_at(), serve.userdata.marks_stamp()?, serve.userdata.works_stamp()?);
    if let Some(k) = KEPT.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && k.built == built
        && k.marks == marks
        && (k.facts == facts || k.at.elapsed() < FACTS_EVERY)
    {
        return Ok(k.index.clone());
    }
    let index = Arc::new(build(&Inputs::read(serve)?));
    let (seen, aliases) = &index.noted;
    if !seen.is_empty() || !aliases.is_empty() {
        serve.userdata.note_composer_keys(seen, aliases, super::moment::now_secs())?;
    }
    *KEPT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Kept { built, marks, facts, at: Instant::now(), index: index.clone() });
    Ok(index)
}

/// An album as listed with what Classical says of it: whether it is (always said, for Cratarr's own library), its mark,
/// a classical one's composers and performers.
pub fn decorate(index: &Index, album: &mut crate::source::Album) {
    let r = index.records.get(&album.id);
    album.release.classical = Some(r.is_some_and(|r| r.classical));
    album.release.classical_mark = r.and_then(|r| r.mark).map(|m| m.as_str().to_string());
    if let Some(r) = r.filter(|r| r.classical) {
        album.release.composers = r.composers.clone();
        album.release.performers = r.performers.clone();
    }
}

/// Cratarr's own albums of a list said classical or not, with a classical one's composers and performers
/// (`decorate`): the Library's, Home's, an artist's; another server's say nothing.
pub async fn tell(source: &str, albums: &mut [crate::source::Album]) {
    if source != crate::source::cratarr::KIND.id || albums.is_empty() {
        return;
    }
    let Some(served) = crate::serve::instance() else { return };
    match tokio::task::spawn_blocking(move || index(&served)).await {
        Ok(Ok(ix)) => albums.iter_mut().for_each(|a| decorate(&ix, a)),
        Ok(Err(e)) => tracing::warn!("classical: not read: {e:#}"),
        Err(e) => tracing::warn!("classical: not read: {e}"),
    }
}

/// Classical as a Type value of the crates' facts (0.10, the owner: everywhere a record type is filtered): each file of
/// a classical record has it among its album's release types. Every list's filter, every crate and what draws on them
/// (the radio's and Autoplay's crates) read these. Blocking.
pub fn add_kind(serve: &crate::serve::Serve, facts: &mut [crate::crates::Facts]) -> Result<()> {
    let ix = index(serve)?;
    with_kind(&ix, &serve.library.song_album_map()?, facts);
    Ok(())
}

/// `add_kind` with the songs' albums (`album_of`: by song id).
fn with_kind(ix: &Index, album_of: &HashMap<String, String>, facts: &mut [crate::crates::Facts]) {
    for f in facts.iter_mut() {
        let classical = f.track.and_then(|t| album_of.get(&crate::serve::ids::song(t))).is_some_and(|a| ix.is_classical(a));
        if classical && !f.release_types.iter().any(|t| t == CLASSICAL) {
            f.release_types.push(CLASSICAL.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::read::WorkArtist;
    use crate::source::ReleaseType;
    use super::super::works::{Performer, Role, TrackWorks};

    fn album(id: &str, artist: &str, mbid: Option<&str>, genres: &[&str], release: Option<&str>) -> WorkAlbum {
        WorkAlbum {
            id: id.into(),
            base: id.into(),
            name: format!("{id} record"),
            artist: artist.into(),
            year: Some(1990),
            release: release.map(str::to_string),
            genres: genres.iter().map(|g| g.to_string()).collect(),
            artists: vec![WorkArtist { id: format!("ar-{artist}"), name: artist.into(), mbid: mbid.map(str::to_string) }],
        }
    }

    fn person(mbid: &str, name: &str, sort: &str) -> Person {
        Person { mbid: mbid.into(), name: name.into(), sort_name: Some(sort.into()) }
    }

    /// A release whose tracks perform these works, with these performers (a conductor, else none).
    fn release(works: &[&str], performers: &[&str]) -> Stored {
        Stored {
            performed: !performers.is_empty(),
            performers: performers.iter().map(|p| Performer { name: p.to_string(), mbid: None, role: Role::Soloist }).collect(),
            tracks: works.iter().enumerate().map(|(n, w)| TrackWorks { recording: Some(format!("rec-{w}-{n}")), disc: 1, position: n as u32 + 1, works: vec![w.to_string()] }).collect(),
            ..Stored::default()
        }
    }

    /// The library of the tests: Arrau playing Brahms (MusicBrainz's works and performers), the owner's false hits
    /// (tagged, nothing more), Indian classical, a Brahms record of the old convention (his name as album artist), and
    /// Tchaikovsky under two spellings.
    fn library() -> Inputs {
        let mut i = Inputs::default();
        i.albums = vec![
            album("al-arrau", "Claudio Arrau", Some("a-arrau"), &["Classical"], Some("r-arrau")),
            album("al-reprise", "Moby", Some("a-moby"), &["Classical", "Electronic"], Some("r-reprise")),
            album("al-time", "Electric Light Orchestra", None, &["Classical"], None),
            album("al-tiersen", "Yann Tiersen", None, &["Modern Classical"], None),
            album("al-pantha", "Pantha du Prince", None, &["Neo-Classical"], None),
            album("al-syro", "Aphex Twin", Some("a-aphex"), &["Classical"], None),
            album("al-raga", "Ravi Shankar", None, &["Indian Classical"], Some("r-raga")),
            album("al-brahms", "Brahms", None, &["Classical"], None),
            album("al-tchaik-1", "Pyotr Ilyich Tchaikovsky", None, &["Romantic"], Some("r-tchaik-1")),
            album("al-tchaik-2", "Peter Tschaikowsky", Some("a-tchaik"), &["classical"], None),
            album("al-bartok", "Bartók Béla", None, &["Classical"], None),
            album("al-bartok-mb", "Takács Quartet", None, &["Chamber Music"], Some("r-bartok")),
        ];
        i.releases = [
            ("r-arrau", release(&["w-sonata", "w-concerto"], &["Claudio Arrau"])),
            // Moby's songs link to works he composed, and nobody else is credited.
            ("r-reprise", release(&["w-porcelain"], &[])),
            ("r-raga", release(&["w-raga"], &["Ravi Shankar"])),
            ("r-tchaik-1", release(&["w-swan"], &["London Symphony Orchestra"])),
            ("r-bartok", release(&["w-quartet"], &["Takács Quartet"])),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let brahms = person("a-brahms", "Johannes Brahms", "Brahms, Johannes");
        i.works = [
            ("w-sonata", ("Piano Sonata no. 3", vec![brahms.clone()])),
            ("w-concerto", ("Piano Concerto no. 1", vec![brahms])),
            ("w-porcelain", ("Porcelain", vec![person("a-moby", "Moby", "Moby")])),
            ("w-raga", ("Raga", vec![person("a-shankar", "Ravi Shankar", "Shankar, Ravi")])),
            ("w-swan", ("Swan Lake", vec![person("a-tchaik", "Pyotr Ilyich Tchaikovsky", "Tchaikovsky, Pyotr Ilyich")])),
            ("w-quartet", ("String Quartet no. 4", vec![person("a-bartok", "Béla Bartók", "Bartók, Béla")])),
        ]
        .into_iter()
        .map(|(k, (t, c))| (k.to_string(), (t.to_string(), c)))
        .collect();
        i
    }

    fn classical(ix: &Index) -> Vec<&str> {
        let mut out: Vec<&str> = ix.records.iter().filter(|(_, r)| r.classical).map(|(k, _)| k.as_str()).collect();
        out.sort();
        out
    }

    #[test]
    fn the_gate_takes_the_owner_s_genres_whole() {
        for g in ["Classical", "opera", "Modern Classical", "Contemporary Classical", "Neo-Classical", "neoclassical", "Baroque", "Romantic", "Symphony", "Concerto", "Chamber Music", "Art Song"] {
            assert!(classical_genre(g), "{g}");
        }
        for g in ["Indian Classical", "Classical Crossover", "Electronic", "Neoclassical Darkwave", ""] {
            assert!(!classical_genre(g), "{g}");
        }
    }

    #[test]
    fn a_tag_alone_is_never_enough_and_one_confirmation_is() {
        let ix = build(&library());
        // Arrau's: tagged, its works have a composer, and he plays. Brahms (old convention) and Tchaikovsky's other
        // spelling: by (b), their composer confirmed on another record. The Takács' Bartók: (a); the Hungarian
        // spelling's record by (b) through MusicBrainz's sort name.
        assert_eq!(classical(&ix), ["al-arrau", "al-bartok", "al-bartok-mb", "al-brahms", "al-tchaik-1", "al-tchaik-2"]);
        // The owner's false hits: tagged, nothing confirms them (Moby's works are his own, nobody else credited).
        for x in ["al-reprise", "al-time", "al-tiersen", "al-pantha", "al-syro"] {
            assert!(!ix.is_classical(x), "{x}");
        }
        // Indian classical never counts, whatever confirms it.
        assert!(!ix.is_classical("al-raga"));
    }

    #[test]
    fn marks_win_both_ways_and_outlast_the_rule() {
        let mut i = library();
        i.marks.insert("al-arrau".into(), Mark::Not);
        i.marks.insert("al-pantha".into(), Mark::Classical);
        let ix = build(&i);
        assert!(!ix.is_classical("al-arrau"), "marked not: not, whatever confirms it");
        assert_eq!(ix.records["al-arrau"].mark, Some(Mark::Not));
        assert!(ix.is_classical("al-pantha"), "marked: classical, with its album artist as composer");
        assert_eq!(ix.records["al-pantha"].composers, [ComposerRef { key: "name:panthaduprince".into(), name: "Pantha du Prince".into() }]);
        // Arrau's record marked not, Brahms is confirmed nowhere: his old-convention record is not either.
        assert!(!ix.is_classical("al-brahms"));
    }

    #[test]
    fn composers_are_one_by_their_musicbrainz_artist_whatever_the_spelling() {
        let ix = build(&library());
        let tchaik = ix.composer("a-tchaik").unwrap();
        assert_eq!((tchaik.name.as_str(), tchaik.sort_name.as_deref()), ("Pyotr Ilyich Tchaikovsky", Some("Tchaikovsky, Pyotr Ilyich")));
        assert_eq!(tchaik.albums, ["al-tchaik-1", "al-tchaik-2"], "Tschaikowsky's record by his MusicBrainz artist");
        let bartok = ix.composer("a-bartok").unwrap();
        assert_eq!(bartok.albums, ["al-bartok", "al-bartok-mb"], "Bartók Béla is Béla Bartók by his sort name");
        let brahms = ix.composer("a-brahms").unwrap();
        assert_eq!((brahms.records, brahms.albums.as_slice()), (2, ["al-arrau".to_string(), "al-brahms".to_string()].as_slice()));
        assert_eq!(ix.records["al-arrau"].composers, [ComposerRef { key: "a-brahms".into(), name: "Johannes Brahms".into() }]);
        assert_eq!(ix.records["al-arrau"].performers, ["Claudio Arrau"]);
        assert_eq!(ix.composer("a-arrau"), None, "the performer is no composer");
        assert_eq!(tchaik.photo.as_deref(), Some("/api/artists/ar-Pyotr Ilyich Tchaikovsky/photo"), "an album artist of the library");
    }

    #[test]
    fn a_name_key_given_out_before_leads_to_the_musicbrainz_artist() {
        let mut i = library();
        // Before Arrau's record was read, the Brahms record was marked, and his composer key was his name.
        i.keys.insert("name:brahms".into(), ("Brahms".into(), None));
        i.keys.insert("name:old".into(), ("Old".into(), Some("a-bartok".into())));
        let ix = build(&i);
        assert_eq!(ix.resolve("name:brahms"), Some("a-brahms"));
        assert_eq!(ix.composer("name:brahms").map(|c| c.key.as_str()), Some("a-brahms"));
        assert_eq!(ix.resolve("name:old"), Some("a-bartok"), "an alias kept stays");
        assert_eq!(ix.noted.1, [("name:brahms".to_string(), "a-brahms".to_string())], "found now: kept from now on");
        assert!(ix.noted.0.iter().any(|(k, _)| k == "a-tchaik"), "new keys noted");
        assert_eq!(ix.resolve("name:nobody"), None);
        // A performance name keyed before resolves to the person it is.
        let mut i = library();
        i.persons.insert("a-tchaik-alias".into(), ("a-tchaik".into(), "Pyotr Ilyich Tchaikovsky".into()));
        i.keys.insert("a-tchaik-alias".into(), ("Tchaikovsky".into(), None));
        assert_eq!(build(&i).resolve("a-tchaik-alias"), Some("a-tchaik"));
    }

    #[test]
    fn a_performance_name_composes_as_the_person() {
        let mut i = library();
        i.persons.insert("a-aphex".into(), ("a-rdj".into(), "Richard D. James".into()));
        i.marks.insert("al-syro".into(), Mark::Classical);
        let ix = build(&i);
        assert_eq!(ix.records["al-syro"].composers, [ComposerRef { key: "a-rdj".into(), name: "Richard D. James".into() }]);
    }

    #[test]
    fn each_record_is_in_one_kind_in_its_primary_view_and_the_kinds_add_up() {
        let k = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(primary_kinds(&k(&["Album", "Compilation", "Classical"])), ["Classical"]);
        assert_eq!(primary_kinds(&k(&["Album", "Compilation", "Live"])), ["Compilation", "Live"], "a Best Of is a Compilation; Live kept");
        assert_eq!(primary_kinds(&k(&["EP", "Live"])), ["EP", "Live"]);
        assert_eq!(primary_kinds(&k(&["Live"])), ["Live"]);
        let records = [k(&["Album"]), k(&["Album", "Compilation"]), k(&["Album", "Classical"]), k(&["EP"]), k(&["Single", "Compilation", "Classical"]), k(&["Live"]), k(&[])];
        let named = ["Classical", "Compilation", "Album", "EP", "Single"];
        let primary: Vec<Vec<String>> = records.iter().map(|r| primary_kinds(r)).collect();
        let count = |kind: &str| primary.iter().filter(|p| p.iter().any(|x| x == kind)).count();
        let other = primary.iter().filter(|p| !p.iter().any(|x| named.contains(&x.as_str()))).count();
        assert_eq!(named.iter().map(|n| count(n)).sum::<usize>() + other, records.len());
        assert_eq!((count("Classical"), count("Compilation"), count("Album"), other), (2, 1, 1, 2));
        // Without it, a record is under each kind it has.
        assert_eq!(records.iter().filter(|r| r.iter().any(|x| x == "Album")).count(), 3);
    }

    #[test]
    fn a_record_s_kinds_are_its_release_type_and_classical() {
        let r = ReleaseType { kind: Some("album".into()), secondary: vec!["compilation".into()], classical: Some(true), ..Default::default() };
        assert_eq!(kinds_of(&r), ["Album", "Compilation", "Classical"]);
        assert_eq!(kinds_of(&ReleaseType { classical: Some(false), ..Default::default() }), Vec::<String>::new());
    }

    #[test]
    fn other_versions_are_the_library_s_tracks_of_the_same_work() {
        let mut i = library();
        // A second recording of the sonata, Zimerman's, on its own release; and a track of no work.
        i.albums.push(album("al-zimerman", "Krystian Zimerman", None, &["Classical"], Some("r-zimerman")));
        i.releases.insert("r-zimerman".into(), release(&["w-sonata"], &["Krystian Zimerman"]));
        let song = |id: &str, album: &str, recording: Option<&str>, track: u32| WorkSong {
            id: id.into(),
            album: album.into(),
            recording: recording.map(str::to_string),
            disc: None,
            track: Some(track),
            title: format!("{id} title"),
            artist: "Someone".into(),
            year: None,
            duration_secs: None,
        };
        i.songs = vec![song("so-arrau-1", "al-arrau", Some("rec-w-sonata-0"), 1), song("so-zim-1", "al-zimerman", None, 1), song("so-arrau-9", "al-arrau", Some("rec-other"), 9)];
        let ix = build(&i);
        let v = ix.versions("so-arrau-1", "cratarr").unwrap();
        assert_eq!((v.work.mbid.as_str(), v.work.title.as_str()), ("w-sonata", "Piano Sonata no. 3"));
        assert_eq!(v.composers, [ComposerRef { key: "a-brahms".into(), name: "Johannes Brahms".into() }]);
        assert_eq!(v.versions.len(), 1);
        assert_eq!((v.versions[0].id.as_str(), v.versions[0].album_id.as_deref(), v.versions[0].year), ("so-zim-1", Some("al-zimerman"), Some(1990)));
        assert_eq!(v.versions[0].performers, ["Krystian Zimerman"]);
        assert!(ix.has_versions("so-arrau-1") && ix.has_versions("so-zim-1"));
        // A track of no work: nothing, and no action.
        assert!(ix.versions("so-arrau-9", "cratarr").is_none());
        assert!(!ix.has_versions("so-arrau-9"));
        assert_eq!(ix.song_composers("so-arrau-1")[0].key, "a-brahms");
    }

    /// The library of the works' tests (0.12): Arrau's record holds Brahms' first piano concerto as its three movements
    /// and the third sonata whole; Zimerman's the concerto's first two movements, the second first, and the sonata.
    fn works_library() -> Inputs {
        let mut i = library();
        i.albums.push(album("al-zimerman", "Krystian Zimerman", None, &["Classical"], Some("r-zimerman")));
        i.releases.insert("r-arrau".into(), release(&["w-pc1-1", "w-pc1-2", "w-pc1-3", "w-sonata"], &["Claudio Arrau"]));
        i.releases.insert("r-zimerman".into(), release(&["w-pc1-2", "w-pc1-1", "w-sonata"], &["Krystian Zimerman"]));
        let brahms = person("a-brahms", "Johannes Brahms", "Brahms, Johannes");
        let concerto = "Piano Concerto no. 1 in D minor, op. 15";
        for (n, name) in [(1, "I. Maestoso"), (2, "II. Adagio"), (3, "III. Rondo. Allegro non troppo")] {
            let id = format!("w-pc1-{n}");
            i.works.insert(id.clone(), (format!("{concerto}: {name}"), vec![brahms.clone()]));
            i.parents.insert(id, ("w-pc1".into(), concerto.into(), Some(n)));
        }
        let song = |id: &str, album: &str, recording: Option<&str>, track: u32| WorkSong {
            id: id.into(),
            album: album.into(),
            recording: recording.map(str::to_string),
            disc: None,
            track: Some(track),
            title: format!("{id} title"),
            artist: "Someone".into(),
            year: None,
            duration_secs: None,
        };
        i.songs = vec![
            song("so-a1", "al-arrau", Some("rec-w-pc1-1-0"), 1),
            song("so-a2", "al-arrau", Some("rec-w-pc1-2-1"), 2),
            song("so-a3", "al-arrau", Some("rec-w-pc1-3-2"), 3),
            song("so-a4", "al-arrau", Some("rec-w-sonata-3"), 4),
            song("so-z1", "al-zimerman", None, 1),
            song("so-z2", "al-zimerman", None, 2),
            song("so-z3", "al-zimerman", None, 3),
        ];
        i.songs[0].duration_secs = Some(1312.5);
        i
    }

    #[test]
    fn a_composer_s_works_are_their_top_level_works_the_most_recorded_first() {
        let ix = build(&works_library());
        let works = ix.composer_works("a-brahms").unwrap();
        let listed: Vec<(&str, usize, usize)> = works.iter().map(|w| (w.work.as_str(), w.parts, w.recordings)).collect();
        // Both on two records: by title. The concerto's movements are no works of their own on his page.
        assert_eq!(listed, [("w-pc1", 3, 2), ("w-sonata", 0, 2)]);
        assert_eq!((works[0].title.as_str(), works[0].key.as_deref(), works[0].catalogue.as_deref()), ("Piano Concerto no. 1 in D minor, op. 15", Some("D minor"), Some("op. 15")));
        assert_eq!((works[1].key.as_deref(), works[1].catalogue.as_deref()), (None, None));
        assert_eq!(ix.composer_works("a-nobody"), None);
        // One more recording of the sonata puts it first.
        let mut i = works_library();
        i.albums.push(album("al-gilels", "Emil Gilels", None, &["Classical"], Some("r-gilels")));
        i.releases.insert("r-gilels".into(), release(&["w-sonata"], &["Emil Gilels"]));
        i.songs.push(WorkSong { id: "so-g1".into(), album: "al-gilels".into(), track: Some(1), ..WorkSong::default() });
        let first = build(&i).composer_works("a-brahms").unwrap()[0].clone();
        assert_eq!((first.work.as_str(), first.recordings), ("w-sonata", 3));
    }

    #[test]
    fn a_work_s_recordings_play_their_movements_in_the_work_s_order() {
        let ix = build(&works_library());
        let w = ix.work("w-pc1").unwrap();
        assert_eq!(w.title, "Piano Concerto no. 1 in D minor, op. 15");
        assert_eq!(w.composers, [ComposerRef { key: "a-brahms".into(), name: "Johannes Brahms".into() }], "its movements' composer");
        assert_eq!(w.parent, None);
        let parts: Vec<(&str, Option<u32>)> = w.parts.iter().map(|p| (p.mbid.as_str(), p.number)).collect();
        assert_eq!(parts, [("w-pc1-1", Some(1)), ("w-pc1-2", Some(2)), ("w-pc1-3", Some(3))]);
        assert_eq!(w.parts[0].title, "Piano Concerto no. 1 in D minor, op. 15: I. Maestoso");
        let recordings: Vec<(&str, Vec<&str>)> = w.recordings.iter().map(|r| (r.album_id.as_str(), r.tracks.iter().map(|t| t.id.as_str()).collect())).collect();
        // Zimerman's second track is the first movement: played first.
        assert_eq!(recordings, [("al-arrau", vec!["so-a1", "so-a2", "so-a3"]), ("al-zimerman", vec!["so-z2", "so-z1"])]);
        assert_eq!(w.recordings[0].performers, ["Claudio Arrau"]);
        assert_eq!((w.recordings[0].album.as_str(), w.recordings[0].year), ("al-arrau record", Some(1990)));
        assert_eq!(w.recordings[1].tracks[0].part.as_deref(), Some("w-pc1-1"));
        // As the page reads it: a part's number always said, a track's part and length only where known.
        let v = serde_json::to_value(&w).unwrap();
        assert!(v.get("parent").is_none());
        assert_eq!(v["parts"][0]["number"], 1);
        assert_eq!(v["recordings"][0]["tracks"][0], serde_json::json!({ "id": "so-a1", "part": "w-pc1-1", "duration_secs": 1312.5 }));
        assert_eq!(v["recordings"][0]["tracks"][1], serde_json::json!({ "id": "so-a2", "part": "w-pc1-2" }));
        // A movement opened on its own: its symphony above it, its recordings.
        let m = ix.work("w-pc1-2").unwrap();
        assert_eq!(m.parent, Some(WorkRef { mbid: "w-pc1".into(), title: "Piano Concerto no. 1 in D minor, op. 15".into() }));
        assert!(m.parts.is_empty());
        assert_eq!(m.recordings.iter().map(|r| r.tracks[0].id.as_str()).collect::<Vec<_>>(), ["so-a2", "so-z1"]);
        assert_eq!(m.recordings[0].tracks[0].part, None, "the work itself");
        assert_eq!(ix.work("w-nothing"), None);
    }

    #[test]
    fn an_album_s_rows_name_their_work_and_the_work_that_holds_it() {
        let ix = build(&works_library());
        let row = ix.row_work("so-a2").unwrap();
        assert_eq!((row.mbid.as_str(), row.part_number), ("w-pc1-2", Some(2)));
        assert_eq!(row.parent.as_ref().map(|p| p.mbid.as_str()), Some("w-pc1"));
        let whole = serde_json::to_value(ix.row_work("so-a4").unwrap()).unwrap();
        assert_eq!(whole, serde_json::json!({ "mbid": "w-sonata", "title": "Piano Sonata no. 3" }), "a whole work: no parent, no number");
        assert_eq!(ix.row_work("so-nothing"), None);
    }

    #[test]
    fn parents_that_go_round_in_a_circle_end() {
        let mut i = works_library();
        i.parents.insert("w-pc1".into(), ("w-pc1-1".into(), "Round".into(), None));
        let ix = build(&i);
        assert_eq!(ix.top("w-pc1-2"), "w-pc1");
        assert_eq!(ix.composer_works("a-brahms").unwrap().len(), 2);
    }

    #[test]
    fn a_title_says_its_catalogue_number_and_its_key() {
        assert_eq!(catalogue_of("Symphony no. 5 in C minor, op. 67").as_deref(), Some("op. 67"));
        assert_eq!(catalogue_of("Cello Suite no. 1 in G major, BWV 1007: I. Prélude").as_deref(), Some("BWV 1007"));
        assert_eq!(catalogue_of("Piano Sonata in E-flat major, Hob. XVI:52").as_deref(), Some("Hob. XVI:52"));
        assert_eq!(catalogue_of("Symphony no. 41 in C major, K. 551 \"Jupiter\"").as_deref(), Some("K. 551"));
        assert_eq!(catalogue_of("Piano Sonata no. 21 in B-flat major, D. 960").as_deref(), Some("D. 960"));
        assert_eq!(catalogue_of("Requiem, Op. posth."), None);
        assert_eq!(catalogue_of("Porcelain"), None);
        assert_eq!(musical_key("Symphony no. 5 in C minor, op. 67").as_deref(), Some("C minor"));
        assert_eq!(musical_key("Piano Sonata in E-flat major, Hob. XVI:52").as_deref(), Some("E-flat major"));
        assert_eq!(musical_key("Symphony in E flat Major").as_deref(), Some("E flat major"));
        assert_eq!(musical_key("Mass in B Minor, BWV 232").as_deref(), Some("B minor"));
        assert_eq!(musical_key("Concerto in D"), None);
        assert_eq!(musical_key("Born in the USA"), None);
    }

    #[test]
    fn a_crate_s_type_condition_knows_classical() {
        use crate::crates::{Condition, Facts, Value, World};
        let ix = build(&library());
        let album_of: HashMap<String, String> = [(crate::serve::ids::song(1), "al-arrau".to_string()), (crate::serve::ids::song(2), "al-time".to_string())].into();
        let mut facts = [Facts { rel: "a/1.flac".into(), track: Some(1), release_types: vec!["Album".into()], ..Facts::default() }, Facts { rel: "b/1.flac".into(), track: Some(2), release_types: vec!["Album".into()], ..Facts::default() }];
        with_kind(&ix, &album_of, &mut facts);
        assert_eq!(facts[0].release_types, ["Album", "Classical"]);
        assert_eq!(facts[1].release_types, ["Album"], "ELO's Time, tagged only: no");
        let membership = HashMap::new();
        let w = World { membership: &membership, favourites: None, favourite_albums: None, uses: None, now: 0 };
        let classical = || Value::AnyOf { values: vec!["classical".into()] };
        let not = [Condition::new("type", true, classical())];
        let only = [Condition::new("type", false, classical())];
        let kept = |c: &[Condition]| facts.iter().filter(|f| crate::crates::passes(&[*f], c, &w)).map(|f| f.rel.as_str()).collect::<Vec<_>>();
        assert_eq!(kept(&not), ["b/1.flac"], "Type is not Classical leaves the classical record's tracks out");
        assert_eq!(kept(&only), ["a/1.flac"]);
        // The Type axis offers it, with its count.
        let items: Vec<[&Facts; 1]> = facts.iter().map(|f| [f]).collect();
        let lists: Vec<&[&Facts]> = items.iter().map(|i| i.as_slice()).collect();
        let axes = crate::crates::list_axes(&lists, &[], None, None);
        let t = axes.iter().find(|a| a.key == "type").unwrap();
        let at = t.values.iter().position(|v| v == "Classical").unwrap();
        assert_eq!(t.counts[at], 1);
    }

    #[test]
    fn a_composer_is_found_by_their_name() {
        let ix = build(&library());
        let found: Vec<&str> = ix.composers_named(&crate::search::words("brahms")).iter().map(|c| c.key.as_str()).collect();
        assert_eq!(found, ["a-brahms"]);
        assert!(ix.composer("a-brahms").unwrap().albums.contains(&"al-arrau".to_string()), "Arrau's record, Brahms not its album artist");
    }

    #[test]
    fn a_listed_album_says_what_classical_says_of_it() {
        let ix = build(&library());
        let mut a = crate::source::Album {
            id: "al-arrau".into(),
            title: "x".into(),
            artist: None,
            year: None,
            added: None,
            path: None,
            tracks: 1,
            format: None,
            bitrate_kbps: None,
            duration_secs: None,
            tags: Vec::new(),
            library: None,
            artists: Vec::new(),
            release: Default::default(),
        };
        decorate(&ix, &mut a);
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["classical"], true);
        assert_eq!(v["composers"], serde_json::json!([{ "key": "a-brahms", "name": "Johannes Brahms" }]));
        assert_eq!(v["performers"], serde_json::json!(["Claudio Arrau"]));
        assert!(v.get("classical_mark").is_none());
        a.id = "al-time".into();
        a.release = Default::default();
        decorate(&ix, &mut a);
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!((v["classical"].clone(), v.get("composers")), (serde_json::json!(false), None));
    }
}
