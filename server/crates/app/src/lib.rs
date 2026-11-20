//! Cratarr's app: playing, crates and playlists, discovery, requests, intake, the served library's routes (split from the server crate: SPLIT-PLAN.md).

pub use cratarr_requests::{acoustid, albums, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, search, serve, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind, export, fixtures_base, folders, opus_copy_base, source, stream_forms_base, voice};
pub use cratarr_playing::{opus_copies, opus_copy, stream, stream_forms};

pub mod analysis;
pub mod app;
pub mod fixtures;

#[cfg(test)]
mod moved_tests;
