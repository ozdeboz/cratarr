//! Cratarr's stores: the catalogue, the user data, the journal, crates and playlists, the served library (split from the server crate: SPLIT-PLAN.md).

pub use cratarr_core::{config, db, diagnostics, m3u, memwatch, mounts, naming, playlist_merge, playlist_ops, run, search, similarity, web};
pub use cratarr_media::{analysis, convert, covers, rewind, wind};

pub mod albums;
pub mod app;
pub mod bandcamp_ids;
pub mod catalogue;
pub mod crates;
pub mod incoming;
pub mod insights;
pub mod journal;
pub mod moved;
pub mod placement;
pub mod playlists;
pub mod serve;
pub mod settings;
pub mod source;
pub mod sync_state;
pub mod userdata;
