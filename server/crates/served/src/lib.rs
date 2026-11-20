//! The served library: Serve, users, sessions, access, sign-in, LDAP, playlists and files, below the app's areas.
//! Depends on gate and the crates below it; nothing above it (split from the app crate: SP2-MOVE-NIGHT.md).

pub use cratarr_gate::{acoustid, albums, analysis, app, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, search, settings, similarity, slskd, snapcast, source, sync_state, trash, userdata, web, wind};

pub mod serve;
