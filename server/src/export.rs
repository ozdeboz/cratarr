//! Export (decision 0007; the panel's Export tab, decision 0025; one step for every object, decision 0058): playlists,
//! or any list of tracks (an album, an artist's, a selection, a queue), as one download: the files as they are, hi-res
//! ones at CD quality (decision 0013's converter) or lossless ones as Opus (decision 0033's copies) or MP3 (320 kb/s or
//! V0), in one folder or in Artist/Album folders, with their playlist files or not. `plan` resolves the playlists'
//! lines to files in the Music folder and `plan_files` takes tracks already found (each file once, with its size and
//! what the catalogue knows of it; missing ones kept apart for the report), `layout` decides the names inside the
//! bundle and the playlists' lines, and `stream` writes the ZIP as it goes (stored, sizes and CRCs in data
//! descriptors, ZIP64 where a size or an offset needs it), one file at a time, so memory does not depend on the
//! export's size. With every file as it is, its exact length is known before the first byte, so the browser shows real
//! progress; a converted file's size is known only once it is made, so then the length is not told. Several files are
//! a ZIP, one file is that file (the owner's rule, decision 0058): `download` decides which, and the lone file's name,
//! from the names `Names` gives inside the bundle.

use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

/// What the catalogue knows of a file: its first artist, title, album and album artist tags, and its format ("FLAC",
/// "MP3", …), bit rate (kb/s) and length (ms) as read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub format: Option<String>,
    pub kbps: Option<u32>,
    pub duration_ms: Option<u64>,
}

/// A file in the export: where it is read, its path in its library (the library's layout and the report name it so),
/// its size, and what the catalogue knows of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub path: PathBuf,
    pub rel: String,
    pub size: u64,
    pub facts: Facts,
}

/// The playlists (by name, each its lines as indexes into `tracks`), every file once, and the lines whose file is
/// missing (playlist, line), which are left out.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub lists: Vec<(String, Vec<usize>)>,
    pub tracks: Vec<Track>,
    pub missing: Vec<(String, String)>,
}

/// Where the tracks go inside the bundle: their paths in the library, one folder (`Artist - Title.ext`), or a folder
/// per album artist and one per album in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Arrange {
    #[default]
    Library,
    Flat,
    Folders,
}

/// The files as they are; hi-res lossless files at CD quality (16-bit FLAC at 44.1 kHz); or lossless files as Opus at
/// this many kb/s, or as MP3 at a constant 320 kb/s or at V0. Lossy files always go as they are: a second lossy pass
/// only loses more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Original,
    FlacCd,
    Opus(u32),
    Mp3Cbr320,
    Mp3V0,
}

pub use crate::stream_forms::Mp3Kind;

/// About what V0 comes to on music (kb/s): LAME's own figure for its `-V 0` is 220 to 260.
const V0_KBPS: u64 = 245;

/// The Opus bit rates offered (kb/s).
pub const OPUS_RATES: [u32; 4] = [96, 128, 160, 256];

#[derive(Debug, Clone)]
pub struct Options {
    /// Bundle the tracks.
    pub tracks: bool,
    pub arrange: Arrange,
    /// One playlist of this name instead of each (every track once, at its first place).
    pub combine: Option<String>,
    pub format: Format,
    /// Write the playlist files (`.m3u`); a list of tracks may go without one.
    pub lists: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { tracks: false, arrange: Arrange::Library, combine: None, format: Format::Original, lists: true }
    }
}

/// What the export will hold, before starting. `bytes` only with tracks; `estimate` when it is about (a converted
/// file's size is known only once it is made).
#[derive(Debug, Serialize, PartialEq)]
pub struct Preview {
    pub tracks: usize,
    pub missing: usize,
    pub bytes: Option<u64>,
    pub estimate: bool,
}

/// Lines whose path stays inside the folder (no `..`, not absolute).
fn inside(rel: &str) -> bool {
    let p = Path::new(rel);
    !rel.is_empty() && !p.is_absolute() && p.components().all(|c| matches!(c, Component::Normal(_)))
}

/// Resolves the playlists' lines (relative to `music`) to files. `facts` gives what the catalogue knows of each file.
pub fn plan(music: &Path, lists: Vec<(String, Vec<String>)>, facts: impl FnOnce(&[String]) -> Result<Vec<Facts>>) -> Result<Plan> {
    let mut index = std::collections::HashMap::<String, usize>::new();
    let mut rels: Vec<(String, u64)> = Vec::new();
    let mut missing = Vec::new();
    let mut out = Vec::new();
    for (name, lines) in lists {
        let mut at = Vec::new();
        for line in lines {
            if let Some(&i) = index.get(&line) {
                at.push(i);
                continue;
            }
            let size = inside(&line).then(|| std::fs::metadata(music.join(&line)).ok()).flatten().filter(|m| m.is_file()).map(|m| m.len());
            match size {
                Some(size) => {
                    index.insert(line.clone(), rels.len());
                    at.push(rels.len());
                    rels.push((line, size));
                }
                None => missing.push((name.clone(), line)),
            }
        }
        out.push((name, at));
    }
    let names: Vec<String> = rels.iter().map(|(r, _)| r.clone()).collect();
    let mut known = facts(&names)?.into_iter();
    let tracks = rels
        .into_iter()
        .map(|(rel, size)| Track { path: music.join(&rel), rel, size, facts: known.next().unwrap_or_default() })
        .collect();
    Ok(Plan { lists: out, tracks, missing })
}

/// One list of tracks already found (an album, an artist's, a selection, a queue): each its file (where it is read, its
/// path in its library, what is known of it), or for one not found what names it in the report. A file named twice
/// is bundled once; one gone since it was found is missing too.
pub fn plan_files(name: &str, found: Vec<std::result::Result<(PathBuf, String, Facts), String>>) -> Plan {
    let mut index = std::collections::HashMap::<PathBuf, usize>::new();
    let (mut tracks, mut at, mut missing) = (Vec::new(), Vec::new(), Vec::new());
    for f in found {
        match f {
            Ok((path, rel, facts)) => {
                if let Some(&i) = index.get(&path) {
                    at.push(i);
                    continue;
                }
                match std::fs::metadata(&path).ok().filter(|m| m.is_file()) {
                    Some(m) => {
                        index.insert(path.clone(), tracks.len());
                        at.push(tracks.len());
                        tracks.push(Track { path, rel, size: m.len(), facts });
                    }
                    None => missing.push((name.to_string(), rel)),
                }
            }
            Err(label) => missing.push((name.to_string(), label)),
        }
    }
    Plan { lists: vec![(name.to_string(), at)], tracks, missing }
}

/// The playlists as written: combined into one (each track once, at its first place), or as they are.
fn lists(plan: &Plan, o: &Options) -> Vec<(String, Vec<usize>)> {
    match &o.combine {
        Some(name) => {
            let mut seen = std::collections::HashSet::new();
            let all = plan.lists.iter().flat_map(|(_, at)| at.iter().copied()).filter(|i| seen.insert(*i)).collect();
            vec![(name.clone(), all)]
        }
        None => plan.lists.clone(),
    }
}

/// The tracks the written playlists hold, each once, in the order they first appear.
fn used(lists: &[(String, Vec<usize>)]) -> Vec<usize> {
    let mut seen = std::collections::HashSet::new();
    lists.iter().flat_map(|(_, at)| at.iter().copied()).filter(|i| seen.insert(*i)).collect()
}

/// What a file is, for the format chosen: lossless (and whether above CD format), or anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Lossless { hires: bool },
    Other,
}

/// What happens to one file: it goes as it is, or is converted to CD-quality FLAC, to Opus at this many kb/s, or to
/// MP3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Treatment {
    AsIs,
    CdFlac,
    Opus(u32),
    Mp3(Mp3Kind),
}

/// Per file: CD quality converts only hi-res lossless files; Opus and MP3 every lossless one; anything else goes as it
/// is.
pub fn treatment(format: Format, kind: Kind) -> Treatment {
    match (format, kind) {
        (Format::FlacCd, Kind::Lossless { hires: true }) => Treatment::CdFlac,
        (Format::Opus(kbps), Kind::Lossless { .. }) => Treatment::Opus(kbps),
        (Format::Mp3Cbr320, Kind::Lossless { .. }) => Treatment::Mp3(Mp3Kind::Cbr320),
        (Format::Mp3V0, Kind::Lossless { .. }) => Treatment::Mp3(Mp3Kind::V0),
        _ => Treatment::AsIs,
    }
}

