//! `playlists`' telling of the sources, in base: the connected sources told of a playlist file changed, as base's follow
//! and sync tell them.

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
