//! Music sources (decision 0005): the plugins and their list here; the types, the `Source` trait and two plugins below, in
//! cratarr-store and cratarr-clients (SPLIT-PLAN 5.2).

pub use cratarr_served::source::*;

pub mod cratarr;
pub mod kinds;

pub use kinds::{KINDS, kind};