/// What a file is, from its headers, by the converter's own test (FLAC, WAV, AIFF, ALAC). Blocking.
pub fn kind_of(path: &Path) -> Kind {
    match crate::convert::Source::open(path) {
        Ok(Some(s)) => Kind::Lossless { hires: crate::convert::is_hires(s.format.sample_rate, s.format.bits_per_sample) },
        _ => Kind::Other,
    }
}

/// CD audio's own rate, kb/s (44.1 kHz × 16 bit × 2): a CD-format FLAC is never above it.
const CD_KBPS: f64 = 1411.2;

/// What a file is, by the catalogue's facts alone, for the preview (no file is opened): FLAC, WAV and AIFF are
/// lossless, and so is an MP4 at 500 kb/s or more (ALAC; AAC stays far below); a lossless file above CD audio's rate
/// is above CD format. A hi-res file that compresses below it counts as CD format here.
pub fn kind_guessed(f: &Facts) -> Kind {
    let kbps = f.kbps.unwrap_or(0);
    let lossless = match f.format.as_deref() {
        Some("FLAC" | "WAV" | "AIFF") => true,
        Some("MP4") => kbps >= 500,
        _ => false,
    };
    if lossless { Kind::Lossless { hires: f64::from(kbps) > CD_KBPS } } else { Kind::Other }
}

/// About how large a file goes out, and whether that is a guess: as it is, its size; at CD quality a hi-res file as
/// many kb/s as `cd_kbps` (the export's own CD-format lossless files', else CD audio's rate), never more than it was;
/// as Opus or MP3 a lossless file its bit rate for its length (V0 about 245 kb/s; its tags and cover not counted). By
/// the catalogue's facts alone; a converted file of unknown length counts as its size.
pub fn size_out(t: &Track, format: Format, cd_kbps: f64) -> (u64, bool) {
    if format == Format::Original {
        return (t.size, false);
    }
    size_as(t, treatment(format, kind_guessed(&t.facts)), cd_kbps).unwrap_or((t.size, false))
}

/// About how large a file goes out with this treatment (as `size_out` says); `None` when it converts and its length
/// is not known.
fn size_as(t: &Track, how: Treatment, cd_kbps: f64) -> Option<(u64, bool)> {
    let ms = t.facts.duration_ms;
    Some(match how {
        Treatment::AsIs => (t.size, false),
        Treatment::CdFlac => (((cd_kbps * ms? as f64 / 8.0) as u64).min(t.size), true),
        Treatment::Opus(kbps) => (u64::from(kbps) * ms? / 8, true),
        Treatment::Mp3(Mp3Kind::Cbr320) => (320 * ms? / 8, true),
        Treatment::Mp3(Mp3Kind::V0) => (V0_KBPS * ms? / 8, true),
    })
}

/// The CD-format lossless files' rate among these tracks (kb/s, by their total size over their total length), else CD
/// audio's own.
fn cd_kbps(tracks: &[&Track]) -> f64 {
    let (bytes, ms) = tracks
        .iter()
        .filter(|t| kind_guessed(&t.facts) == Kind::Lossless { hires: false })
        .filter_map(|t| Some((t.size, t.facts.duration_ms.filter(|d| *d > 0)?)))
        .fold((0u64, 0u64), |(b, m), (size, d)| (b + size, m + d));
    if ms == 0 { CD_KBPS } else { (bytes as f64 * 8.0 / ms as f64).min(CD_KBPS) }
}

pub fn preview(plan: &Plan, o: &Options) -> Preview {
    let written = lists(plan, o);
    let used = used(&written);
    let missing = if o.combine.is_some() {
        plan.missing.iter().map(|(_, l)| l).collect::<std::collections::HashSet<_>>().len()
    } else {
        plan.missing.len()
    };
    let tracks: Vec<&Track> = used.iter().map(|&i| &plan.tracks[i]).collect();
    let rate = cd_kbps(&tracks);
    let sizes: Vec<(u64, bool)> = tracks.iter().map(|t| size_out(t, o.format, rate)).collect();
    Preview {
        tracks: used.len(),
        missing,
        bytes: o.tracks.then(|| sizes.iter().map(|(b, _)| b).sum()),
        estimate: o.tracks && sizes.iter().any(|(_, e)| *e),
    }
}

/// A name that is safe as one file name on any system: no separators or reserved characters, no control characters,
/// no trailing dots or spaces.
pub fn safe_name(s: &str) -> String {
    let cleaned: String = s.chars().map(|c| if c.is_control() || r#"/\:*?"<>|"#.contains(c) { '_' } else { c }).collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    if trimmed.is_empty() { "Untitled".into() } else { trimmed }
}

/// `name` if it is free (ignoring case), else `stem (2).ext`, `stem (3).ext`…
fn unique(name: String, taken: &mut std::collections::HashSet<String>) -> String {
    if taken.insert(name.to_lowercase()) {
        return name;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 && !name[i..].contains('/') => (&name[..i], &name[i..]),
        _ => (name.as_str(), ""),
    };
    (2..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|c| taken.insert(c.to_lowercase()))
        .expect("an unused name")
}

/// A file's extension as it is (`flac`), or none.
fn extension(rel: &str) -> &str {
    Path::new(rel).extension().and_then(|e| e.to_str()).unwrap_or("")
}

/// The extension a file goes out with.
fn extension_out<'a>(t: &'a Track, how: Treatment) -> &'a str {
    match how {
        Treatment::AsIs => extension(&t.rel),
        Treatment::CdFlac => "flac",
        Treatment::Opus(_) => "opus",
        Treatment::Mp3(_) => "mp3",
    }
}

/// Each track's name inside the bundle, one at a time (a name taken stays taken).
struct Namer {
    arrange: Arrange,
    taken: std::collections::HashSet<String>,
}

impl Namer {
    fn new(arrange: Arrange) -> Self {
        Self { arrange, taken: std::collections::HashSet::new() }
    }

    /// Its path in the library; `Artist - Title` in one folder; or `Album artist/Album/` and its file's own name. With
    /// the extension it goes out with.
    fn name(&mut self, t: &Track, ext: &str) -> String {
        let dot = if ext.is_empty() { String::new() } else { format!(".{ext}") };
        let stem = Path::new(&t.rel).file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled");
        let name = match self.arrange {
            Arrange::Library => {
                let dir = t.rel.rfind('/').map(|i| &t.rel[..=i]).unwrap_or("");
                format!("{dir}{stem}{dot}")
            }
            Arrange::Flat => {
                let named = match (&t.facts.artist, &t.facts.title) {
                    (Some(a), Some(ti)) => format!("{a} - {ti}"),
                    (None, Some(ti)) => ti.clone(),
                    _ => stem.to_string(),
                };
                format!("{}{dot}", safe_name(&named))
            }
            Arrange::Folders => {
                let artist = t.facts.album_artist.as_ref().or(t.facts.artist.as_ref()).map(|a| safe_name(a)).unwrap_or_else(|| "Unknown artist".into());
                // Without an album tag, the folder it is in.
                let album = t.facts.album.as_deref().map(safe_name).or_else(|| {
                    Path::new(&t.rel).parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).map(safe_name)
                });
                match album {
                    Some(album) => format!("{artist}/{album}/{}{dot}", safe_name(stem)),
                    None => format!("{artist}/{}{dot}", safe_name(stem)),
                }
            }
        };
        unique(name, &mut self.taken)
    }
}

/// What goes in the bundle: the tracks (name inside, track), the playlist files (name, contents) and the report of
/// missing tracks.
#[derive(Debug, PartialEq)]
pub struct Layout {
    pub files: Vec<(String, usize)>,
    pub lists: Vec<(String, String)>,
    pub report: Option<String>,
}

/// The names inside the bundle, decided in this one place for the ZIP, its length beforehand and a file that goes
/// alone: each track in turn (`track`, with the extension it goes out with), then the playlist files, whose lines name
/// the tracks so, and the report (`rest`).
struct Names<'a> {
    plan: &'a Plan,
    o: &'a Options,
    written: Vec<(String, Vec<usize>)>,
    namer: Namer,
    inside: std::collections::HashMap<usize, String>,
}

