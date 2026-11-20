//! The one decode turn, below Incoming and requests (moved from `app::analysis_pass`, which keeps its name): the pass, a page's
//! visuals, a play's waveform, the look ahead and Soulseek's gate each decode a file on it.

/// The one decode at a time (`app::analysis_pass`'s top). A turn moves into the blocking work and ends with it.
pub static HEAVY: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
