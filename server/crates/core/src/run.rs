//! One spawn for every task (the owner's decision 1, 2026-11-18; lane 3's REPORT: about a quarter of the crate's mono items
//! were tokio's task machinery, built once per future type and call site, and twice in tests). The future or closure is
//! boxed here, so that machinery is built once per output type.

use std::future::Future;
use std::pin::Pin;

use tokio::task::JoinHandle;

/// `tokio::spawn`, its future boxed.
#[track_caller]
pub fn spawn<T: Send + 'static>(fut: impl Future<Output = T> + Send + 'static) -> JoinHandle<T> {
    tokio::spawn(Box::pin(fut) as Pin<Box<dyn Future<Output = T> + Send>>)
}

/// `tokio::task::spawn_blocking`, its closure boxed.
#[track_caller]
pub fn spawn_blocking<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> JoinHandle<R> {
    tokio::task::spawn_blocking(Box::new(f) as Box<dyn FnOnce() -> R + Send>)
}