impl<'a> Names<'a> {
    fn new(plan: &'a Plan, o: &'a Options) -> Self {
        Self { plan, o, written: lists(plan, o), namer: Namer::new(o.arrange), inside: std::collections::HashMap::new() }
    }

    /// The tracks, in the bundle's order (each once).
    fn order(&self) -> Vec<usize> {
        used(&self.written)
    }

    /// Track `i`'s name inside the bundle, going out with this extension; without tracks, its path in the library (the
    /// playlists' lines). Once for each track, in `order`.
    fn track(&mut self, i: usize, ext: &str) -> String {
        let t = &self.plan.tracks[i];
        let name = if self.o.tracks { self.namer.name(t, ext) } else { t.rel.clone() };
        self.inside.insert(i, name.clone());
        name
    }

    /// After every track: the playlist files (name, contents) and the report, which also names the tracks `kept` as
    /// they were.
    fn rest(self, kept: &[(String, String)]) -> (Vec<(String, String)>, Option<String>) {
        let mut lists = Vec::new();
        if self.o.lists {
            let mut list_names = std::collections::HashSet::new();
            for (name, at) in &self.written {
                let body: String = at.iter().map(|i| format!("{}\r\n", self.inside[i])).collect();
                lists.push((unique(format!("{}.m3u", safe_name(name)), &mut list_names), body));
            }
        }
        (lists, report(self.plan, self.o, kept))
    }
}

/// The bundle with every file as it is.
pub fn layout(plan: &Plan, o: &Options) -> Layout {
    let mut names = Names::new(plan, o);
    let mut files = Vec::new();
    for i in names.order() {
        let name = names.track(i, extension(&plan.tracks[i].rel));
        if o.tracks {
            files.push((name, i));
        }
    }
    let (lists, report) = names.rest(&[]);
    Layout { files, lists, report }
}

/// A content type for a file of the export, by its extension.
pub fn content_type(path: &str) -> &'static str {
    match extension(path).to_ascii_lowercase().as_str() {
        "zip" => "application/zip",
        "m3u" => "audio/x-mpegurl",
        "txt" => "text/plain; charset=utf-8",
        ext => crate::serve::api::content_type(ext),
    }
}

/// The report: the tracks left out, and those that went as they are because they could not be converted (`kept`:
/// path in the library, why).
fn report(plan: &Plan, o: &Options, kept: &[(String, String)]) -> Option<String> {
    let mut r = String::new();
    if o.tracks && !plan.missing.is_empty() {
        r.push_str("These tracks were left out: their files are missing from the music folder.\r\n\r\n");
        for (p, line) in &plan.missing {
            r.push_str(&format!("{p}: {line}\r\n"));
        }
    }
    if !kept.is_empty() {
        if !r.is_empty() {
            r.push_str("\r\n");
        }
        r.push_str("These tracks are as they were: they could not be converted.\r\n\r\n");
        for (rel, why) in kept {
            r.push_str(&format!("{rel}: {why}\r\n"));
        }
    }
    (!r.is_empty()).then_some(r)
}

pub const REPORT: &str = "Missing tracks.txt";

/// What an export downloads as (decision 0058, the owner's rule: several files are a zip, one file is that file).
#[derive(Debug, Clone, PartialEq)]
pub enum Download {
    /// Two files or more: one ZIP of them, named after the export.
    Zip { name: String },
    /// A playlist file, or the report, alone: its name and contents.
    Text { name: String, body: String },
    /// One track alone: where it is in the plan, what happens to it, and the name it goes out with.
    Track { at: usize, how: Treatment, name: String },
}

impl Download {
    /// The download's file name: the ZIP's, or the one file's own.
    pub fn name(&self) -> &str {
        match self {
            Download::Zip { name } | Download::Text { name, .. } | Download::Track { name, .. } => name,
        }
    }
}

/// What an export downloads as, decided in this one place for the name told when it is prepared and for what is sent:
/// the files its bundle would hold are counted (its tracks, its playlist files, the report of missing tracks), and
/// when that is exactly one, it goes alone under its own name, the last part of its name in the bundle. A lone track's
/// treatment is chosen here from what `kind` says of it (`kind_of` reads its header, so this blocks then), so the
/// extension it goes out with is known before it is converted; with the files as they are, nothing is opened.
pub fn download(plan: &Plan, o: &Options, kind: impl FnOnce(&Track) -> Kind) -> Download {
    let written = lists(plan, o);
    let order = used(&written);
    let report = report(plan, o, &[]);
    let files = (if o.tracks { order.len() } else { 0 }) + (if o.lists { written.len() } else { 0 }) + usize::from(report.is_some());
    if files != 1 {
        let base = match (&o.combine, written.as_slice()) {
            (Some(n), _) => safe_name(n),
            (None, [(n, _)]) => safe_name(n),
            _ => "Playlists".into(),
        };
        return Download::Zip { name: format!("{base}.zip") };
    }
    // The last part of a name in the bundle.
    fn own(name: &str) -> String {
        name.rsplit('/').next().unwrap_or(name).to_string()
    }
    if o.tracks && order.len() == 1 {
        let at = order[0];
        let t = &plan.tracks[at];
        let how = if o.format == Format::Original { Treatment::AsIs } else { treatment(o.format, kind(t)) };
        let name = Names::new(plan, o).track(at, extension_out(t, how));
        return Download::Track { at, how, name: own(&name) };
    }
    match layout(plan, o).lists.into_iter().next() {
        Some((name, body)) => Download::Text { name: own(&name), body },
        None => Download::Text { name: REPORT.into(), body: report.unwrap_or_default() },
    }
}

// The ZIP format (APPNOTE 6.3): stored entries, each a local header, the data and a data descriptor (the CRC is known
// only after reading); the central directory and its end, with ZIP64 records where 32 bits do not hold a value.

const MAX32: u64 = 0xFFFF_FFFF;

const CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

