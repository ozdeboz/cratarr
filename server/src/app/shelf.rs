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
        let mut albums = self.with_details(&a.cfg.id, s, listed, shelf).await;
        albums.sort_by_key(|a: &source::Album| std::cmp::Reverse(a.added.unwrap_or(0)));
        ShelfRead { supported, albums, error }
    }

    /// Each album with its track count, format, lowest bitrate and length: from the catalogue where Cratarr sees the
    /// album's folder (no request), else as kept from before while the source has not saved the album since, else asked
    /// of the source, one album at a time. Asking is costly for Jellyfin (all 246 albums at once after a restart took it to
    /// 280% CPU and made it time out, 2026-10-13), so what was asked is kept, also across restarts.
    async fn with_details(&self, source_id: &str, s: &dyn Source, listed: Vec<source::Listed>, shelf: Shelf) -> Vec<source::Album> {
        let c = self.lib.catalogue();
        // The Music folder comes first when one is set (library::catalogue_folders).
        let music = i64::from(self.lib.music().is_some());
        // By album directory (relative to its folder); a server's path is matched by its longest such ending.
        let seen: HashMap<String, source::AlbumDetails> = tokio::task::spawn_blocking(move || {
            let all = c.folders().len() as i64;
            let folders = match shelf {
                Shelf::Music => 0..music.min(all),
                Shelf::Incoming => music..all,
            };
            let mut out = HashMap::new();
            for folder in folders {
                for f in c.album_facts(folder).unwrap_or_default() {
                    if f.dir.is_empty() {
                        continue;
                    }
                    let format = (f.formats.len() == 1).then(|| f.formats.iter().next().cloned()).flatten();
                    let duration_secs = f.duration_ms.map(|ms| ms as f64 / 1000.0);
                    out.insert(f.dir, source::AlbumDetails { tracks: f.tracks, format, bitrate_kbps: f.min_kbps, duration_secs });
                }
            }
            out
        })
        .await
        .unwrap_or_default();
        let from_catalogue = |path: &str| path.match_indices('/').find_map(|(i, _)| seen.get(&path[i + 1..])).cloned();
        let key = |id: &str| format!("{source_id}\n{id}");
        let mut albums = Vec::with_capacity(listed.len());
        let mut asked = false;
        for source::Listed { mut album, stamp } in listed {
            let kept = self.details.lock().unwrap_or_else(|p| p.into_inner()).get(&key(&album.id)).filter(|(s, _)| *s == stamp).map(|(_, d)| d.clone());
            let details = match album.path.as_deref().and_then(from_catalogue).or(kept) {
                Some(d) => Some(d),
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
