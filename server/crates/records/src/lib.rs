//! Records: discovery, references, labels, classical, Discogs, artists, works and their kin.
//! Depends on listening and the crates below it; nothing above it (split from the app crate: SP2-MOVE-NIGHT.md).

pub use cratarr_listening::{acoustid, albums, analysis, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, intake, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, runner, search, serve, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind, export, fixtures_base, folders, opus_copy_base, source, stream_forms_base, voice};

pub mod app;