/// CRC-32 (as ZIP uses it), continued over chunks: start with 0.
pub fn crc32(crc: u32, data: &[u8]) -> u32 {
    let mut c = !crc;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

/// An entry being written: where its local header starts, and whether its sizes need 64 bits.
pub struct Entry {
    name: String,
    offset: u64,
    zip64: bool,
}

/// The writer's bookkeeping; every method returns the bytes to send next.
pub struct Zip {
    offset: u64,
    central: Vec<u8>,
    count: u64,
    time: u16,
    date: u16,
}

impl Zip {
    pub fn new(time: u16, date: u16) -> Self {
        Self { offset: 0, central: Vec::new(), count: 0, time, date }
    }

    /// The local header of an entry of about `size` bytes (its CRC and exact size follow in its descriptor).
    pub fn begin(&mut self, name: &str, size: u64) -> (Vec<u8>, Entry) {
        let zip64 = size >= MAX32;
        let mut h = Vec::with_capacity(30 + name.len() + 20);
        h.extend(0x0403_4b50u32.to_le_bytes());
        h.extend((if zip64 { 45u16 } else { 20u16 }).to_le_bytes());
        h.extend(0x0808u16.to_le_bytes()); // sizes in the descriptor; UTF-8 names
        h.extend(0u16.to_le_bytes()); // stored
        h.extend(self.time.to_le_bytes());
        h.extend(self.date.to_le_bytes());
        h.extend(0u32.to_le_bytes());
        let size32 = if zip64 { MAX32 as u32 } else { 0 };
        h.extend(size32.to_le_bytes());
        h.extend(size32.to_le_bytes());
        h.extend((name.len() as u16).to_le_bytes());
        h.extend((if zip64 { 20u16 } else { 0 }).to_le_bytes());
        h.extend(name.as_bytes());
        if zip64 {
            h.extend(1u16.to_le_bytes());
            h.extend(16u16.to_le_bytes());
            h.extend(0u64.to_le_bytes());
            h.extend(0u64.to_le_bytes());
        }
        let e = Entry { name: name.to_string(), offset: self.offset, zip64 };
        self.offset += h.len() as u64;
        (h, e)
    }

    /// After `size` bytes of data with this CRC: the entry's descriptor (and its record for the central directory).
    pub fn finish(&mut self, e: Entry, crc: u32, size: u64) -> Vec<u8> {
        self.offset += size;
        let mut d = Vec::with_capacity(24);
        d.extend(0x0807_4b50u32.to_le_bytes());
        d.extend(crc.to_le_bytes());
        if e.zip64 {
            d.extend(size.to_le_bytes());
            d.extend(size.to_le_bytes());
        } else {
            d.extend((size as u32).to_le_bytes());
            d.extend((size as u32).to_le_bytes());
        }
        self.offset += d.len() as u64;

        let big_size = size >= MAX32;
        let big_offset = e.offset >= MAX32;
        let mut extra = Vec::new();
        if big_size || big_offset {
            let mut fields = Vec::new();
            if big_size {
                fields.extend(size.to_le_bytes());
                fields.extend(size.to_le_bytes());
            }
            if big_offset {
                fields.extend(e.offset.to_le_bytes());
            }
            extra.extend(1u16.to_le_bytes());
            extra.extend((fields.len() as u16).to_le_bytes());
            extra.extend(fields);
        }
        let c = &mut self.central;
        c.extend(0x0201_4b50u32.to_le_bytes());
        c.extend((0x0300u16 | 45).to_le_bytes()); // made by: Unix, 4.5
        c.extend((if e.zip64 || big_size || big_offset { 45u16 } else { 20u16 }).to_le_bytes());
        c.extend(0x0808u16.to_le_bytes());
        c.extend(0u16.to_le_bytes());
        c.extend(self.time.to_le_bytes());
        c.extend(self.date.to_le_bytes());
        c.extend(crc.to_le_bytes());
        let s32 = if big_size { MAX32 as u32 } else { size as u32 };
        c.extend(s32.to_le_bytes());
        c.extend(s32.to_le_bytes());
        c.extend((e.name.len() as u16).to_le_bytes());
        c.extend((extra.len() as u16).to_le_bytes());
        c.extend(0u16.to_le_bytes()); // comment
        c.extend(0u16.to_le_bytes()); // disk
        c.extend(0u16.to_le_bytes()); // internal attributes
        c.extend((0o100644u32 << 16).to_le_bytes()); // a regular file, rw-r--r--
        c.extend((if big_offset { MAX32 as u32 } else { e.offset as u32 }).to_le_bytes());
        c.extend(e.name.as_bytes());
        c.extend(extra);
        self.count += 1;
        d
    }

    /// A small entry whose bytes are at hand (a playlist, the report): header, data and descriptor.
    pub fn whole(&mut self, name: &str, data: &[u8]) -> Vec<u8> {
        let (mut out, e) = self.begin(name, data.len() as u64);
        out.extend(data);
        let d = self.finish(e, crc32(0, data), data.len() as u64);
        out.extend(d);
        out
    }

    /// The central directory and its end.
    pub fn end(self) -> Vec<u8> {
        let (cd_offset, cd_size, n) = (self.offset, self.central.len() as u64, self.count);
        let mut out = self.central;
        let zip64 = n >= 0xFFFF || cd_offset >= MAX32 || cd_size >= MAX32;
        if zip64 {
            let at = cd_offset + cd_size;
            out.extend(0x0606_4b50u32.to_le_bytes());
            out.extend(44u64.to_le_bytes());
            out.extend((0x0300u16 | 45).to_le_bytes());
            out.extend(45u16.to_le_bytes());
            out.extend(0u32.to_le_bytes());
            out.extend(0u32.to_le_bytes());
            out.extend(n.to_le_bytes());
            out.extend(n.to_le_bytes());
            out.extend(cd_size.to_le_bytes());
            out.extend(cd_offset.to_le_bytes());
            out.extend(0x0706_4b50u32.to_le_bytes());
            out.extend(0u32.to_le_bytes());
            out.extend(at.to_le_bytes());
            out.extend(1u32.to_le_bytes());
        }
        let n16 = if n >= 0xFFFF { 0xFFFF } else { n as u16 };
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(n16.to_le_bytes());
        out.extend(n16.to_le_bytes());
        out.extend((cd_size.min(MAX32) as u32).to_le_bytes());
        out.extend((cd_offset.min(MAX32) as u32).to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }
}

/// DOS time and date (ZIP's), from seconds since 1970 (UTC).
pub fn dos_time(unix: u64) -> (u16, u16) {
    let days = (unix / 86_400) as i64;
    let secs = unix % 86_400;
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let year = year.clamp(1980, 2107);
    let time = ((secs / 3600) << 11 | (secs % 3600 / 60) << 5 | (secs % 60) / 2) as u16;
    let date = (((year - 1980) << 9) | (month << 5) | day) as u16;
    (time, date)
}

/// The ZIP's exact length with every file as it is, before writing it: its tracks, then its playlists and report.
fn length(plan: &Plan, layout: &Layout) -> u64 {
    let mut z = Zip::new(0, 0);
    let mut n = 0u64;
    let mut add = |name: &str, size: u64| {
        let (h, e) = z.begin(name, size);
        n += h.len() as u64 + size;
        n += z.finish(e, 0, size).len() as u64;
    };
    for (name, i) in &layout.files {
        add(name, plan.tracks[*i].size);
    }
    for (name, body) in &layout.lists {
        add(name, body.len() as u64);
    }
    if let Some(r) = &layout.report {
        add(REPORT, r.len() as u64);
    }
    n + z.end().len() as u64
}

pub type Chunk = std::result::Result<axum::body::Bytes, std::io::Error>;

/// A converted file in the work folder, removed once it has been sent (or the export stopped).
struct Made(PathBuf);

impl Drop for Made {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Converts one track into the work folder, one conversion at a time per core the copies may take
/// (`opus_copy::COPIES`). `None` when there was nothing to convert after all.
fn convert(t: &Track, how: Treatment, work: &Path, rt: &tokio::runtime::Handle) -> Result<Option<Made>> {
    use anyhow::Context;
    if how == Treatment::AsIs {
        return Ok(None);
    }
    let _permit = rt.block_on(crate::opus_copy::COPIES.acquire()).context("the copies' permits are closed")?;
    std::fs::create_dir_all(work).with_context(|| format!("creating {}", work.display()))?;
    let mut id = [0u8; 8];
    getrandom::fill(&mut id).map_err(|e| anyhow::anyhow!("random: {e}"))?;
    let id: String = id.iter().map(|b| format!("{b:02x}")).collect();
    let made = Made(work.join(format!("export-{id}.{}", extension_out(t, how))));
    match how {
        Treatment::AsIs => {}
        Treatment::CdFlac => {
            crate::convert::to_cd_flac_told(&t.path, &made.0, |_| {})?;
        }
        Treatment::Opus(kbps) => {
            let file = std::fs::File::create(&made.0).with_context(|| format!("creating {}", made.0.display()))?;
            let mut out = std::io::BufWriter::new(file);
            if !crate::opus_copy::copy_at(&t.path, &mut out, (None, None), kbps as i32 * 1000)? {
                return Ok(None);
            }
            std::io::Write::flush(&mut out)?;
        }
        Treatment::Mp3(kind) => {
            if !crate::stream_forms::mp3_file(&t.path, &made.0, kind)? {
                return Ok(None);
            }
        }
    }
    Ok(Some(made))
}

/// The one track of an export that downloads as that file alone (`Download::Track`): converted first into `work` when
/// `how` says so (as the ZIP's are, with the same permits), else read where it is. Its exact length, and the receiving
/// end of its bytes; a converted file is removed once sent (or the download stopped). A conversion that fails, or
/// finds nothing to convert, is an error: the file is not sent as it is under a name that says otherwise, and a file
/// alone has no report to say why. Blocking (the conversion): run on a blocking thread.
pub fn send_track(t: &Track, how: Treatment, work: &Path, rt: &tokio::runtime::Handle) -> Result<(u64, tokio::sync::mpsc::Receiver<Chunk>)> {
    use std::io::Read;
    let made = match how {
        Treatment::AsIs => None,
        _ => {
            sweep(work);
            Some(convert(t, how, work, rt)?.ok_or_else(|| anyhow::anyhow!("{} has nothing to convert", t.rel))?)
        }
    };
    let path = made.as_ref().map_or_else(|| t.path.clone(), |m| m.0.clone());
    let mut file = std::fs::File::open(&path)?;
    let size = file.metadata()?.len();
    let rel = t.rel.clone();
    let (tx, rx) = tokio::sync::mpsc::channel::<Chunk>(4);
    rt.spawn_blocking(move || {
        let _made = made;
        let (mut left, mut buf) = (size, vec![0u8; 256 * 1024]);
        while left > 0 {
            let want = left.min(buf.len() as u64) as usize;
            let chunk = match file.read(&mut buf[..want]) {
                Ok(0) => Err(std::io::Error::other(format!("{rel} changed during the export"))),
                Ok(n) => Ok(axum::body::Bytes::copy_from_slice(&buf[..n])),
                Err(e) => Err(e),
            };
            let failed = chunk.is_err();
            if let Ok(b) = &chunk {
                left -= b.len() as u64;
            }
            if tx.blocking_send(chunk).is_err() || failed {
                return;
            }
        }
    });
    Ok((size, rx))
}

/// Files the work folder still holds from an export that stopped half-way (Cratarr was stopped): older than an hour,
/// so never one being made.
fn sweep(work: &Path) {
    let Ok(dir) = std::fs::read_dir(work) else { return };
    let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    for e in dir.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < hour_ago);
        if old && e.file_name().to_string_lossy().starts_with("export-") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Writes the ZIP into a channel as it goes (one file's chunk at a time): the tracks, each converted first when the
/// format asks (into `work`, then sent), then the playlists, whose lines name the tracks as they went in, and the
/// report. Returns its exact length when every file goes as it is, and the receiving end. A track that goes as it is
/// is read for exactly the size measured in the plan: a file that changed meanwhile ends the download with an error
/// rather than a broken bundle. One that could not be converted goes as it is, and the report says so. A closed
/// channel (the download was cancelled) stops it.
pub fn stream(plan: Plan, o: Options, work: PathBuf, now_unix: u64) -> (Option<u64>, tokio::sync::mpsc::Receiver<Chunk>) {
    let len = (o.format == Format::Original || !o.tracks).then(|| length(&plan, &layout(&plan, &o)));
    let (tx, rx) = tokio::sync::mpsc::channel::<Chunk>(4);
    let rt = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let send = |b: Vec<u8>| tx.blocking_send(Ok(axum::body::Bytes::from(b))).is_ok();
        let fail = |e: std::io::Error| {
            let _ = tx.blocking_send(Err(e));
        };
        let (time, date) = dos_time(now_unix);
        let mut z = Zip::new(time, date);
        let mut names = Names::new(&plan, &o);
        let mut kept = Vec::new();
        if o.format != Format::Original {
            sweep(&work);
        }
        for i in names.order() {
            let t = &plan.tracks[i];
            if !o.tracks {
                names.track(i, "");
                continue;
            }
            let how = if o.format == Format::Original { Treatment::AsIs } else { treatment(o.format, kind_of(&t.path)) };
            let made = match convert(t, how, &work, &rt) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("Export: converting {}: {e:#}", t.path.display());
                    kept.push((t.rel.clone(), format!("{e:#}")));
                    None
                }
            };
            let (path, size, ext) = match &made {
                Some(m) => match std::fs::metadata(&m.0) {
                    Ok(meta) => (m.0.clone(), meta.len(), extension_out(t, how)),
                    Err(e) => return fail(e),
                },
                None => (t.path.clone(), t.size, extension(&t.rel)),
            };
            // Named once it is made: one that could not be converted keeps its own extension.
            let name = names.track(i, ext);
            if !entry(&mut z, &name, &path, size, &t.rel, &send, &fail) {
                return;
            }
            drop(made);
        }
        let (lists, report) = names.rest(&kept);
        for (name, body) in lists {
            if !send(z.whole(&name, body.as_bytes())) {
                return;
            }
        }
        if let Some(r) = report
            && !send(z.whole(REPORT, r.as_bytes()))
        {
            return;
        }
        let _ = send(z.end());
    });
    (len, rx)
}

