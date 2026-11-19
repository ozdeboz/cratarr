//! One import (decision 0067): the matcher below, in cratarr-store; listens into the history here (SPLIT-PLAN P6).

pub use cratarr_gate::app::import::*;

/// Listens into an account's history: the part that takes the server.
pub mod history;
pub use history::{Listened, into_history};
