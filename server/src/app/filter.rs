//! The Filter sheet's engine (decision 0061): a filter is a crate's conditions, unsaved, on the list in view. One
//! question for every list: given the conditions and what the list is, which of its items pass, how many of how many,
//! and the axes with their values and counts in this list (the sheet's chips, most common first; the ranges' span; the
//! tempo spread). Items that hold tracks (the Library's albums, the playlists) are tested with each condition's
//! quantifier, any or all of their tracks (`crates::passes`); in a list of tracks each track is an item of one.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{App, ToggleError};
use crate::crates::{self, Facts};

/// What the list in view is.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum List {
    /// The Library's albums on the server asked for, of the libraries switched on (`without`).
    Library,
    /// The Playlists page's list: the account's own playlists and those shared with it.
    Playlists,
    /// A playlist's tracks.
    Playlist { id: String },
    /// A crate's tracks as it is looked at: its toggles as given, else as saved; `without` from the request.
    Crate {
        id: String,
        #[serde(default)]
        favourites: Option<crates::Favourites>,
        #[serde(default)]
        favourite_albums: Option<crates::Favourites>,
    },
    /// An album's tracks, or an artist's, on their own server.
    Album { source: String, id: String },
    Artist { source: String, name: String },
}

#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub conditions: Vec<crates::Condition>,
    pub list: List,
    /// The libraries switched off (`source/library` keys): the Library's albums and a crate's tracks leave theirs out;
    /// a playlist, an album, an artist and the playlists are what they hold.
    #[serde(default)]
    pub without: Vec<String>,
}

/// What a list's items are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Items {
    Albums,
    Playlists,
    Tracks,
}

/// The answer: the items that pass by their keys (an album's id, a playlist's id, a track row's `entry`; none for a
/// crate, whose rows come filtered: `CrateLook::filter`), how many pass of how many, and the list's axes and tempo
/// spread, over all its items (what there is to choose from), not only those that pass.
#[derive(Debug, Serialize)]
pub struct Filtered {
    pub source: String,
    pub items: Items,
    pub keys: Vec<String>,
    pub passed: usize,
    pub total: usize,
    pub axes: Vec<crates::AxisView>,
    pub tempo: crates::TempoBands,
}

/// A line's facts; none for a line the catalogue has no file for (a missing one, or switched off).
fn facts_of<'a>(by_line: &HashMap<&str, &'a Facts>, none: &'a Facts, line: Option<&str>) -> &'a Facts {
    line.and_then(|l| by_line.get(l).copied()).unwrap_or(none)
}

/// Works a filter out on a list (`want`: the server, else the first connected; an album's or an artist's own).
pub async fn filter(app: &Arc<App>, want: Option<&str>, r: Request) -> Result<Filtered, ToggleError> {
    if let Some(c) = r.conditions.iter().find(|c| !crates::is_axis(&c.axis)) {
        return Err(ToggleError::Refused(format!("Unknown axis: {}.", c.axis)));
    }
    let want = match &r.list {
        List::Album { source, .. } | List::Artist { source, .. } => Some(source.as_str()),
        _ => want,
    };
    // A crate's own conditions, worked out on the same reading of the library as the filter's.
    let shown = match &r.list {
        List::Crate { id, favourites, favourite_albums } => {
            let look = super::CrateLook { favourites: *favourites, favourite_albums: *favourite_albums, ..Default::default() };
            app.playlists.crate_shown(id, &look).await?
        }
        _ => Vec::new(),
    };
    let all: Vec<crates::Condition> = shown.iter().chain(&r.conditions).cloned().collect();
    let drawn = matches!(r.list, List::Library | List::Crate { .. });
    let lib = app.playlists.crate_library_for(&all, want, if drawn { &r.without } else { &[] }).await?;
    let by_line: HashMap<&str, &Facts> = lib.files().map(|f| (f.rel.as_str(), f)).collect();
    let none = Facts::default();
    let one = |key: String, line: Option<&str>| (key, vec![facts_of(&by_line, &none, line)]);
    let (items, grouped): (Items, Vec<(String, Vec<&Facts>)>) = match &r.list {
        List::Crate { .. } => (Items::Tracks, lib.select(&shown).iter().map(|l| one(String::new(), Some(l.as_str()))).collect()),
        List::Playlist { id } => (Items::Tracks, app.playlists.entries(id).await?.into_iter().map(|l| one(l.clone(), Some(l.as_str()))).collect()),
        List::Playlists => {
            let lists = app.playlists.snapshot().await?;
            let of = |lines: &[String]| lines.iter().map(|l| facts_of(&by_line, &none, Some(l.as_str()))).collect();
            (Items::Playlists, lists.into_iter().map(|(id, lines, _)| (id, of(&lines))).collect())
        }
        List::Album { source, id } => {
            let rows = app.playing.album_view(source, id).await?.rows;
            (Items::Tracks, row_lines(app, source, rows).await.into_iter().map(|(e, l)| one(e, l.as_deref())).collect())
        }
        List::Artist { source, name } => {
            let rows = app.playing.artist_view(source, name).await?.rows;
            (Items::Tracks, row_lines(app, source, rows).await.into_iter().map(|(e, l)| one(e, l.as_deref())).collect())
        }
        List::Library => {
            // An album is a folder of files: its tracks are the files whose lines are in its folder's.
            let mut by_dir: HashMap<&str, Vec<&Facts>> = HashMap::new();
            for f in lib.files() {
                by_dir.entry(f.rel.rsplit_once('/').map_or("", |(d, _)| d)).or_default().push(f);
            }
            let albums = library_albums(app, &lib.source, &r.without).await;
            (Items::Albums, albums.into_iter().map(|(id, dir)| (id, dir.and_then(|d| by_dir.get(d.as_str()).cloned()).unwrap_or_default())).collect())
        }
    };
    let w = lib.world();
    let keys: Vec<String> = grouped.iter().filter(|(_, t)| crates::passes(t, &r.conditions, &w)).map(|(k, _)| k.clone()).collect();
    let passed = keys.len();
    let tracks: Vec<&[&Facts]> = grouped.iter().map(|(_, t)| t.as_slice()).collect();
    let axes = crates::list_axes(&tracks, &app.playlists.own_playlist_names().await?, Some(lib.membership()));
    let tempo = crates::tempo_bands(tracks.iter().flat_map(|t| t.iter().map(|f| f.bpm)));
    // A crate's rows are paged on the server: they come filtered there, so its thousands of lines are not sent here.
    let keys = if matches!(r.list, List::Crate { .. }) { Vec::new() } else { keys };
    Ok(Filtered { source: lib.source.clone(), items, keys, passed, total: grouped.len(), axes, tempo })
}

