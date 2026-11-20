//! The top: the App struct and its wiring of every area (App::new, the background pass, the hosts each area reads it
//! through), first start's runtime, the capabilities, the page aggregators and the fixtures. Depends on health and search
//! and everything below them; only the API is above it (split from the server crate: SPLIT-PLAN.md, SP2-MOVE-NIGHT.md).

pub use cratarr_health::{acoustid, albums, analysis, bandcamp, bandcamp_ids, catalogue, config, convert, covers, covers_inside, crates, db, diagnostics, incoming, insights, jellyfin, intake, journal, lidarr, m3u, memwatch, mounts, moved, moves, naming, nowplaying, placement, playlist_merge, playlist_ops, playlists, ports, retag, rewind, run, runner, search, serve, settings, similarity, slskd, snapcast, sync_state, trash, userdata, web, wind, export, fixtures_base, folders, opus_copy_base, source, stream_forms_base, voice, opus_copies, opus_copy, stream, stream_forms};

pub mod app;
pub mod fixtures;

#[cfg(test)]
mod moved_tests;
