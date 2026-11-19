//! Cratarr's sound and pictures: analysis, conversion, covers, the tape's rewind and wind (split from the server crate: SPLIT-PLAN.md).

// Decision 0035's core; the pass that uses it comes next.
#[allow(dead_code)]
pub mod analysis;
pub mod convert;
pub mod covers;
pub mod rewind;
pub mod serve;
pub mod wind;
