//! Album lists read from a server's libraries, for the pages that list albums (Incoming, Collection): the albums,
//! newest first, each with its track count, format and lowest bitrate.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::library::{Active, Library};
use crate::source::{self, Source};

/// Which catalogue folders hold a list's albums: the Music folder (catalogue folder 0) or the Incoming ones (1 on).
/// Both can hold an album directory of the same name, so a list looks only at its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shelf {
    Music,
    Incoming,
}

/// What the catalogue says of each album directory, by catalogue folder.
type Seen = HashMap<i64, HashMap<String, source::AlbumDetails>>;

/// The details of the album at a server's `path`, among these catalogue folders' directories: the server's paths and
/// Cratarr's differ in front, so the longest ending that is one.
fn details_at(seen: &Seen, folders: &[i64], path: &str) -> Option<source::AlbumDetails> {
    let dirs: Vec<&HashMap<String, source::AlbumDetails>> = folders.iter().filter_map(|f| seen.get(f)).collect();
    path.match_indices('/').find_map(|(i, _)| dirs.iter().find_map(|d| d.get(&path[i + 1..]))).cloned()
}

/// One read of a server's albums.
pub(super) struct ShelfRead {
    /// False when the server cannot list albums at all.
    pub supported: bool,
    pub albums: Vec<source::Album>,
    pub error: Option<String>,
}

/// Reads album lists, keeping what had to be asked of a server (see `read`).
pub(super) struct Shelves {
    lib: Arc<Library>,
    /// Album details asked of a source, by source and album id: (the source's stamp then, details).
    details: Mutex<HashMap<String, (Option<String>, source::AlbumDetails)>>,
    /// Where `details` is kept between restarts.
    details_file: Option<PathBuf>,
}

