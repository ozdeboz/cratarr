//! Cratarr's domain rules, pure (DOMAIN-CORE §14): who may do what with a record, its copies and its arrivals, the arrival's
//! machine, and the places a record is seen from. No I/O, no clock, no async: it reads only the facts it is handed, and
//! depends on nothing but the rules crate and serde.

pub mod arrival;
pub mod can;
pub mod when;

/// The holding, the one had rule, and the pairing it reads: in the rules crate, since the device runs them too (§14, X28).
pub use cratarr_rules::{assign, holding};
