//! Cratarr's app: playing, crates and playlists, discovery, requests, intake, the served library's routes (split from the server crate: SPLIT-PLAN.md).

pub use cratarr_base::{acoustid, albums, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, search, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind, export, fixtures_base, folders, opus_copy_base, source, stream_forms_base, voice};

pub mod analysis;
pub mod app;
pub mod fixtures;
pub mod opus_copies;
pub mod opus_copy;
pub mod serve;
pub mod stream;
pub mod stream_forms;

#[cfg(test)]
mod moved_tests;
