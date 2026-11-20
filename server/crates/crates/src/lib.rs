//! Crates: the built-in ones and one's own, Sounds like, the fits and the taste that rank them, Home's crates and what is
//! excluded everywhere, worked out over the playlist service. Depends on playlists and the crates below it; not on
//! playing, requests nor anything above it (decision 0106's amendment: one crate per domain; SP3-SCAN.md).

pub use cratarr_playlists::{acoustid, albums, analysis, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, intake, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, runner, search, serve, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind, export, fixtures_base, folders, opus_copy_base, source, stream_forms_base, voice};

pub mod app;
