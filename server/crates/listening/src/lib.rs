//! Listening: plays, scrobbles, history imports and resume.
//! Depends on base and the crates below it; nothing above it (split from the app crate: SP2-MOVE-NIGHT.md).

pub use cratarr_base::{acoustid, albums, analysis, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, intake, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, runner, search, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind, export, fixtures_base, folders, opus_copy_base, source, stream_forms_base, voice};

pub mod app;
pub mod serve;
