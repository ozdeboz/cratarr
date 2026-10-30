//! The naming template of a library (decision 0013): how Keep names an album's folder and its files from their tags.
//! Tags are never changed; only names are made from them.
//!
//! A template is one string: the folder name, `/`, the file name. The folder part may itself hold `/` for nested
//! folders (`{albumartist}/{album} ({originalyear})`). The reference install's:
//! `{albumartist} - {album} ({originalyear})/{disc-}{track:02} {artist} - {title}`.
//!
//! Tokens:
//! - `{albumartist}`, `{album}`, `{artist}`, `{title}`: the tags of those names;
//! - `{originalyear}`: the first four digits of `originaldate`, else of `date`; `{year}`: `date` first;
//! - `{track}`, `{disc}`, padded with zeros by `{track:02}`;
//! - `{disc-}`: the disc number and `-` on an album whose tracks carry two or more disc numbers, nothing on others.
//!
//! A tag that occurs twice in a file counts by its first value. A folder takes one value per album: the value most
//! of its tracks have. No albumartist: the artist when every track has the same one, else `Various Artists`.
//!
//! Names are safe on any file system and over SMB from Windows. The rules follow the reference library where it is
//! consistent (2,230 folders, 24,269 files, majority where it is not):
//! - `/` in a value is part of it (`yes/and`, `AC/DC`, a credit like `Low Flung / Other Joe`) and becomes `_`, as
//!   MusicBrainz Picard does (its default "directory separators character"). Several artists are told apart by the
//!   list tags (`albumartists`, `artists`), not by slashes in the text tags (Cratarr decision 0016);
//! - every other character Windows cannot take (`:` `?` `"` `*` `<` `>` `|` `\`) becomes `_` too (`Title: Subtitle`
//!   → `Title_ Subtitle`), as Picard does;
//! - control characters are dropped (tabs and line breaks become a space);
//! - everything else stays: accents, other scripts, other punctuation, double spaces, dots inside a name;
//! - no name starts with a space, and a dot at its start becomes `_` (it would hide the file). A folder name does not
//!   end with a space, and a dot at its end becomes `_` (`Howie B.` → `Howie B_`): Windows cannot take either. Both as
//!   Picard does. A file name ends with its extension, so `R.E.M..mp3` is fine;
//! - at most [`MAX_NAME_BYTES`] per name, cut at a character; Windows device names (`CON`, `NUL`, `COM1` …) get `_`.
//!
//! [`infer`] finds the template a library already follows: it renders every album with candidate templates and
//! keeps the one whose names match the real ones most often.
//!
//! Pure: no IO.
#![allow(dead_code)] // used by Keep in Incoming and the Settings page (decision 0013)

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Serialize, Serializer};

/// The template a library with no albums starts with.
pub const DEFAULT: &str = "{albumartist} - {album} ({originalyear})/{disc-}{track:02} {artist} - {title}";

/// Every tag a template reads: what a library's names are judged from.
pub const TAGS: [&str; 10] =
    ["albumartist", "album", "artist", "title", "tracknumber", "discnumber", "originaldate", "originalyear", "date", "year"];

/// The longest name, in bytes: well under the 255 of every common file system, with room for the folders above it in
/// a Windows path. The reference library's longest is 147.
pub const MAX_NAME_BYTES: usize = 180;

/// The albumartist of an album without one whose tracks have different artists (MusicBrainz's name for it).
const VARIOUS: &str = "Various Artists";

/// How far, as a share of the names, an earlier candidate template may match fewer than the best and still be taken.
const MARGIN: f64 = 0.005;

/// Mismatches kept per kind and cause, to show.
const SAMPLES: usize = 3;

/// Characters no name may hold (with control characters and `/`).
const ILLEGAL: [char; 8] = ['\\', ':', '*', '?', '"', '<', '>', '|'];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Field {
    AlbumArtist,
    Album,
    OriginalYear,
    Year,
    Artist,
    Title,
    Track,
    Disc,
    /// `{disc-}`.
    DiscPrefix,
}

impl Field {
    const ALL: [Field; 9] = [
        Field::AlbumArtist,
        Field::Album,
        Field::OriginalYear,
        Field::Year,
        Field::Artist,
        Field::Title,
        Field::Track,
        Field::Disc,
        Field::DiscPrefix,
    ];

    fn name(self) -> &'static str {
        match self {
            Field::AlbumArtist => "albumartist",
            Field::Album => "album",
            Field::OriginalYear => "originalyear",
            Field::Year => "year",
            Field::Artist => "artist",
            Field::Title => "title",
            Field::Track => "track",
            Field::Disc => "disc",
            Field::DiscPrefix => "disc-",
        }
    }

    /// One value per album, so it may name a folder.
    fn of_album(self) -> bool {
        matches!(self, Field::AlbumArtist | Field::Album | Field::OriginalYear | Field::Year | Field::Artist)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(String),
    /// A field, padded with zeros to a width (0: not padded).
    Token(Field, usize),
}

/// One folder or file name: text and tokens.
type Name = Vec<Part>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    #[error("the template is empty")]
    Empty,
    #[error("no folder: the template is the folder name, a /, and the file name")]
    NoFolder,
    #[error("an empty folder or file name")]
    EmptyName,
    #[error("unknown token {{{0}}}")]
    UnknownToken(String),
    #[error("a {{ without its }}")]
    Unclosed,
    #[error("a }} without its {{")]
    Unopened,
    #[error("{0:?} cannot be in a name")]
    IllegalChar(char),
    #[error("{{{0}}} cannot be padded; only {{track}} and {{disc}} can, like {{track:02}}")]
    Width(String),
    #[error("{{{0}}} differs per track, so it cannot name the folder")]
    NotOfAlbum(String),
    #[error("the folder name needs {{album}}")]
    FolderNeedsAlbum,
    #[error("the file name needs {{title}} or {{track}}")]
    FileNeedsTitleOrTrack,
}

