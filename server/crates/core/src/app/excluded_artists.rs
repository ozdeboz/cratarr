//! Excluding an artist everywhere (decision 0083) lives in the rules crate (`cratarr_rules::excluded_artists`): the
//! crates read it, and a player that plays on its own leaves out the same artists. Named here too, so the server says
//! `app::excluded_artists` as it always did.

pub use cratarr_rules::excluded_artists::*;
