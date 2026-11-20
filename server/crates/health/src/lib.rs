//! Health: every domain's check in one report, and a record's own (decision 0063), beside search.
//! Depends on incoming and the crates below it; not on search, nor anything above it (split from the app crate:
//! SP2-MOVE-NIGHT.md).

pub use cratarr_incoming::{acoustid, albums, analysis, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, search, serve, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind, export, fixtures_base, folders, opus_copy_base, source, stream_forms_base, voice, opus_copies, opus_copy, stream, stream_forms, fixtures};

pub mod app;