/// A library's naming template. Its string form (`Display`, `FromStr`) is the Settings value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    folders: Vec<Name>,
    file: Name,
}

impl Default for Template {
    fn default() -> Self {
        DEFAULT.parse().expect("the default template is valid")
    }
}

impl FromStr for Template {
    type Err = TemplateError;

    fn from_str(s: &str) -> Result<Self, TemplateError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(TemplateError::Empty);
        }
        let (folder, file) = s.rsplit_once('/').ok_or(TemplateError::NoFolder)?;
        let template = Template { folders: parse_folders(folder)?, file: parse_name(file)? };
        if !template.file.iter().any(|p| matches!(p, Part::Token(Field::Title | Field::Track, _))) {
            return Err(TemplateError::FileNeedsTitleOrTrack);
        }
        Ok(template)
    }
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for name in &self.folders {
            write_name(f, name)?;
            f.write_str("/")?;
        }
        write_name(f, &self.file)
    }
}

impl Serialize for Template {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

fn write_name(f: &mut fmt::Formatter<'_>, name: &Name) -> fmt::Result {
    for part in name {
        match part {
            Part::Text(t) => f.write_str(t)?,
            Part::Token(field, 0) => write!(f, "{{{}}}", field.name())?,
            Part::Token(field, width) => write!(f, "{{{}:0{width}}}", field.name())?,
        }
    }
    Ok(())
}

fn parse_folders(s: &str) -> Result<Vec<Name>, TemplateError> {
    let folders = s.split('/').map(parse_name).collect::<Result<Vec<_>, _>>()?;
    let tokens = || folders.iter().flatten().filter_map(|p| match p {
        Part::Token(field, _) => Some(*field),
        Part::Text(_) => None,
    });
    if let Some(field) = tokens().find(|f| !f.of_album()) {
        return Err(TemplateError::NotOfAlbum(field.name().into()));
    }
    if !tokens().any(|f| f == Field::Album) {
        return Err(TemplateError::FolderNeedsAlbum);
    }
    Ok(folders)
}

fn parse_name(s: &str) -> Result<Name, TemplateError> {
    let mut parts = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(['{', '}']) {
        if rest[i..].starts_with('}') {
            return Err(TemplateError::Unopened);
        }
        push_text(&mut parts, &rest[..i])?;
        let token = &rest[i + 1..];
        let end = token.find('}').ok_or(TemplateError::Unclosed)?;
        if token[..end].contains('{') {
            return Err(TemplateError::Unclosed);
        }
        parts.push(parse_token(&token[..end])?);
        rest = &token[end + 1..];
    }
    push_text(&mut parts, rest)?;
    let only_dots = parts.iter().all(|p| matches!(p, Part::Text(t) if t.trim_matches([' ', '.']).is_empty()));
    if only_dots {
        return Err(TemplateError::EmptyName);
    }
    Ok(parts)
}

fn push_text(parts: &mut Vec<Part>, text: &str) -> Result<(), TemplateError> {
    if let Some(c) = text.chars().find(|c| ILLEGAL.contains(c) || c.is_control()) {
        return Err(TemplateError::IllegalChar(c));
    }
    if !text.is_empty() {
        parts.push(Part::Text(text.to_string()));
    }
    Ok(())
}

fn parse_token(inner: &str) -> Result<Part, TemplateError> {
    let (name, spec) = inner.split_once(':').map_or((inner, None), |(n, s)| (n, Some(s)));
    let name = name.trim();
    let field = Field::ALL
        .into_iter()
        .find(|f| f.name().eq_ignore_ascii_case(name))
        .ok_or_else(|| TemplateError::UnknownToken(inner.to_string()))?;
    let Some(spec) = spec else { return Ok(Part::Token(field, 0)) };
    let width = spec.trim().parse::<usize>().ok().filter(|w| (1..=9).contains(w));
    match (field, width) {
        (Field::Track | Field::Disc, Some(width)) => Ok(Part::Token(field, width)),
        _ => Err(TemplateError::Width(inner.to_string())),
    }
}

/// A file's tags: (name, value) pairs as the catalogue holds them, Picard names (`albumartist`, `tracknumber`, …).
/// A name may repeat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tags(pub Vec<(String, String)>);

impl Tags {
    /// The non-empty values of a tag, trimmed, without repeats.
    fn values(&self, name: &str) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for (n, v) in &self.0 {
            let v = v.trim();
            if n.eq_ignore_ascii_case(name) && !v.is_empty() && !out.contains(&v) {
                out.push(v);
            }
        }
        out
    }

    pub(crate) fn first(&self, name: &str) -> Option<&str> {
        self.values(name).into_iter().next()
    }

    /// The (original) release year: `originaldate`, else `date`.
    pub(crate) fn release_year(&self) -> Option<String> {
        year(self, &["originaldate", "date"])
    }
}

impl<const N: usize> From<[(&str, &str); N]> for Tags {
    fn from(pairs: [(&str, &str); N]) -> Self {
        Tags(pairs.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect())
    }
}

