//! `playlists`' telling of the sources, in base: the connected sources told of a playlist file changed, as base's follow
//! and sync tell them; and a name as the page compares names, as records' classical orders its works.

use cratarr_rules::names::{name_key, natural_keys};

use crate::source::{PlaylistChange, PlaylistEvent};

use super::library::Library;

/// Best effort, in the background: every connected source hears about changed playlist files, each with the file's
/// path as it sees it.
pub(super) fn tell_sources(lib: &Library, changes: Vec<(String, PlaylistChange)>) {
    let Some(music) = lib.music() else { return };
    for (a, s) in lib.connected() {
        let events: Vec<PlaylistEvent> = changes
            .iter()
            .map(|(name, change)| PlaylistEvent { name: name.clone(), path: a.placement.playlist_path(&music, name), change: *change })
            .collect();
        crate::run::spawn(async move {
            if let Err(e) = s.playlists_changed(events).await {
                tracing::warn!("source {}: could not pass on playlist changes: {e:#}", a.cfg.id);
            }
        });
    }
}

/// A name as the page compares names (`localeCompare` at base sensitivity, numeric): case and the common accents
/// aside (`name_key`), each run of digits as its number. Classical's works are ordered by it too (0.12).
pub fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    natural_keys(&name_key(a), &name_key(b))
}