/// One file's entry: its header, exactly `size` bytes of it, its descriptor. False when the download stopped or failed.
fn entry(z: &mut Zip, name: &str, path: &Path, size: u64, rel: &str, send: &impl Fn(Vec<u8>) -> bool, fail: &impl Fn(std::io::Error)) -> bool {
    use std::io::Read;
    let (h, e) = z.begin(name, size);
    if !send(h) {
        return false;
    }
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(err) => {
            fail(err);
            return false;
        }
    };
    let (mut crc, mut left) = (0u32, size);
    let mut buf = vec![0u8; 256 * 1024];
    while left > 0 {
        let want = left.min(buf.len() as u64) as usize;
        match file.read(&mut buf[..want]) {
            Ok(0) => {
                fail(std::io::Error::other(format!("{rel} changed during the export")));
                return false;
            }
            Ok(n) => {
                crc = crc32(crc, &buf[..n]);
                left -= n as u64;
                if !send(buf[..n].to_vec()) {
                    return false;
                }
            }
            Err(err) => {
                fail(err);
                return false;
            }
        }
    }
    send(z.finish(e, crc, size))
}

/// Exports prepared for a download (decision 0058: a list of tracks is too long for an address): each by a random
/// token, for `LIFE` seconds from when it was made, at most `MOST` at once (the oldest go first).
pub struct Tokens<T> {
    held: std::collections::HashMap<String, (u64, T)>,
}

impl<T: Clone> Tokens<T> {
    pub const LIFE: u64 = 600;
    pub const MOST: usize = 64;

    pub fn new() -> Self {
        Self { held: std::collections::HashMap::new() }
    }

    pub fn put(&mut self, token: String, now: u64, v: T) {
        self.held.retain(|_, (made, _)| now < *made + Self::LIFE);
        while self.held.len() >= Self::MOST {
            let Some(oldest) = self.held.iter().min_by_key(|(_, (made, _))| *made).map(|(k, _)| k.clone()) else { break };
            self.held.remove(&oldest);
        }
        self.held.insert(token, (now, v));
    }

    /// The export of this token while it lasts (a download may be asked for again, as a browser does on a retry).
    pub fn get(&self, token: &str, now: u64) -> Option<T> {
        self.held.get(token).filter(|(made, _)| now < *made + Self::LIFE).map(|(_, v)| v.clone())
    }
}

