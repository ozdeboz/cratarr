//! `renumber`'s file name and its reading of a Discogs release, in base: a track's file name as the sheets show it, which
//! the words' writer names too, and a Discogs release's media and lengths, as records' references and Discogs read them.
//! And a record's tracks as the library has them, read as the requests' and the intake card's readers read them, and titles
//! compared as Renumber compares them. `renumber` re-exports them.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;

pub fn file_name(rel: &str) -> String {
    Path::new(rel).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| rel.to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReleaseFacts {
    pub title: String,
    pub format: String,
    pub country: Option<String>,
    pub year: Option<u32>,
}

/// A release's medium: its tracks' titles and lengths (s), in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Medium {
    pub format: String,
    pub tracks: Vec<(String, Option<u32>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// MusicBrainz's id (none for a Discogs release).
    pub id: String,
    pub facts: ReleaseFacts,
    pub media: Vec<Medium>,
}

/// "4:05" as seconds.
pub fn duration(s: &str) -> Option<u32> {
    let mut secs = 0u32;
    for part in s.trim().split(':') {
        secs = secs * 60 + part.trim().parse::<u32>().ok()?;
    }
    (secs > 0).then_some(secs)
}

/// A Discogs tracklist position's medium: "2-5" disc 2, "B3" side B, "3" the one.
pub fn medium_of(position: &str) -> String {
    let p = position.trim();
    if let Some((d, _)) = p.split_once(['-', '.']) {
        return d.to_string();
    }
    p.chars().take_while(|c| c.is_ascii_alphabetic()).collect()
}

pub fn discogs_release(v: &Value) -> Release {
    let format = v["formats"].as_array().and_then(|f| f.first()).and_then(|f| f["name"].as_str()).unwrap_or_default().to_string();
    let mut media: Vec<(String, Medium)> = Vec::new();
    for t in v["tracklist"].as_array().into_iter().flatten().filter(|t| t["type_"].as_str().is_none_or(|k| k == "track")) {
        let key = medium_of(t["position"].as_str().unwrap_or_default());
        let track = (t["title"].as_str().unwrap_or_default().to_string(), t["duration"].as_str().and_then(duration));
        match media.last_mut() {
            Some((k, m)) if *k == key => m.tracks.push(track),
            _ => media.push((key, Medium { format: format.clone(), tracks: vec![track] })),
        }
    }
    Release {
        id: String::new(),
        facts: ReleaseFacts {
            title: v["title"].as_str().unwrap_or_default().to_string(),
            format,
            country: v["country"].as_str().map(str::to_string),
            year: v["year"].as_u64().filter(|y| *y > 0).map(|y| y as u32),
        },
        media: media.into_iter().map(|(_, m)| m).collect(),
    }
}

/// One of the album's tracks as the library has it.
#[derive(Debug, Clone)]
pub struct Track {
    pub id: String,
    pub path: PathBuf,
    /// Its catalogue folder and its path in it.
    pub place: (i64, String),
    pub file: String,
    pub title: String,
    pub secs: u32,
    pub disc: Option<u32>,
    pub track: Option<u32>,
}

/// Titles compared as Renumber compares them: the domain's, the one title rule's base (PAIR01, C4).
pub use cratarr_domain::identity::same_title;

/// A record's tracks to read: one file a track, the one that plays. What writes goes by `files_in`.
pub fn tracks_of(album: &str) -> Result<(Vec<Track>, Option<String>, String, String)> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let (mbid, artist, name) = served.library.album_names(album)?.context("No such album in Cratarr's own library")?;
    let tracks = row_tracks(&served.library, album)?;
    Ok((tracks, mbid.filter(|m| crate::userdata::is_mbid(m)), artist, name))
}

/// `tracks_of`'s reading of the `songs` rows.
fn row_tracks(lib: &crate::serve::library::Library, album: &str) -> Result<Vec<Track>> {
    let roots = lib.roots();
    lib.album_tracks(album)?
        .into_iter()
        .map(|(id, folder, rel, title, ms, disc, track)| {
            let root = roots.iter().find(|(f, _)| *f == folder).map(|(_, r)| r).context("Its library is not there")?;
            Ok(Track { id, file: file_name(&rel), path: Path::new(root).join(&rel), place: (folder, rel), title, secs: (ms / 1000) as u32, disc, track })
        })
        .collect()
}

