//! The base: the library, settings, sync, shelf, collection, follow, tasks, words, and the views the areas above share.
//! Depends on served and the crates below it; nothing above it (split from the app crate: SP2-MOVE-NIGHT.md).

pub use cratarr_served::{acoustid, albums, analysis, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, intake, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, search, serve, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind};

/// The job runner (RN), below the base: its jobs, states, words and resources.
pub use cratarr_runner as runner;

pub mod app;
pub mod export;
pub mod fixtures_base;
pub mod folders;
pub mod opus_copy_base;
pub mod source;
pub mod stream_forms_base;
pub mod voice;