/// A file of an album: its present name (the extension is kept) and its tags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Track {
    pub file: String,
    pub tags: Tags,
}

/// An album as it lies in a library: its folder, relative to the library (`/` between folders), and its files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Album {
    pub folder: String,
    pub tracks: Vec<Track>,
}

/// An album's names under a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Names {
    /// Relative to the library, `/` between folders.
    pub folder: String,
    /// In the order of the tracks given.
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    /// A tag the template needs is missing: the year can be asked for and added to the tags.
    #[error("no {token} for {}", .track.map_or("the album".to_string(), |t| format!("track {}", t + 1)))]
    Missing { token: &'static str, track: Option<usize> },
    #[error("two tracks would both be named {0}")]
    Duplicate(String),
}

impl Template {
    /// An album's folder and file names.
    pub fn render(&self, tracks: &[Track]) -> Result<Names, RenderError> {
        let album = AlbumFacts::of(tracks);
        let folder = self.render_folder(&album, false)?;
        let mut files: Vec<String> = Vec::with_capacity(tracks.len());
        for (i, track) in tracks.iter().enumerate() {
            let name = render_file(&self.file, &album, track, album.multi_disc)
                .map_err(|token| RenderError::Missing { token, track: Some(i) })?;
            if files.iter().any(|f| f.to_lowercase() == name.to_lowercase()) {
                return Err(RenderError::Duplicate(name));
            }
            files.push(name);
        }
        Ok(Names { folder, files })
    }

    /// `swap_years`: `{originalyear}` and `{year}` trade places (to tell a year from the other tag apart).
    fn render_folder(&self, album: &AlbumFacts, swap_years: bool) -> Result<String, RenderError> {
        render_folders(&self.folders, album, swap_years).map_err(|token| RenderError::Missing { token, track: None })
    }
}

fn render_folders(folders: &[Name], album: &AlbumFacts, swap_years: bool) -> Result<String, &'static str> {
    let mut out = Vec::with_capacity(folders.len());
    for name in folders {
        let mut s = String::new();
        for part in name {
            match part {
                Part::Text(t) => s.push_str(t),
                Part::Token(field, _) => {
                    let field = match (*field, swap_years) {
                        (Field::OriginalYear, true) => Field::Year,
                        (Field::Year, true) => Field::OriginalYear,
                        (f, _) => f,
                    };
                    s.push_str(album.get(field).ok_or(field.name())?);
                }
            }
        }
        out.push(finish(&s, ""));
    }
    Ok(out.join("/"))
}

fn render_file(name: &Name, album: &AlbumFacts, track: &Track, multi_disc: bool) -> Result<String, &'static str> {
    let mut s = String::new();
    for part in name {
        match part {
            Part::Text(t) => s.push_str(t),
            Part::Token(field, width) => {
                let value = match field {
                    Field::Artist => text(&track.tags, "artist"),
                    Field::Title => text(&track.tags, "title"),
                    Field::Track => track_number(&track.tags, *width),
                    Field::Disc => disc(&track.tags).or((!multi_disc).then_some(1)).map(|d| pad(d, *width)),
                    Field::DiscPrefix if !multi_disc => Some(String::new()),
                    Field::DiscPrefix => disc(&track.tags).map(|d| format!("{d}-")),
                    _ => album.get(*field).map(str::to_string),
                };
                s.push_str(&value.ok_or(field.name())?);
            }
        }
    }
    Ok(finish(&s, extension(&track.file)))
}

/// The values an album's folder is named from, each the one most of its tracks have.
struct AlbumFacts {
    values: Vec<(Field, String)>,
    multi_disc: bool,
}

impl AlbumFacts {
    fn of(tracks: &[Track]) -> Self {
        let mut values = Vec::new();
        let mut push = |field: Field, value: Option<String>| {
            if let Some(v) = value {
                values.push((field, v));
            }
        };
        let artist = most(tracks.iter().map(|t| text(&t.tags, "artist")));
        let albumartist = most(tracks.iter().map(|t| text(&t.tags, "albumartist"))).or_else(|| {
            let mut artists = tracks.iter().map(|t| text(&t.tags, "artist"));
            let first = artists.next().flatten()?;
            Some(if artists.all(|a| a.as_ref() == Some(&first)) { first } else { VARIOUS.to_string() })
        });
        push(Field::AlbumArtist, albumartist);
        push(Field::Artist, artist);
        push(Field::Album, most(tracks.iter().map(|t| text(&t.tags, "album"))));
        push(Field::OriginalYear, most(tracks.iter().map(|t| year(&t.tags, &["originaldate", "originalyear", "date"]))));
        push(Field::Year, most(tracks.iter().map(|t| year(&t.tags, &["date", "year", "originaldate", "originalyear"]))));
        let mut discs: Vec<u32> = tracks.iter().filter_map(|t| disc(&t.tags)).collect();
        discs.sort_unstable();
        discs.dedup();
        AlbumFacts { values, multi_disc: discs.len() > 1 }
    }

    fn get(&self, field: Field) -> Option<&str> {
        self.values.iter().find(|(f, _)| *f == field).map(|(_, v)| v.as_str())
    }
}

/// The most frequent value; on a tie, the first seen.
fn most(values: impl Iterator<Item = Option<String>>) -> Option<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for v in values.flatten() {
        match counts.iter_mut().find(|(c, _)| *c == v) {
            Some((_, n)) => *n += 1,
            None => counts.push((v, 1)),
        }
    }
    let best = counts.iter().map(|(_, n)| *n).max()?;
    counts.into_iter().find(|(_, n)| *n == best).map(|(v, _)| v)
}

