//! Incoming: new music and its runs, the album actions, fixes, covers, the analysis pass and the bulk queue.
//! Depends on requests and playing and the crates below them; nothing above it (split from the app crate:
//! SP2-MOVE-NIGHT.md).

pub use cratarr_requests::{acoustid, albums, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, intake, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, runner, search, serve, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind, export, fixtures_base, folders, opus_copy_base, source, stream_forms_base, voice, fixtures};
pub use cratarr_playing::{opus_copies, opus_copy, stream, stream_forms};

pub mod analysis;
pub mod app;