/// What a writer writes to: every file of a record, or of its copy at (catalogue folder, folder in it) alone. A track's files
/// come together, in the album's order, the one that plays first, and read as that track. A file on no track of the row copy
/// (a copy with tracks of its own, until they are taken) comes last, as its own tags say.
pub fn files_in(served: &crate::serve::Serve, album: &str, at: Option<&(i64, String)>) -> Result<Vec<Track>> {
    let (lib, cat) = (&served.library, served.catalogue());
    if let Some((folder, dir)) = at {
        return copy_tracks(lib, cat, album, *folder, dir);
    }
    let rows = row_tracks(lib, album)?;
    let roots = lib.roots();
    let mut read: std::collections::HashMap<(i64, String), Tagged> = std::collections::HashMap::new();
    let mut out = rows.clone();
    for (id, folder, rel, ms) in lib.files_of_record(album)? {
        if rows.iter().any(|t| t.place.0 == folder && t.place.1 == rel) {
            continue;
        }
        let root = roots.iter().find(|(f, _)| *f == folder).map(|(_, r)| r).context("Its library is not there")?;
        out.push(match rows.iter().find(|t| t.id == id) {
            Some(t) => Track { file: file_name(&rel), path: Path::new(root).join(&rel), place: (folder, rel), secs: (ms / 1000) as u32, ..t.clone() },
            None => {
                let dir = crate::catalogue::dir_of(&rel).to_string();
                if !read.contains_key(&(folder, dir.clone())) {
                    read.insert((folder, dir.clone()), tagged(cat, folder, &dir)?);
                }
                by_its_tags(root, id, folder, rel, ms, &read[&(folder, dir)])
            }
        });
    }
    out.sort_by_key(|t| rows.iter().position(|r| r.id == t.id).unwrap_or(rows.len()));
    Ok(out)
}

/// The tracks of a record's copy at (catalogue folder, folder in it), in its order, or of the record with none named. A copy
/// that does not play has no rows in `songs`: its files and what their tags say are all there is of it.
pub fn tracks_in(album: &str, at: Option<&(i64, String)>) -> Result<(Vec<Track>, Option<String>, String, String)> {
    let Some((folder, dir)) = at else { return tracks_of(album) };
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let (mbid, artist, name) = served.library.album_names(album)?.context("No such album in Cratarr's own library")?;
    let tracks = copy_tracks(&served.library, &served.catalogue(), album, *folder, dir)?;
    Ok((tracks, mbid.filter(|m| crate::userdata::is_mbid(m)), artist, name))
}

type Tagged = std::collections::HashMap<String, crate::naming::Tags>;

/// What the catalogue read from a folder's files, by path.
fn tagged(cat: &crate::catalogue::Catalogue, folder: i64, dir: &str) -> Result<Tagged> {
    Ok(cat.album_tags(folder, dir)?.into_iter().map(|(rel, t)| (rel, crate::naming::Tags(t))).collect())
}

/// A file as its own tags say it: a file with no row in `songs` has nothing else.
fn by_its_tags(root: &str, id: String, folder: i64, rel: String, ms: u64, tags: &Tagged) -> Track {
    let of = tags.get(&rel);
    let number = |name: &str| of.and_then(|t| t.first(name)).and_then(crate::serve::library::leading_number);
    let file = file_name(&rel);
    Track {
        id,
        path: Path::new(root).join(&rel),
        title: of.and_then(|t| t.first("title")).map_or_else(|| file.clone(), str::to_string),
        secs: (ms / 1000) as u32,
        disc: number("discnumber"),
        track: number("tracknumber"),
        file,
        place: (folder, rel),
    }
}

/// `tracks_in`'s reading of a copy's files.
pub fn copy_tracks(lib: &crate::serve::library::Library, cat: &crate::catalogue::Catalogue, album: &str, folder: i64, dir: &str) -> Result<Vec<Track>> {
    let root = lib.roots().into_iter().find(|(f, _)| *f == folder).map(|(_, r)| r).context("Its library is not there")?;
    let tags = tagged(cat, folder, dir)?;
    let mut tracks: Vec<Track> = lib.copy_songs(album, folder, dir)?.into_iter().map(|(id, rel, ms)| by_its_tags(&root, id, folder, rel, ms, &tags)).collect();
    tracks.sort_by(|a, b| (a.disc.unwrap_or(1), a.track, &a.place.1).cmp(&(b.disc.unwrap_or(1), b.track, &b.place.1)));
    Ok(tracks)
}
