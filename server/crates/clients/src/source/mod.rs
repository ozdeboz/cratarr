//! Music sources: two plugins here, Jellyfin and Subsonic; the types and the `Source` trait below (SPLIT-PLAN 5.2).

pub use cratarr_store::source::*;

pub mod jellyfin;
pub mod subsonic;
