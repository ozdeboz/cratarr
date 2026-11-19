//! Cratarr's API: the page's routes, the MCP server, listening along and the guest portal (split from the server crate: SPLIT-PLAN.md).

pub use cratarr_app::{acoustid, albums, analysis, app, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, export, fixtures, folders, incoming, insights, jellyfin, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, opus_copies, opus_copy, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, search, settings, similarity, slskd, snapcast, source, stream, stream_forms, sync_state, trash, userdata, voice, web, wind};

pub mod api;
pub mod ctx;
pub mod idempotent;
// Cratarr as an MCP server (0.13).
pub mod mcp;
pub mod serve;

#[cfg(test)]
mod moved_tests;
