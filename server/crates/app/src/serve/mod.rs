//! The served library's parts in this crate: the rest of it below, in cratarr-served (SP2-MOVE-NIGHT.md).

pub use cratarr_served::serve::*;

pub mod accounts;
pub mod auth_check;
pub mod import;
pub mod import_base;
pub mod library_api;
pub mod queues;
pub mod review;
pub mod routes;
#[cfg(test)]
mod served_tests_api;
