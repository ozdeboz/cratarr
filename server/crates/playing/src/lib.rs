//! Playing: Play On, the queue, devices, radio, home, playlists, the stream and the opus copies, beside requests.
//! Depends on records and the crates below it; not on requests, nor anything above it (split from the app crate:
//! SP2-MOVE-NIGHT.md).

pub use cratarr_records::{acoustid, albums, analysis, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, search, serve, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind, export, fixtures_base, folders, opus_copy_base, source, stream_forms_base, voice};

pub mod app;
pub mod opus_copies;
pub mod opus_copy;
pub mod stream;
pub mod stream_forms;