impl Shelves {
    pub(super) fn new(lib: Arc<Library>, data_dir: Option<&str>) -> Self {
        let details_file = data_dir.map(|d| PathBuf::from(d).join("incoming-albums.json"));
        let details = details_file.as_ref().and_then(|f| std::fs::read(f).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { lib, details: Mutex::new(details), details_file }
    }

    /// The albums of `libraries` on one server, newest first, at most `limit` from each library.
    pub(super) async fn read(&self, a: &Active, s: &dyn Source, libraries: &[String], shelf: Shelf, limit: usize) -> ShelfRead {
        let supported = s.lists_albums();
        let mut listed = Vec::new();
        let mut error = None;
        if supported {
            for library in libraries {
                match s.albums(library, limit).await {
                    // Each knows its library, for the pages' library switches.
                    Ok(list) => listed.extend(list.into_iter().map(|mut l| {
                        l.album.library = Some(library.clone());
                        l
                    })),
                    Err(e) => {
                        error.get_or_insert(format!("{e:#}"));
                    }
                }
            }
        }
        let mut albums = self.with_details(a, s, listed, shelf).await;
        albums.sort_by_key(|a: &source::Album| std::cmp::Reverse(a.added.unwrap_or(0)));
        ShelfRead { supported, albums, error }
    }

    /// Each album with its track count, format, lowest bitrate and length: from the catalogue where Cratarr sees the
    /// album's folder (no request), else as kept from before while the source has not saved the album since, else asked
    /// of the source, one album at a time. Asking is costly for Jellyfin (all 246 albums at once after a restart took it to
    /// 280% CPU and made it time out, 2026-10-13), so what was asked is kept, also across restarts.
    async fn with_details(&self, a: &Active, s: &dyn Source, listed: Vec<source::Listed>, shelf: Shelf) -> Vec<source::Album> {
        let source_id = a.cfg.id.as_str();
        let c = self.lib.catalogue();
        let in_use = c.folders();
        // The folders of what is kept (main libraries and own music) are Music's shelf, the others Incoming's: by what they
        // are, never by their place (0101 point 1: a folder's id says nothing of its kind).
        let kept = self.lib.kept_folders();
        // By catalogue folder and album directory (relative to it): two libraries can hold one directory, and with a record
        // in both that is the likely case, each with its own format. A server's path is matched by its longest such ending.
        let seen: Seen = tokio::task::spawn_blocking(move || {
            let folders: Vec<i64> = c.folders().ids().filter(|id| kept.contains(id) == matches!(shelf, Shelf::Music)).collect();
            let mut out = Seen::new();
            for folder in folders {
                for f in c.album_facts(folder).unwrap_or_default() {
                    if f.dir.is_empty() {
                        continue;
                    }
                    let format = (f.formats.len() == 1).then(|| f.formats.iter().next().cloned()).flatten();
                    let duration_secs = f.duration_ms.map(|ms| ms as f64 / 1000.0);
                    out.entry(folder).or_default().insert(f.dir, source::AlbumDetails { tracks: f.tracks, format, bitrate_kbps: f.min_kbps, duration_secs });
                }
            }
            out
        })
        .await
        .unwrap_or_default();
        // A library's catalogue folders: what its album's path is looked for in, and nowhere else.
        let folders_of = |library: Option<&str>| -> Vec<i64> {
            let placed = a.placement.libraries.iter().find(|l| Some(l.id.as_str()) == library);
            let holds = |path: &std::path::Path| placed.is_some_and(|l| l.folders.iter().any(|f| std::path::Path::new(f) == path));
            in_use.iter().filter(|(_, p)| holds(p)).map(|(id, _)| id).collect()
        };
        let key = |id: &str| format!("{source_id}\n{id}");
        let mut albums = Vec::with_capacity(listed.len());
        let mut asked = false;
        for source::Listed { mut album, stamp } in listed {
            // Cratarr's own lists each copy as it is: what is kept and what it says of an album are by record, so the row copy's.
            let own = !album.copies.is_empty();
            let kept = self.details.lock().unwrap_or_else(|p| p.into_inner()).get(&key(&album.id)).filter(|(s, _)| !own && *s == stamp).map(|(_, d)| d.clone());
            let found = album.path.as_deref().and_then(|p| details_at(&seen, &folders_of(album.library.as_deref()), p));
            let details = match found.or(kept) {
                Some(d) => Some(d),
                None if own => None,
                None => match s.album_summary(&album.id).await {
                    Ok(Some(d)) => {
                        self.details.lock().unwrap_or_else(|p| p.into_inner()).insert(key(&album.id), (stamp.clone(), d.clone()));
                        asked = true;
                        Some(d)
                    }
                    Ok(None) => None,
                    Err(e) => {
                        tracing::warn!("tracks of album {}: {e:#}", album.id);
                        None
                    }
                },
            };
            if let Some(d) = details {
                album.tracks = d.tracks;
                album.format = d.format;
                album.bitrate_kbps = d.bitrate_kbps;
                // Details kept from before lengths were kept have none: the listing's own then.
                album.duration_secs = d.duration_secs.or(album.duration_secs);
            }
            albums.push(album);
        }
        if asked {
            self.save_details();
        }
        albums
    }

    /// Keep the asked album details on disk (see `with_details`); best effort.
    fn save_details(&self) {
        let Some(file) = self.details_file.as_ref() else { return };
        let body = match serde_json::to_vec(&*self.details.lock().unwrap_or_else(|p| p.into_inner())) {
            Ok(b) => b,
            Err(e) => return tracing::warn!("album details: {e:#}"),
        };
        let tmp = file.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, body).and_then(|()| std::fs::rename(&tmp, file)) {
            tracing::warn!("cannot save {}: {e:#}", file.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record in two libraries most likely lies at one path in each: keyed by that path alone, one copy would show the
    /// other's format and bitrate, and the card's quality would be a copy that does not play.
    #[test]
    fn two_libraries_with_one_folder_name_keep_their_own_details() {
        let details = |format: &str| source::AlbumDetails { tracks: 2, format: Some(format.into()), bitrate_kbps: None, duration_secs: None };
        let dir = |d: source::AlbumDetails| HashMap::from([("A/Album".to_string(), d)]);
        let seen: Seen = HashMap::from([(0, dir(details("MP3"))), (1, dir(details("FLAC")))]);
        let at = |folders: &[i64], path: &str| details_at(&seen, folders, path).and_then(|d| d.format);
        assert_eq!(at(&[0], "/music/A/Album").as_deref(), Some("MP3"));
        assert_eq!(at(&[1], "/data/own/ann/A/Album").as_deref(), Some("FLAC"), "the server's path differs in front");
        assert_eq!(at(&[2], "/music/A/Album"), None, "a library of neither");
        assert_eq!(at(&[0], "/music/B/Album"), None);
    }
}
