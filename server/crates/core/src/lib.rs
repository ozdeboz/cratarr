//! Cratarr's base: the database, settings files, names, search, diagnostics, who asks (split from the server crate: SPLIT-PLAN.md).

pub mod app;
pub mod config;
pub mod db;
pub mod diagnostics;
pub mod m3u;
pub mod memwatch;
pub mod mounts;
pub mod naming;
#[allow(dead_code)] // the playlist sync uses it (decision 0012)
pub mod playlist_merge;
pub mod playlist_ops;
pub mod search;
pub mod serve;
pub mod similarity;
pub mod web;
