//! Cratarr's outside services: Jellyfin, Subsonic, Lidarr, slskd, Bandcamp, AcoustID, MusicBrainz, Snapcast (split from the server crate: SPLIT-PLAN.md).

pub use cratarr_store::{albums, analysis, bandcamp_ids, catalogue, config, convert, covers, crates, db, diagnostics, incoming, insights, journal, m3u, memwatch, mounts, moved, naming, placement, playlist_merge, playlist_ops, playlists, rewind, search, serve, settings, similarity, sync_state, userdata, web, wind};

pub mod acoustid;
pub mod app;
pub mod bandcamp;
pub mod jellyfin;
pub mod lidarr;
pub mod nowplaying;
pub mod slskd;
pub mod snapcast;
pub mod source;

#[cfg(test)]
mod moved_tests;
