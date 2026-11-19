//! Cratarr's file gate: writing tags, moving files, the trash (split from the server crate: SPLIT-PLAN.md).

pub use cratarr_clients::{acoustid, albums, analysis, app, bandcamp, bandcamp_ids, catalogue, config, convert, covers, crates, db, diagnostics, incoming, insights, jellyfin, journal, lidarr, m3u, memwatch, mounts, moved, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, rewind, search, serve, settings, similarity, slskd, snapcast, source, sync_state, userdata, web, wind};

pub mod covers_inside;
pub mod moves;
pub mod ports;
pub mod retag;
pub mod trash;

#[cfg(test)]
mod moved_tests;