/// A text tag made safe for a name. Only its first value counts: a second is most often the same tag from another
/// frame (`TPE2` and `TXXX:ALBUMARTIST` saying `Various Artists` and `Various`), not a list.
fn text(tags: &Tags, name: &str) -> Option<String> {
    tags.first(name).map(clean).filter(|v| !v.trim().is_empty())
}

/// Makes one tag value safe for a name: the character rules above. Edges and length are a whole name's.
pub fn clean(value: &str) -> String {
    value.chars().filter_map(replace).collect()
}

fn replace(c: char) -> Option<char> {
    match c {
        ':' | '*' | '<' | '>' | '|' | '\\' | '?' | '"' | '/' => Some('_'),
        '\t' | '\n' | '\r' => Some(' '),
        c if c.is_control() => None,
        c => Some(c),
    }
}

/// The first four characters of a date when they are a year; `0000` is none.
fn year(tags: &Tags, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        tags.values(name).into_iter().find_map(|v| {
            let y: String = v.chars().take(4).collect();
            (y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit()) && y != "0000").then_some(y)
        })
    })
}

/// `3`, `03/12` → the number; a vinyl side's `A1` stays as it is.
fn track_number(tags: &Tags, width: usize) -> Option<String> {
    let raw = tags.first("tracknumber")?.split('/').next()?.trim();
    match raw.parse::<u32>() {
        Ok(n) => Some(pad(n, width)),
        Err(_) => Some(clean(raw)).filter(|s| !s.is_empty()),
    }
}

fn disc(tags: &Tags) -> Option<u32> {
    tags.first("discnumber")?.split('/').next()?.trim().parse().ok()
}

fn pad(n: u32, width: usize) -> String {
    format!("{n:0width$}")
}

/// A file name's extension, without the dot; empty when it has none.
fn extension(file: &str) -> &str {
    match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()) => ext,
        _ => "",
    }
}

/// A name's edges and length; `ext` is empty for a folder.
fn finish(name: &str, ext: &str) -> String {
    let budget = if ext.is_empty() { MAX_NAME_BYTES } else { MAX_NAME_BYTES - ext.len() - 1 };
    let mut stem = name.trim_start_matches(' ').to_string();
    // A leading dot would hide the file: it becomes `_`, as Picard does.
    if stem.starts_with('.') {
        stem.replace_range(..1, "_");
    }
    if stem.len() > budget {
        let mut cut = budget;
        while !stem.is_char_boundary(cut) {
            cut -= 1;
        }
        stem.truncate(cut);
    }
    let mut stem = stem.trim_end_matches(' ').to_string();
    // A folder's last dot becomes `_`, as Picard does (`./` → `_/`): Windows drops a trailing dot.
    if ext.is_empty() && stem.ends_with('.') {
        stem.pop();
        stem.push('_');
    }
    if stem.is_empty() {
        stem.push('_');
    }
    let base = stem.find('.').unwrap_or(stem.len());
    if is_device(&stem[..base]) {
        stem.insert(base, '_');
    }
    if ext.is_empty() { stem } else { format!("{stem}.{ext}") }
}

