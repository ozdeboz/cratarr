//! Playlists: the playlist files, edits and repairs, Play's playlist, the account's own and the shared ones, and the
//! service the crates are worked out over (their stores, the favourites heard), with the journal and the Undo of every
//! change, the playlists' and the crates' alike. Depends on records and the crates below it; not on crates, playing,
//! requests nor anything above it (decision 0106's amendment: one crate per domain; SP3-SCAN.md).

pub use cratarr_records::{acoustid, albums, analysis, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, search, serve, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind, export, fixtures_base, folders, opus_copy_base, source, stream_forms_base, voice};

pub mod app;
