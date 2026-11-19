//! The server's parts in this crate; the rest are above it (SPLIT-PLAN 5.2).

pub use cratarr_core::serve::{proxy, rate, who};
pub use cratarr_media::serve::art;

pub mod api;
pub mod copies;
pub mod ids;
pub mod index_rows;
pub mod library;
pub mod listener;
pub mod read;
pub mod view;
