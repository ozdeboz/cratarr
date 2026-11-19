//! The server's routes: listening along and the guest portal here; the rest below, in cratarr-app (SPLIT-PLAN 5.2).

pub use cratarr_app::serve::*;

pub mod along;
pub mod guest;
