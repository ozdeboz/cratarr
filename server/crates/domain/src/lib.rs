//! Cratarr's domain rules, pure (DOMAIN-CORE §14): who may do what with a record, its copies and its arrivals, the arrival's
//! machine, the places a record is seen from and its sheet's when-table, the record's words, and the record as one object (its
//! entity, card, row and chip: OBJECT-MAP-2 §2.1), the request's and the Soulseek item's machines and their one wording table
//! (DOMAIN-CORE §4.1, §4.2, §4.5). No I/O, no clock, no async: it reads only the facts it is handed, and depends
//! on nothing but the rules crate and serde (and ts-rs, for the page's generated types, in tests and the `ts` feature alone).

pub mod arrival;
pub mod can;
pub mod change;
pub mod credits;
pub mod facts;
pub mod identity;
pub mod picture;
pub mod record;
pub mod request;
pub mod soulseek;
pub mod view;
pub mod when;
pub mod which_release;
pub mod wording;
pub mod words;

/// The holding, the one had rule, and the pairing it reads: in the rules crate, since the device runs them too (§14, X28).
pub use cratarr_rules::{assign, holding};
