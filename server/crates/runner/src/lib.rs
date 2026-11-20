//! Cratarr's job runner (RN; job-runner/due-diligence.md §4: "our own small runner on the journal's SQLite, shaped by the
//! references; borrow parts, not a framework"). One process, so an in-memory list chooses and the journal's file only makes it
//! durable: no leases, no heartbeats.
//!
//! - **The states** (`state`): waiting, scheduled, running, retrying, done, failed, cancelled (DOMAIN-CORE §4.7), walked by one
//!   pure function.
//! - **The words** (`words`): C.8's table, every kind's state and its words, written as `shared/states.json` for the page.
//! - **The jobs** (`jobs`): what tasks.rs did (a person's long work on their pages, one of a kind per account, kept an hour after
//!   it ends) and the work Cratarr does on its own, each a row in the journal's file (`jobs`), its progress in memory. A restart
//!   cuts off what ran: a kind with checkpoints goes on from its last, the others end "It stopped before it was done.".
//! - **The resources** (`resource`): one decode at a time, three covers, memory; someone waiting goes first, and a waiter past its
//!   bound gets the next turn.
//! - **Activity** (`Runner::rows`): every job that runs, what it does now, how far, and whose; the live channel follows it.

pub mod jobs;
pub mod kinds;
pub mod resource;
pub mod state;
pub mod words;

pub use jobs::{Handle, Row, Runner, Step, Stepper, Task, Wait, runner, spawn};
pub use resource::{COVERS, DECODE, Priority, Resource};
pub use state::State;