/// Windows reserves these names, with any extension.
fn is_device(base: &str) -> bool {
    let b = base.trim_end().to_ascii_uppercase();
    matches!(b.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((b.starts_with("COM") || b.starts_with("LPT")) && b.len() == 4 && matches!(b.as_bytes()[3], b'1'..=b'9'))
}

// Inference.

/// Folder templates tried, the most likely first (it wins a tie).
const FOLDER_CANDIDATES: [&str; 12] = [
    "{albumartist} - {album} ({originalyear})",
    "{albumartist} - {album} ({year})",
    "{albumartist} - {album} [{originalyear}]",
    "{albumartist} - {originalyear} - {album}",
    "{albumartist} - {album}",
    "{albumartist}/{album} ({originalyear})",
    "{albumartist}/{album} ({year})",
    "{albumartist}/{originalyear} - {album}",
    "{albumartist}/{album}",
    "{album} ({originalyear})",
    "{originalyear} - {album}",
    "{album}",
];

/// File templates tried: a number, a separator, then artist and title or the title alone; or no number.
fn file_candidates() -> Vec<String> {
    let numbers = ["{disc-}{track:02}", "{track:02}", "{disc-}{track}"];
    let separators = [" ", " - ", ". ", "-"];
    let bodies = ["{artist} - {title}", "{title}"];
    let mut out = Vec::new();
    for body in bodies {
        for number in numbers {
            for sep in separators {
                out.push(format!("{number}{sep}{body}"));
            }
        }
    }
    out.extend(bodies.iter().map(|b| b.to_string()));
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Folder,
    File,
}

/// Why a real name differs from the template's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    /// A tag the template needs is missing.
    MissingTags,
    /// The year is the other date tag's (`date` where the template says original year, or the reverse).
    YearSource,
    /// The disc prefix: present where the album has one disc, or missing on several.
    MultiDisc,
    /// Only letter case differs.
    Case,
    /// Only punctuation, spaces or symbols differ (another replacement for a character).
    Characters,
    /// The name says something the tags do not.
    TagsDiffer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Share {
    pub matched: usize,
    pub total: usize,
}

impl Share {
    pub fn ratio(&self) -> f64 {
        if self.total == 0 { 0.0 } else { self.matched as f64 / self.total as f64 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CauseCount {
    pub kind: Kind,
    pub cause: Cause,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mismatch {
    pub kind: Kind,
    pub cause: Cause,
    /// The album's folder.
    pub folder: String,
    /// The real name: the folder, or the file's name.
    pub name: String,
    /// What the template makes of the tags; none when a tag is missing.
    pub rendered: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Inference {
    pub template: Template,
    pub folders: Share,
    pub files: Share,
    /// Every mismatch counted by kind and cause.
    pub causes: Vec<CauseCount>,
    /// A few of each kind and cause.
    pub samples: Vec<Mismatch>,
}

/// The template a library's albums follow best, judged folder and file name apart: the candidate with the most
/// exact matches. Candidates are in order of likelihood, and the first within [`MARGIN`] of the best is taken, so noise
/// does not overturn a likelier convention (on the reference library `{year}` beats `{originalyear}` by one folder of
/// 2,230). A library with no albums gets the default.
pub fn infer(albums: &[Album]) -> Inference {
    let facts: Vec<AlbumFacts> = albums.iter().map(|a| AlbumFacts::of(&a.tracks)).collect();
    let folder_of = |a: &Album| a.folder.trim_matches('/').to_string();

    let folders: Vec<Vec<Name>> = FOLDER_CANDIDATES.iter().map(|c| parse_folders(c).expect("a valid candidate")).collect();
    let folder_score = |candidate: &Vec<Name>| {
        albums
            .iter()
            .zip(&facts)
            .filter(|(a, f)| render_folders(candidate, f, false).is_ok_and(|r| r == folder_of(a)))
            .count()
    };
    let files: Vec<Name> = file_candidates().iter().map(|c| parse_name(c).expect("a valid candidate")).collect();
    let file_score = |candidate: &Name| {
        albums
            .iter()
            .zip(&facts)
            .flat_map(|(a, f)| a.tracks.iter().map(move |t| (t, f)))
            .filter(|(t, f)| render_file(candidate, f, t, f.multi_disc).is_ok_and(|r| r == t.file))
            .count()
    };
    let file_total = albums.iter().map(|a| a.tracks.len()).sum();
    let best_folder = best(&folders, albums.len(), folder_score);
    let best_file = best(&files, file_total, file_score);
    let template = Template { folders: folders[best_folder].clone(), file: files[best_file].clone() };
    measure(template, albums, &facts)
}

/// How closely a library's albums follow `template`, as [`infer`] measures the one it picks: what the Settings page
/// shows for a template typed there.
pub fn check(template: &Template, albums: &[Album]) -> Inference {
    let facts: Vec<AlbumFacts> = albums.iter().map(|a| AlbumFacts::of(&a.tracks)).collect();
    measure(template.clone(), albums, &facts)
}

/// One album named by `template`, as `folder/first file`: the first album it can name.
pub fn example(template: &Template, albums: &[Album]) -> Option<String> {
    albums.iter().find_map(|a| {
        let names = template.render(&a.tracks).ok()?;
        Some(format!("{}/{}", names.folder, names.files.first()?))
    })
}

fn measure(template: Template, albums: &[Album], facts: &[AlbumFacts]) -> Inference {
    let folder_of = |a: &Album| a.folder.trim_matches('/').to_string();
    let mut folder_share = Share { matched: 0, total: albums.len() };
    let mut file_share = Share::default();
    let mut counts: BTreeMap<(Kind, Cause), usize> = BTreeMap::new();
    let mut samples = Vec::new();
    let mut note = |kind: Kind, cause: Cause, folder: &str, name: &str, rendered: Option<String>| {
        let n = counts.entry((kind, cause)).or_default();
        *n += 1;
        if *n <= SAMPLES {
            samples.push(Mismatch { kind, cause, folder: folder.to_string(), name: name.to_string(), rendered });
        }
    };
    for (album, f) in albums.iter().zip(facts) {
        let real = folder_of(album);
        match render_folders(&template.folders, f, false) {
            Ok(r) if r == real => folder_share.matched += 1,
            Ok(r) => {
                let other_year = render_folders(&template.folders, f, true).is_ok_and(|y| y == real);
                let cause = if other_year { Cause::YearSource } else { compare(&real, &r) };
                note(Kind::Folder, cause, &real, &real, Some(r));
            }
            Err(_) => note(Kind::Folder, Cause::MissingTags, &real, &real, None),
        }
        for track in &album.tracks {
            file_share.total += 1;
            match render_file(&template.file, f, track, f.multi_disc) {
                Ok(r) if r == track.file => file_share.matched += 1,
                Ok(r) => {
                    let other_disc = render_file(&template.file, f, track, !f.multi_disc).is_ok_and(|d| d == track.file);
                    let cause = if other_disc { Cause::MultiDisc } else { compare(&track.file, &r) };
                    note(Kind::File, cause, &real, &track.file, Some(r));
                }
                Err(_) => note(Kind::File, Cause::MissingTags, &real, &track.file, None),
            }
        }
    }
    let causes = counts.into_iter().map(|((kind, cause), count)| CauseCount { kind, cause, count }).collect();
    Inference { template, folders: folder_share, files: file_share, causes, samples }
}

/// The first candidate whose score is within the margin of the highest.
fn best<T>(candidates: &[T], total: usize, score: impl Fn(&T) -> usize) -> usize {
    let margin = (total as f64 * MARGIN) as usize;
    let scores: Vec<usize> = candidates.iter().map(score).collect();
    let top = scores.iter().copied().max().unwrap_or(0);
    scores.iter().position(|s| s + margin >= top).unwrap_or(0)
}

fn compare(real: &str, rendered: &str) -> Cause {
    let fold = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    if real.to_lowercase() == rendered.to_lowercase() {
        Cause::Case
    } else if fold(real) == fold(rendered) {
        Cause::Characters
    } else {
        Cause::TagsDiffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(file: &str, tags: &[(&str, &str)]) -> Track {
        Track { file: file.into(), tags: Tags(tags.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect()) }
    }

    /// A track of an album: albumartist, album, originaldate, date, disc, track number, artist, title.
    #[allow(clippy::too_many_arguments)]
    fn t(aa: &str, album: &str, od: &str, date: &str, disc: &str, n: &str, artist: &str, title: &str) -> Track {
        let mut tags = vec![("album", album), ("artist", artist), ("title", title), ("tracknumber", n)];
        for (k, v) in [("albumartist", aa), ("originaldate", od), ("date", date), ("discnumber", disc)] {
            if !v.is_empty() {
                tags.push((k, v));
            }
        }
        track("in.flac", &tags)
    }

    fn dark_side() -> Vec<Track> {
        vec![
            t("Pink Floyd", "The Dark Side of the Moon", "1973-03-01", "2011", "1/1", "1/10", "Pink Floyd", "Speak to Me"),
            t("Pink Floyd", "The Dark Side of the Moon", "1973-03-01", "2011", "1/1", "2/10", "Pink Floyd", "Breathe"),
        ]
    }

    #[test]
    fn the_default_template_reads_and_writes_back() {
        let template: Template = DEFAULT.parse().unwrap();
        assert_eq!(template.to_string(), DEFAULT);
        assert_eq!(Template::default(), template);
        assert_eq!(serde_json::to_string(&template).unwrap(), format!("\"{}\"", DEFAULT.replace('"', "\\\"")));
    }

    #[test]
    fn templates_are_written_in_one_form() {
        let template: Template = " {AlbumArtist}/{album}/{disc:1}-{track:2} {title} ".parse().unwrap();
        assert_eq!(template.to_string(), "{albumartist}/{album}/{disc:01}-{track:02} {title}");
    }

    #[test]
    fn invalid_templates_say_why() {
        let err = |s: &str| s.parse::<Template>().unwrap_err();
        assert_eq!(err("  "), TemplateError::Empty);
        assert_eq!(err("{album} {title}"), TemplateError::NoFolder);
        assert_eq!(err("{album}/{tittle}"), TemplateError::UnknownToken("tittle".into()));
        assert_eq!(err("{album}/{title"), TemplateError::Unclosed);
        assert_eq!(err("{album}/{ti{tle}"), TemplateError::Unclosed);
        assert_eq!(err("{album}/title}"), TemplateError::Unopened);
        assert_eq!(err("{album}: {albumartist}/{title}"), TemplateError::IllegalChar(':'));
        assert_eq!(err("{album}/{title}?"), TemplateError::IllegalChar('?'));
        assert_eq!(err("{album}//{title}"), TemplateError::EmptyName);
        assert_eq!(err("{album}/../{title}"), TemplateError::EmptyName);
        assert_eq!(err("{albumartist}/{title}"), TemplateError::FolderNeedsAlbum);
        assert_eq!(err("{album} {track}/{title}"), TemplateError::NotOfAlbum("track".into()));
        assert_eq!(err("{album}/{artist}"), TemplateError::FileNeedsTitleOrTrack);
        assert_eq!(err("{album}/{title:02}"), TemplateError::Width("title:02".into()));
        assert_eq!(err("{album}/{track:x}"), TemplateError::Width("track:x".into()));
    }

    #[test]
    fn an_album_is_named_by_the_template() {
        let names = Template::default().render(&dark_side()).unwrap();
        assert_eq!(names.folder, "Pink Floyd - The Dark Side of the Moon (1973)");
        assert_eq!(names.files, ["01 Pink Floyd - Speak to Me.flac", "02 Pink Floyd - Breathe.flac"]);
    }

    #[test]
    fn the_disc_prefix_is_only_on_albums_of_several_discs() {
        let wall = vec![
            t("Pink Floyd", "The Wall", "1979", "", "1/2", "3", "Pink Floyd", "Another Brick in the Wall, Part 1"),
            t("Pink Floyd", "The Wall", "1979", "", "2/2", "11", "Pink Floyd", "Comfortably Numb"),
        ];
        let names = Template::default().render(&wall).unwrap();
        assert_eq!(names.files, ["1-03 Pink Floyd - Another Brick in the Wall, Part 1.flac", "2-11 Pink Floyd - Comfortably Numb.flac"]);
        // One disc of two present: no prefix (the reference library names by what is there).
        let names = Template::default().render(&wall[..1]).unwrap();
        assert_eq!(names.files, ["03 Pink Floyd - Another Brick in the Wall, Part 1.flac"]);
    }

    #[test]
    fn the_year_is_the_original_one_and_0000_is_none() {
        let mut album = dark_side();
        assert!(Template::default().render(&album).unwrap().folder.ends_with("(1973)"));
        let by_date: Template = "{albumartist} - {album} ({year})/{track:02} {title}".parse().unwrap();
        assert!(by_date.render(&album).unwrap().folder.ends_with("(2011)"));
        for tr in &mut album {
            tr.tags.0.retain(|(n, _)| n != "originaldate");
            tr.tags.0.push(("originaldate".into(), "0000".into()));
        }
        assert!(Template::default().render(&album).unwrap().folder.ends_with("(2011)"));
        for tr in &mut album {
            tr.tags.0.retain(|(n, _)| n != "date");
        }
        assert_eq!(
            Template::default().render(&album),
            Err(RenderError::Missing { token: "originalyear", track: None })
        );
    }

    #[test]
    fn missing_track_numbers_and_duplicate_names_are_refused() {
        let mut album = dark_side();
        album[1].tags.0.retain(|(n, _)| n != "tracknumber");
        assert_eq!(Template::default().render(&album), Err(RenderError::Missing { token: "track", track: Some(1) }));
        let titles: Template = "{album}/{title}".parse().unwrap();
        let mut album = dark_side();
        album[0].tags.0.retain(|(n, _)| n != "title");
        album[0].tags.0.push(("title".into(), "breathe".into()));
        assert_eq!(titles.render(&album), Err(RenderError::Duplicate("Breathe.flac".into())));
    }

    #[test]
    fn a_slash_is_part_of_the_value_and_becomes_an_underscore() {
        assert_eq!(clean("yes/and"), "yes_and");
        assert_eq!(clean("AC/DC"), "AC_DC");
        assert_eq!(clean("Low Flung / Other Joe"), "Low Flung _ Other Joe");
        assert_eq!(clean("Improvisations And Edits Tokyo 09/26/2001"), "Improvisations And Edits Tokyo 09_26_2001");
        assert_eq!(clean("King Tubby meets Lee 'Scratch' Perry"), "King Tubby meets Lee 'Scratch' Perry");
        assert_eq!(clean("Simon & Garfunkel"), "Simon & Garfunkel");
    }

    #[test]
    fn a_repeated_tag_counts_by_its_first_value() {
        let tr = track("x.mp3", &[("albumartist", "Various Artists"), ("albumartist", "Various"), ("albumartist", "Various Artists")]);
        assert_eq!(text(&tr.tags, "albumartist").unwrap(), "Various Artists");
        let tr = track("x.mp3", &[("artist", " "), ("artist", "Cos/Mes")]);
        assert_eq!(text(&tr.tags, "artist").unwrap(), "Cos_Mes");
    }

    #[test]
    fn characters_windows_cannot_take_are_replaced() {
        assert_eq!(clean("Sgt. Pepper: Reprise"), "Sgt. Pepper_ Reprise");
        assert_eq!(clean("Are You Experienced?"), "Are You Experienced_");
        assert_eq!(clean("The \"Heroes\" Sessions"), "The _Heroes_ Sessions");
        assert_eq!(clean("F*ck <this> | \\that"), "F_ck _this_ _ _that");
        assert_eq!(clean("Tab\there\u{0}\nnext"), "Tab here next");
        assert_eq!(clean("Björk – Homogenic  (Live)…"), "Björk – Homogenic  (Live)…");
    }

    #[test]
    fn name_edges_and_length_are_safe() {
        assert_eq!(finish("R.E.M.", ""), "R.E.M_");
        assert_eq!(finish("Howie B. ", ""), "Howie B_");
        assert_eq!(finish("01 R.E.M. - Stand.", "mp3"), "01 R.E.M. - Stand..mp3");
        assert_eq!(finish("...And You Will Know Us by the Trail of Dead", ""), "_..And You Will Know Us by the Trail of Dead");
        assert_eq!(finish(" . ", ""), "_");
        assert_eq!(finish("con", "mp3"), "con_.mp3");
        assert_eq!(finish("NUL.x", ""), "NUL_.x");
        assert_eq!(finish("Com1 ", ""), "Com1_");
        assert_eq!(finish("Concerto", ""), "Concerto");
        let long = format!("01 {}", "é".repeat(200));
        let name = finish(&long, "flac");
        assert!(name.len() <= MAX_NAME_BYTES && name.ends_with("é.flac"), "{name}");
        let folder = finish(&format!("{} .x", "a".repeat(MAX_NAME_BYTES - 1)), "");
        assert_eq!(folder, "a".repeat(MAX_NAME_BYTES - 1));
    }

    #[test]
    fn vinyl_sides_keep_their_track_numbers() {
        let side = vec![track("a.flac", &[("albumartist", "Kraftwerk"), ("album", "Autobahn"), ("date", "1974"), ("tracknumber", "A1"), ("artist", "Kraftwerk"), ("title", "Autobahn")])];
        assert_eq!(Template::default().render(&side).unwrap().files, ["A1 Kraftwerk - Autobahn.flac"]);
    }

    #[test]
    fn an_album_without_albumartist_takes_its_artist_or_various() {
        let mut solo = dark_side();
        for tr in &mut solo {
            tr.tags.0.retain(|(n, _)| n != "albumartist");
        }
        assert!(Template::default().render(&solo).unwrap().folder.starts_with("Pink Floyd - "));
        solo[1].tags.0.push(("artist".into(), "Roger Waters".into()));
        solo[1].tags.0.retain(|(n, v)| n != "artist" || v != "Pink Floyd");
        assert_eq!(Template::default().render(&solo).unwrap().folder, "Various Artists - The Dark Side of the Moon (1973)");
    }

    #[test]
    fn the_folder_takes_what_most_tracks_say() {
        let mut album = dark_side();
        album.push(t("Pink Floyd", "Dark Side", "1973", "", "", "3", "Pink Floyd", "On the Run"));
        album.push(t("Pink Floyd", "The Dark Side of the Moon", "1973", "", "", "4", "Pink Floyd", "Time"));
        assert_eq!(Template::default().render(&album).unwrap().folder, "Pink Floyd - The Dark Side of the Moon (1973)");
    }

    /// A small library named by `template`, with some names off in known ways.
    fn library(template: &Template) -> Vec<Album> {
        let albums = [
            vec![
                t("Pink Floyd", "The Wall", "1979", "", "1", "1", "Pink Floyd", "In the Flesh?"),
                t("Pink Floyd", "The Wall", "1979", "", "2", "1", "Pink Floyd", "Hey You"),
            ],
            dark_side(),
            vec![
                t("Miles Davis", "Kind of Blue", "1959", "", "", "1", "Miles Davis", "So What"),
                t("Miles Davis", "Kind of Blue", "1959", "", "", "2", "Miles Davis", "Freddie Freeloader"),
            ],
            vec![t("Massive Attack", "Mezzanine", "1998", "", "", "1", "Massive Attack", "Angel")],
            vec![t("Daft Punk", "Discovery", "2001", "", "", "1", "Daft Punk", "One More Time")],
            vec![t("Radiohead", "OK Computer", "1997", "2017", "", "1", "Radiohead", "Airbag")],
        ];
        albums
            .into_iter()
            .map(|tracks| {
                let names = template.render(&tracks).unwrap();
                let tracks = tracks.into_iter().zip(names.files).map(|(t, file)| Track { file, ..t }).collect();
                Album { folder: names.folder, tracks }
            })
            .collect()
    }

    #[test]
    fn inference_finds_the_template_and_explains_the_rest() {
        let template = Template::default();
        let mut albums = library(&template);
        albums[2].tracks[1].file = "02 Miles Davis - Freddie FreeLoader.flac".into(); // case
        albums[3].tracks[0].file = "01 Massive Attack - Angel (Remastered).flac".into(); // tags differ
        albums[4].folder = "Daft Punk - Discovery- (2001)".into(); // characters
        albums[5].folder = "Radiohead - OK Computer (2017)".into(); // year from date
        albums[0].tracks[0].file = "01 Pink Floyd - In the Flesh_.flac".into(); // disc prefix missing
        albums[1].tracks[0].tags.0.retain(|(n, _)| n != "title"); // no title

        let found = infer(&albums);
        assert_eq!(found.template, template);
        assert_eq!(found.folders, Share { matched: 4, total: 6 });
        assert_eq!(found.files, Share { matched: 5, total: 9 });
        let causes: Vec<(Kind, Cause, usize)> = found.causes.iter().map(|c| (c.kind, c.cause, c.count)).collect();
        assert_eq!(
            causes,
            [
                (Kind::Folder, Cause::YearSource, 1),
                (Kind::Folder, Cause::Characters, 1),
                (Kind::File, Cause::MissingTags, 1),
                (Kind::File, Cause::MultiDisc, 1),
                (Kind::File, Cause::Case, 1),
                (Kind::File, Cause::TagsDiffer, 1),
            ]
        );
        let disc = found.samples.iter().find(|m| m.cause == Cause::MultiDisc).unwrap();
        assert_eq!(disc.rendered.as_deref(), Some("1-01 Pink Floyd - In the Flesh_.flac"));
        assert_eq!(disc.folder, "Pink Floyd - The Wall (1979)");
    }

    #[test]
    fn inference_recognises_other_layouts() {
        for s in [
            "{albumartist}/{album} ({originalyear})/{track:02} - {title}",
            "{albumartist} - {album}/{disc-}{track:02}. {title}",
            "{album} ({originalyear})/{artist} - {title}",
        ] {
            let template: Template = s.parse().unwrap();
            let found = infer(&library(&template));
            assert_eq!(found.template.to_string(), s);
            assert_eq!(found.folders.ratio(), 1.0);
            assert_eq!(found.files.ratio(), 1.0);
            assert!(found.causes.is_empty());
        }
    }

    #[test]
    fn a_later_candidate_must_win_clearly() {
        // 200 albums; `reissued` of them carry a later date, and `by_date` of those are named by it.
        let library = |reissued: usize, by_date: usize| -> Vec<Album> {
            (0..200)
                .map(|i| {
                    let date = if i < reissued { "2015" } else { "1990" };
                    let tracks = vec![t("Band", &format!("Album {i}"), "1990", date, "", "1", "Band", "Song")];
                    let year = if i < by_date { date } else { "1990" };
                    Album { folder: format!("Band - Album {i} ({year})"), tracks }
                })
                .collect()
        };
        // Named by date 2 to 1: within the margin (1 of 200), the original year stays.
        assert_eq!(infer(&library(3, 2)).template, Template::default());
        // Named by date 10 to 0: the date wins.
        let found = infer(&library(10, 10));
        assert_eq!(found.template.to_string(), DEFAULT.replace("originalyear", "year"));
        assert_eq!(found.folders, Share { matched: 200, total: 200 });
    }

    #[test]
    fn an_empty_library_gets_the_default() {
        let found = infer(&[]);
        assert_eq!(found.template, Template::default());
        assert_eq!(found.folders, Share::default());
        assert_eq!(found.folders.ratio(), 0.0);
    }

    #[test]
    fn every_candidate_is_a_valid_template() {
        for folder in FOLDER_CANDIDATES {
            for file in file_candidates() {
                let s = format!("{folder}/{file}");
                assert_eq!(s.parse::<Template>().unwrap().to_string(), s);
            }
        }
    }
}
