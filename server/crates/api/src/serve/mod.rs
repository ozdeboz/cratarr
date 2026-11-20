//! The server's routes: serve's HTTP files (SP2), listening along and the guest portal here; the rest below, in cratarr-app
//! and cratarr-served (SPLIT-PLAN 5.2).

pub use cratarr_app::serve::*;

pub mod accounts;
pub mod along;
pub mod auth_check;
pub mod guest;
pub mod import;
pub mod library_api;
pub mod queues;
pub mod review;
pub mod routes;
#[cfg(test)]
mod served_tests_api;
