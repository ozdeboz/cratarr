//! Cratarr's app: playing, crates and playlists, discovery, requests, intake, the served library's routes (split from the server crate: SPLIT-PLAN.md).

pub use cratarr_gate::{acoustid, albums, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, search, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind};

pub mod analysis;
pub mod app;
pub mod export;
pub mod fixtures;
pub mod folders;
pub mod opus_copies;
pub mod opus_copy;
pub mod opus_copy_base;
pub mod serve;
pub mod source;
pub mod stream;
pub mod stream_forms;
pub mod stream_forms_base;
pub mod voice;

#[cfg(test)]
mod moved_tests;
