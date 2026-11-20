//! Cratarr's base: the app's shared services (library, settings, sync, shelf, collection, follow, tasks, words) and what
//! the areas above it share (split from the app crate: SP2-MOVE-NIGHT.md).

pub use cratarr_served::{acoustid, albums, analysis, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, search, serve, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind};

pub mod app;
pub mod export;
pub mod fixtures_base;
pub mod folders;
pub mod opus_copy_base;
pub mod source;
pub mod stream_forms_base;
pub mod voice;