/// An album's or an artist's rows by their `entry`, each with its file's line (the catalogue's link of the server's
/// id), none for one it cannot place.
async fn row_lines(app: &Arc<App>, source: &str, rows: Vec<super::playlists::PlaylistRow>) -> Vec<(String, Option<String>)> {
    let (c, sid, ids) = (app.library.catalogue(), source.to_string(), rows.iter().map(|r| r.id.clone().unwrap_or_default()).collect::<Vec<_>>());
    let lines: Vec<Option<String>> = tokio::task::spawn_blocking(move || {
        ids.iter().map(|id| c.track_of_source(&sid, id).and_then(|t| c.file_of(t)).and_then(|(f, rel)| c.line_of(f, &rel))).collect()
    })
    .await
    .unwrap_or_default();
    rows.into_iter().map(|r| r.entry).zip(lines.into_iter().chain(std::iter::repeat(None))).collect()
}

/// The Library's albums on a server, of the libraries not switched off (as the Library page shows them), each with
/// the line of its folder, as its files' lines begin (none where the server's path for it is not known).
async fn library_albums(app: &Arc<App>, source: &str, without: &[String]) -> Vec<(String, Option<String>)> {
    let view = app.albums(Some(source)).await;
    let Some(s) = view.sources.into_iter().find(|s| s.id == source) else { return Vec::new() };
    let roots = app.library.connected().into_iter().find(|(a, _)| a.cfg.id == source).map(|(a, _)| a.placement.roots.clone()).unwrap_or_default();
    let cat = app.library.catalogue();
    let folders = cat.folders();
    s.albums
        .into_iter()
        .filter(|a| a.library.as_ref().is_none_or(|l| !without.contains(&format!("{source}/{l}"))))
        .map(|a| {
            let dir = a.path.as_deref().and_then(|p| folder_of(&roots, &folders, p)).and_then(|(i, rel)| cat.line_of(i, &rel));
            (a.id, dir)
        })
        .collect()
}

/// Where an album's folder is in the catalogue (folder, its path there), from the server's path for it: the server's
/// root for it exchanged for the folder Cratarr sees there (`Placement::roots`), or the path as it is (Cratarr's own
/// library names its folders as Cratarr sees them).
fn folder_of(roots: &[crate::placement::FolderRoot], folders: &[std::path::PathBuf], path: &str) -> Option<(i64, String)> {
    let path = path.replace('\\', "/");
    let path = path.trim_end_matches('/');
    let seen = roots
        .iter()
        .find_map(|r| {
            let rest = path.strip_prefix(r.root.trim_end_matches('/'))?;
            rest.starts_with('/').then(|| format!("{}{rest}", r.folder.trim_end_matches('/')))
        })
        .unwrap_or_else(|| path.to_string());
    let seen = std::path::Path::new(&seen);
    folders.iter().enumerate().find_map(|(i, f)| {
        let rel = seen.strip_prefix(f).ok()?.to_string_lossy().replace('\\', "/");
        (!rel.is_empty()).then_some((i as i64, rel))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_albums_folder_is_found_through_the_servers_root() {
        let roots = [crate::placement::FolderRoot { folder: "/media/Music".into(), root: "/data/music/".into() }];
        let folders = ["/media/Music", "/media/lidarr-music"].map(std::path::PathBuf::from);
        assert_eq!(folder_of(&roots, &folders, "/data/music/A/B (2020)/"), Some((0, "A/B (2020)".into())));
        assert_eq!(folder_of(&roots, &folders, "/media/lidarr-music/X/New"), Some((1, "X/New".into())), "Cratarr's own: as it is");
        assert_eq!(folder_of(&roots, &folders, "/data/musical/A"), None, "a root is a whole folder");
        assert_eq!(folder_of(&roots, &folders, "/media/Music"), None, "the folder itself is no album");
    }

    #[test]
    fn a_list_is_named_by_its_kind() {
        let r: Request = serde_json::from_value(serde_json::json!({
            "conditions": [{ "axis": "genre", "value": { "kind": "any_of", "values": ["Dub"] }, "quantifier": "all" }],
            "list": { "kind": "album", "source": "cratarr", "id": "al-1" },
            "without": ["cratarr//media/Incoming"]
        }))
        .unwrap();
        assert_eq!(r.list, List::Album { source: "cratarr".into(), id: "al-1".into() });
        assert_eq!((r.conditions[0].quantifier, r.conditions[0].enabled), (crates::Quantifier::All, true));
        let r: Request = serde_json::from_value(serde_json::json!({ "list": { "kind": "library" } })).unwrap();
        assert_eq!((r.list, r.conditions.len(), r.without.len()), (List::Library, 0, 0));
    }
}