/// A new token: 16 random bytes, as hex.
pub fn new_token() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("the operating system's random source");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn music() -> tempfile_dir::Dir {
        tempfile_dir::Dir::new()
    }

    /// A throwaway folder under the system's temp folder.
    mod tempfile_dir {
        pub struct Dir(pub std::path::PathBuf);
        impl Dir {
            pub fn new() -> Self {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let p = std::env::temp_dir().join(format!("cratarr-export-{}-{}-{n}", std::process::id(), rand()));
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
            pub fn put(&self, rel: &str, bytes: &[u8]) {
                let p = self.0.join(rel);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, bytes).unwrap();
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        fn rand() -> u128 {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        }
    }

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn named(artist: &str, title: &str) -> Facts {
        Facts { artist: Some(artist.into()), title: Some(title.into()), ..Facts::default() }
    }

    fn facts(rels: &[String]) -> Result<Vec<Facts>> {
        Ok(rels
            .iter()
            .map(|r| match r.as_str() {
                "A - Alb/01 One.flac" => named("A", "One"),
                "B - Alb/01 One.mp3" => named("A", "One"),
                _ => Facts::default(),
            })
            .collect())
    }

    fn sample() -> (tempfile_dir::Dir, Plan) {
        let d = music();
        d.put("A - Alb/01 One.flac", b"one-flac");
        d.put("A - Alb/02 Two.flac", b"two");
        d.put("B - Alb/01 One.mp3", b"one-mp3!");
        let plan = plan(
            &d.0,
            vec![
                ("Chill".into(), lines(&["A - Alb/01 One.flac", "Gone/x.mp3", "A - Alb/02 Two.flac"])),
                ("Jazz/Soul".into(), lines(&["A - Alb/02 Two.flac", "B - Alb/01 One.mp3", "../outside.mp3"])),
            ],
            facts,
        )
        .unwrap();
        (d, plan)
    }

    /// The whole ZIP, and the length told before it.
    fn zipped(plan: Plan, o: Options, work: PathBuf) -> (Option<u64>, Vec<u8>) {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let (len, mut rx) = stream(plan, o, work, 1_791_000_000);
            let mut all = Vec::new();
            while let Some(c) = rx.recv().await {
                all.extend_from_slice(&c.unwrap());
            }
            (len, all)
        })
    }

    /// No file is opened: every file goes as it is, or there is more than one.
    fn unopened(_: &Track) -> Kind {
        panic!("no file is opened")
    }

    /// One track's bytes as `send_track` sends them, and the length it told.
    fn sent(t: &Track, how: Treatment, work: &Path) -> Result<(u64, Vec<u8>)> {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
        let (t, work, h) = (t.clone(), work.to_path_buf(), rt.handle().clone());
        rt.block_on(async move {
            let (len, mut rx) = tokio::task::spawn_blocking(move || send_track(&t, how, &work, &h)).await.unwrap()?;
            let mut all = Vec::new();
            while let Some(c) = rx.recv().await {
                all.extend_from_slice(&c?);
            }
            Ok::<_, anyhow::Error>((len, all))
        })
    }

    fn names(zip: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>) -> Vec<String> {
        (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().to_string()).collect()
    }

    fn text(zip: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str) -> String {
        let mut s = String::new();
        zip.by_name(name).unwrap().read_to_string(&mut s).unwrap();
        s
    }

    #[test]
    fn a_plan_has_each_file_once_and_keeps_missing_or_outside_lines_apart() {
        let (d, p) = sample();
        assert_eq!(p.tracks.iter().map(|t| t.rel.as_str()).collect::<Vec<_>>(), ["A - Alb/01 One.flac", "A - Alb/02 Two.flac", "B - Alb/01 One.mp3"]);
        assert_eq!(p.lists, vec![("Chill".to_string(), vec![0, 1]), ("Jazz/Soul".to_string(), vec![1, 2])]);
        assert_eq!(p.missing, vec![("Chill".into(), "Gone/x.mp3".into()), ("Jazz/Soul".into(), "../outside.mp3".into())]);
        assert_eq!(p.tracks[0].size, 8);
        assert_eq!(p.tracks[0].path, d.0.join("A - Alb/01 One.flac"));
    }

    #[test]
    fn a_list_of_tracks_has_each_file_once_and_keeps_those_not_found_apart() {
        let d = music();
        d.put("Alb/1.flac", b"1");
        d.put("Alb/2.mp3", b"22");
        let at = |rel: &str| Ok((d.0.join(rel), rel.to_string(), Facts::default()));
        let p = plan_files("Queue", vec![at("Alb/1.flac"), Err("cratarr tr-9".into()), at("Alb/2.mp3"), at("Alb/1.flac"), at("Alb/gone.flac")]);
        assert_eq!(p.tracks.iter().map(|t| (t.rel.as_str(), t.size)).collect::<Vec<_>>(), [("Alb/1.flac", 1), ("Alb/2.mp3", 2)]);
        assert_eq!(p.lists, vec![("Queue".to_string(), vec![0, 1, 0])]);
        assert_eq!(p.missing, vec![("Queue".into(), "cratarr tr-9".into()), ("Queue".into(), "Alb/gone.flac".into())]);
        let o = Options { tracks: true, lists: false, ..Options::default() };
        assert_eq!(download(&p, &o, unopened), Download::Zip { name: "Queue.zip".into() }, "two tracks and the report");
        assert!(layout(&p, &o).lists.is_empty());
        assert_eq!(preview(&p, &o), Preview { tracks: 2, missing: 2, bytes: Some(3), estimate: false });
    }

    #[test]
    fn without_tracks_the_lines_are_relative_to_the_music_folder_and_one_playlist_is_a_plain_m3u() {
        let (_d, p) = sample();
        let o = Options::default();
        let l = layout(&p, &o);
        assert!(l.files.is_empty() && l.report.is_none());
        assert_eq!(l.lists[0], ("Chill.m3u".into(), "A - Alb/01 One.flac\r\nA - Alb/02 Two.flac\r\n".into()));
        assert_eq!(l.lists[1].0, "Jazz_Soul.m3u", "a name that is not a safe file name");
        assert_eq!(download(&p, &o, unopened), Download::Zip { name: "Playlists.zip".into() }, "two playlists: a zip");
        let one = Plan { lists: p.lists[..1].to_vec(), ..p.clone() };
        assert_eq!(
            download(&one, &o, unopened),
            Download::Text { name: "Chill.m3u".into(), body: "A - Alb/01 One.flac\r\nA - Alb/02 Two.flac\r\n".into() },
            "one playlist without its tracks: its .m3u alone, whatever is missing"
        );
    }

    #[test]
    fn combining_lists_each_track_once_at_its_first_place() {
        let (_d, p) = sample();
        let o = Options { combine: Some("Both".into()), ..Default::default() };
        let l = layout(&p, &o);
        assert_eq!(l.lists, vec![("Both.m3u".into(), "A - Alb/01 One.flac\r\nA - Alb/02 Two.flac\r\nB - Alb/01 One.mp3\r\n".into())]);
        assert_eq!(download(&p, &o, unopened).name(), "Both.m3u");
        assert_eq!(preview(&p, &o), Preview { tracks: 3, missing: 2, bytes: None, estimate: false });
    }

    #[test]
    fn in_one_folder_tracks_are_named_artist_title_and_a_clash_gets_a_number() {
        let (_d, p) = sample();
        let o = Options { tracks: true, arrange: Arrange::Flat, ..Default::default() };
        let l = layout(&p, &o);
        assert_eq!(l.files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["A - One.flac", "02 Two.flac", "A - One.mp3"]);
        let d = music();
        d.put("x/1.mp3", b"1");
        d.put("y/1.mp3", b"2");
        let same = plan(&d.0, vec![("p".into(), lines(&["x/1.mp3", "y/1.mp3"]))], |r| Ok(r.iter().map(|_| named("A", "T")).collect())).unwrap();
        let names: Vec<_> = layout(&same, &o).files.into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["A - T.mp3", "A - T (2).mp3"]);
        assert_eq!(layout(&same, &o).lists[0].1, "A - T.mp3\r\nA - T (2).mp3\r\n", "the lines name the bundled files");
    }

    #[test]
    fn in_artist_and_album_folders_the_album_artist_comes_first_and_the_files_keep_their_names() {
        let t = |rel: &str, f: Facts| Track { path: PathBuf::from(rel), rel: rel.into(), size: 1, facts: f };
        let tracks = vec![
            t("A - Alb (1999)/01 A - One.flac", Facts { album_artist: Some("Various".into()), artist: Some("A".into()), album: Some("Alb: Live".into()), ..Facts::default() }),
            t("B/02 B - Two.mp3", Facts { artist: Some("B".into()), ..Facts::default() }),
            t("03 Three.mp3", Facts::default()),
            t("Other/01 A - One.flac", Facts { album_artist: Some("various".into()), album: Some("alb: live".into()), ..Facts::default() }),
        ];
        let p = Plan { lists: vec![("L".into(), vec![0, 1, 2, 3])], tracks, missing: vec![] };
        let o = Options { tracks: true, arrange: Arrange::Folders, ..Default::default() };
        let l = layout(&p, &o);
        assert_eq!(
            l.files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            ["Various/Alb_ Live/01 A - One.flac", "B/B/02 B - Two.mp3", "Unknown artist/03 Three.mp3", "various/alb_ live/01 A - One (2).flac"],
            "no album tag: the folder it is in; no folder either: under the artist; a clash (ignoring case) gets a number"
        );
        assert_eq!(l.lists[0].1.lines().next(), Some("Various/Alb_ Live/01 A - One.flac"));
    }

    #[test]
    fn each_format_says_per_file_what_happens() {
        let hires = Kind::Lossless { hires: true };
        let cd = Kind::Lossless { hires: false };
        for k in [hires, cd, Kind::Other] {
            assert_eq!(treatment(Format::Original, k), Treatment::AsIs, "{k:?}");
        }
        assert_eq!(treatment(Format::FlacCd, hires), Treatment::CdFlac);
        assert_eq!(treatment(Format::FlacCd, cd), Treatment::AsIs, "CD format already");
        assert_eq!(treatment(Format::FlacCd, Kind::Other), Treatment::AsIs, "lossy: as it is");
        assert_eq!(treatment(Format::Opus(128), hires), Treatment::Opus(128));
        assert_eq!(treatment(Format::Opus(128), cd), Treatment::Opus(128));
        assert_eq!(treatment(Format::Opus(128), Kind::Other), Treatment::AsIs, "never a second lossy pass");
        for k in [hires, cd] {
            assert_eq!(treatment(Format::Mp3Cbr320, k), Treatment::Mp3(Mp3Kind::Cbr320), "{k:?}");
            assert_eq!(treatment(Format::Mp3V0, k), Treatment::Mp3(Mp3Kind::V0), "{k:?}");
        }
        assert_eq!(treatment(Format::Mp3Cbr320, Kind::Other), Treatment::AsIs, "lossy: as it is, never a second pass");
        assert_eq!(treatment(Format::Mp3V0, Kind::Other), Treatment::AsIs);
        let f = |format: &str, kbps: u32| Facts { format: Some(format.into()), kbps: Some(kbps), ..Facts::default() };
        assert_eq!(kind_guessed(&f("FLAC", 2800)), hires);
        assert_eq!(kind_guessed(&f("FLAC", 900)), cd);
        assert_eq!(kind_guessed(&f("WAV", 1411)), cd);
        assert_eq!(kind_guessed(&f("MP4", 700)), cd, "ALAC");
        assert_eq!(kind_guessed(&f("MP4", 256)), Kind::Other, "AAC");
        assert_eq!(kind_guessed(&f("MP3", 320)), Kind::Other);
        assert_eq!(kind_guessed(&Facts::default()), Kind::Other);
    }

    #[test]
    fn a_converted_formats_size_is_an_estimate_from_the_catalogues_facts() {
        let t = |format: &str, kbps: u32, size: u64| Track {
            path: PathBuf::from("x"),
            rel: "x".into(),
            size,
            facts: Facts { format: Some(format.into()), kbps: Some(kbps), duration_ms: Some(60_000), ..Facts::default() },
        };
        let (hi, cd, mp3) = (t("FLAC", 2800, 21_000_000), t("FLAC", 900, 6_750_000), t("MP3", 320, 2_400_000));
        assert_eq!(size_out(&hi, Format::Original, 900.0), (21_000_000, false));
        assert_eq!(size_out(&hi, Format::Opus(160), 900.0), (1_200_000, true), "160 kb/s for a minute");
        assert_eq!(size_out(&mp3, Format::Opus(160), 900.0), (2_400_000, false), "lossy: as it is");
        assert_eq!(size_out(&hi, Format::FlacCd, 900.0), (6_750_000, true), "at the CD-format files' rate");
        assert_eq!(size_out(&cd, Format::FlacCd, 900.0), (6_750_000, false));
        assert_eq!(size_out(&t("FLAC", 2800, 1000), Format::FlacCd, 900.0), (1000, true), "never larger than it was");
        assert_eq!(size_out(&cd, Format::Mp3Cbr320, 900.0), (2_400_000, true), "320 kb/s for a minute");
        assert_eq!(size_out(&hi, Format::Mp3V0, 900.0), (1_837_500, true), "about 245 kb/s for a minute");
        assert_eq!(size_out(&mp3, Format::Mp3V0, 900.0), (2_400_000, false), "lossy: as it is");
        assert_eq!(cd_kbps(&[&hi, &cd, &mp3]), 900.0, "the CD-format lossless files' own rate");
        assert_eq!(cd_kbps(&[&hi, &mp3]), CD_KBPS, "none: CD audio's");
        let p = Plan { lists: vec![("L".into(), vec![0, 1, 2])], tracks: vec![hi, cd, mp3], missing: vec![] };
        let o = |format| Options { tracks: true, format, ..Options::default() };
        assert_eq!(preview(&p, &o(Format::FlacCd)), Preview { tracks: 3, missing: 0, bytes: Some(15_900_000), estimate: true });
        assert_eq!(preview(&p, &o(Format::Original)).estimate, false);
    }

    #[test]
    fn the_zip_holds_the_tracks_the_playlists_and_the_report_and_its_length_is_known_before() {
        let (_d, p) = sample();
        let o = Options { tracks: true, ..Default::default() };
        let l = layout(&p, &o);
        assert!(l.report.as_deref().unwrap().contains("Chill: Gone/x.mp3"));
        assert_eq!(preview(&p, &o), Preview { tracks: 3, missing: 2, bytes: Some(19), estimate: false });
        let work = music();
        let (len, bytes) = zipped(p, o, work.0.clone());
        assert_eq!(len, Some(bytes.len() as u64), "the announced length is the length sent");
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(names(&mut zip), ["A - Alb/01 One.flac", "A - Alb/02 Two.flac", "B - Alb/01 One.mp3", "Chill.m3u", "Jazz_Soul.m3u", "Missing tracks.txt"]);
        assert_eq!(text(&mut zip, "B - Alb/01 One.mp3"), "one-mp3!", "the data is read back with its CRC checked");
    }

    #[test]
    fn as_opus_a_lossless_file_becomes_a_copy_a_lossy_one_stays_and_the_lines_name_what_went_in() {
        let d = music();
        std::fs::create_dir_all(d.0.join("Alb")).unwrap();
        crate::opus_copy::tests::wav(&d.0.join("Alb/01 One.wav"), 44_100, 0.5);
        d.put("Alb/02 Two.mp3", b"not really an mp3");
        let found = ["Alb/01 One.wav", "Alb/02 Two.mp3"].iter().map(|r| Ok((d.0.join(r), r.to_string(), Facts { album: Some("Alb".into()), artist: Some("A".into()), ..Facts::default() }))).collect();
        let p = plan_files("Mix", found);
        let o = Options { tracks: true, arrange: Arrange::Folders, format: Format::Opus(96), ..Options::default() };
        let work = music();
        let (len, bytes) = zipped(p, o, work.0.clone());
        assert_eq!(len, None, "a copy's size is known only once it is made");
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(names(&mut zip), ["A/Alb/01 One.opus", "A/Alb/02 Two.mp3", "Mix.m3u"]);
        assert_eq!(text(&mut zip, "Mix.m3u"), "A/Alb/01 One.opus\r\nA/Alb/02 Two.mp3\r\n");
        let mut ogg = Vec::new();
        zip.by_name("A/Alb/01 One.opus").unwrap().read_to_end(&mut ogg).unwrap();
        assert_eq!(&ogg[..4], b"OggS");
        assert!(ogg.len() < 20_000, "about 96 kb/s for half a second: {} bytes", ogg.len());
        assert_eq!(std::fs::read_dir(&work.0).unwrap().count(), 0, "the copy is gone from the work folder once sent");
    }

    #[test]
    fn at_cd_quality_a_hires_file_becomes_cd_flac_and_a_cd_one_stays() {
        let d = music();
        std::fs::create_dir_all(d.0.join("Alb")).unwrap();
        crate::convert::tests::write_wav24(&d.0.join("Alb/01 Hi.wav"), 96_000, &crate::convert::tests::sine(96_000, 440.0, 0.5, 0.3, 2));
        crate::opus_copy::tests::wav(&d.0.join("Alb/02 Cd.wav"), 44_100, 0.3);
        let cd_size = std::fs::metadata(d.0.join("Alb/02 Cd.wav")).unwrap().len();
        let found = ["Alb/01 Hi.wav", "Alb/02 Cd.wav"].iter().map(|r| Ok((d.0.join(r), r.to_string(), Facts::default()))).collect();
        let p = plan_files("Hi", found);
        let o = Options { tracks: true, format: Format::FlacCd, lists: false, ..Options::default() };
        let work = music();
        let (_, bytes) = zipped(p, o, work.0.clone());
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(names(&mut zip), ["Alb/01 Hi.flac", "Alb/02 Cd.wav"], "no playlist file asked for");
        let mut flac = Vec::new();
        zip.by_name("Alb/01 Hi.flac").unwrap().read_to_end(&mut flac).unwrap();
        assert_eq!(&flac[..4], b"fLaC");
        // STREAMINFO: 20 bits of sample rate, then 3 of channels - 1, 5 of bits - 1.
        let info = &flac[8..];
        let rate = (u32::from(info[10]) << 12) | (u32::from(info[11]) << 4) | (u32::from(info[12]) >> 4);
        let bits = (((info[12] & 1) << 4) | (info[13] >> 4)) + 1;
        assert_eq!((rate, bits), (44_100, 16));
        assert_eq!(zip.by_name("Alb/02 Cd.wav").unwrap().size(), cd_size, "as it is");
    }

    #[test]
    fn one_track_alone_is_that_file_under_its_own_name() {
        let d = music();
        let rel = "Miles Davis/Kind of Blue/01 So What.mp3";
        d.put(rel, b"so what");
        let facts = Facts { artist: Some("Miles Davis".into()), album: Some("Kind of Blue".into()), ..Facts::default() };
        let p = plan_files("Kind of Blue", vec![Ok((d.0.join(rel), rel.to_string(), facts))]);
        for arrange in [Arrange::Library, Arrange::Folders, Arrange::Flat] {
            let o = Options { tracks: true, arrange, lists: false, ..Options::default() };
            let one = download(&p, &o, unopened);
            assert_eq!(one, Download::Track { at: 0, how: Treatment::AsIs, name: "01 So What.mp3".into() }, "{arrange:?}: the last part of its name in the bundle");
        }
        let o = Options { tracks: true, lists: false, format: Format::Opus(160), ..Options::default() };
        assert_eq!(download(&p, &o, |_| Kind::Other).name(), "01 So What.mp3", "lossy, asked as Opus: as it is");
        let work = music();
        assert_eq!(sent(&p.tracks[0], Treatment::AsIs, &work.0).unwrap(), (7, b"so what".to_vec()), "its bytes, their length told");
        assert_eq!(content_type("01 So What.mp3"), "audio/mpeg");
    }

    #[test]
    fn one_track_converted_goes_with_its_new_extension_or_not_at_all() {
        let d = music();
        std::fs::create_dir_all(d.0.join("Alb")).unwrap();
        crate::opus_copy::tests::wav(&d.0.join("Alb/01 One.wav"), 44_100, 0.5);
        let one_of = |rel: &str| plan_files("Alb", vec![Ok((d.0.join(rel), rel.to_string(), Facts::default()))]);
        let p = one_of("Alb/01 One.wav");
        let o = Options { tracks: true, format: Format::Mp3V0, lists: false, ..Options::default() };
        let one = download(&p, &o, |t| kind_of(&t.path));
        assert_eq!(one, Download::Track { at: 0, how: Treatment::Mp3(Mp3Kind::V0), name: "01 One.mp3".into() }, "its header read: lossless");
        assert_eq!(content_type(one.name()), "audio/mpeg");
        let work = music();
        let (len, mp3) = sent(&p.tracks[0], Treatment::Mp3(Mp3Kind::V0), &work.0).unwrap();
        assert_eq!(len, mp3.len() as u64, "its exact length told");
        assert_eq!((mp3[0], mp3[1] & 0xe0), (0xff, 0xe0), "an MP3 frame (the WAV has no tags)");
        assert!(mp3.windows(4).take(200).any(|w| w == b"Xing"), "LAME's header frame first");
        assert_eq!(std::fs::read_dir(&work.0).unwrap().count(), 0, "the converted file is gone once sent");
        let o = Options { format: Format::Opus(96), ..o };
        assert_eq!(download(&p, &o, |t| kind_of(&t.path)).name(), "01 One.opus");

        // One that cannot be converted is refused rather than sent as it is under a name that says otherwise.
        d.put("Alb/02 Two.flac", b"not really a flac");
        let p = one_of("Alb/02 Two.flac");
        let o = Options { format: Format::Mp3Cbr320, ..o };
        let one = download(&p, &o, |_| Kind::Lossless { hires: false });
        assert_eq!(one.name(), "02 Two.mp3");
        assert!(sent(&p.tracks[0], Treatment::Mp3(Mp3Kind::Cbr320), &work.0).is_err());
    }

    #[test]
    fn more_than_one_file_is_a_zip() {
        let d = music();
        d.put("Alb/1.mp3", b"1");
        d.put("Alb/2.mp3", b"22");
        let at = |rel: &str| Ok((d.0.join(rel), rel.to_string(), Facts::default()));
        let of = |o: &Options, found: Vec<std::result::Result<(PathBuf, String, Facts), String>>| download(&plan_files("Alb", found), o, unopened);
        let bare = Options { tracks: true, lists: false, ..Options::default() };
        assert_eq!(of(&bare, vec![at("Alb/1.mp3")]).name(), "1.mp3", "one track");
        assert_eq!(of(&bare, vec![at("Alb/1.mp3"), at("Alb/1.mp3")]).name(), "1.mp3", "the same file twice is one file");
        assert_eq!(of(&Options { lists: true, ..bare.clone() }, vec![at("Alb/1.mp3")]).name(), "Alb.zip", "one track and its playlist file");
        assert_eq!(of(&bare, vec![at("Alb/1.mp3"), at("Alb/2.mp3")]).name(), "Alb.zip", "two tracks");
        assert_eq!(of(&bare, vec![at("Alb/1.mp3"), Err("cratarr tr-9".into())]).name(), "Alb.zip", "one track and the report of one missing");
        assert_eq!(of(&bare, vec![at("Alb/gone.mp3")]).name(), REPORT, "nothing found: the report is the one file");

        // The zip of one track and the report holds both.
        let p = plan_files("Alb", vec![at("Alb/1.mp3"), Err("cratarr tr-9".into())]);
        let work = music();
        let (_, bytes) = zipped(p, bare, work.0.clone());
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(names(&mut z), ["Alb/1.mp3", REPORT]);
        assert_eq!(content_type("Alb.zip"), "application/zip");
        assert_eq!(content_type("Alb.m3u"), "audio/x-mpegurl");
        assert_eq!(content_type(REPORT), "text/plain; charset=utf-8");
    }

    #[test]
    fn a_token_lasts_its_life_and_the_oldest_go_when_there_are_too_many() {
        let mut t = Tokens::<u32>::new();
        t.put("a".into(), 1000, 1);
        assert_eq!(t.get("a", 1000), Some(1));
        assert_eq!(t.get("a", 1000 + Tokens::<u32>::LIFE - 1), Some(1), "asked for again while it lasts");
        assert_eq!(t.get("a", 1000 + Tokens::<u32>::LIFE), None, "expired");
        assert_eq!(t.get("b", 1000), None);
        for i in 0..Tokens::<u32>::MOST as u64 + 1 {
            t.put(format!("t{i}"), 2000 + i, i as u32);
        }
        assert_eq!(t.held.len(), Tokens::<u32>::MOST);
        assert_eq!(t.get("t0", 2100), None, "the oldest went");
        assert_eq!(t.get("t1", 2100), Some(1));
        assert!(!t.held.contains_key("a"), "an expired one is dropped");
        let token = new_token();
        assert_eq!(token.len(), 32);
        assert_ne!(token, new_token());
    }

    #[test]
    fn sizes_and_offsets_past_4_gb_get_zip64_fields() {
        let mut z = Zip::new(0, 0);
        z.offset = 5_000_000_000;
        let (h, e) = z.begin("big.flac", 6_000_000_000);
        assert_eq!(u16::from_le_bytes([h[28], h[29]]), 20, "a zip64 extra in the local header");
        let d = z.finish(e, 7, 6_000_000_000);
        assert_eq!(d.len(), 24, "64-bit sizes in the descriptor");
        let end = z.end();
        assert_eq!(&end[end.len() - 22..end.len() - 18], &0x0605_4b50u32.to_le_bytes());
        assert!(end.windows(4).any(|w| w == 0x0606_4b50u32.to_le_bytes()), "a zip64 end record");
        assert_eq!(u32::from_le_bytes(end[end.len() - 6..end.len() - 2].try_into().unwrap()), u32::MAX, "the offset says: see zip64");
    }

    #[test]
    fn dos_dates_and_safe_names() {
        // 2026-10-14 12:34:56 UTC.
        let (t, d) = dos_time(1_791_981_296);
        assert_eq!((d >> 9) + 1980, 2026);
        assert_eq!((d >> 5) & 0xF, 10);
        assert_eq!(d & 0x1F, 14);
        assert_eq!((t >> 11, (t >> 5) & 0x3F, (t & 0x1F) * 2), (12, 34, 56));
        assert_eq!(safe_name("AC/DC: Live?"), "AC_DC_ Live_");
        assert_eq!(safe_name(" ... "), "Untitled");
        assert_eq!(crc32(0, b"123456789"), 0xCBF4_3926);
    }
}
