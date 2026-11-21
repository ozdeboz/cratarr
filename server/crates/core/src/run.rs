//! One spawn for every task (the owner's decision 1, 2026-11-18; lane 3's REPORT: about a quarter of the crate's mono items
//! were tokio's task machinery, built once per future type and call site, and twice in tests). The future or closure is
//! boxed here, so that machinery is built once per output type.
//!
//! Tests (SCENARIOS-LOOKUPS LK.0's harness 2, R-N2): a route's scope, carried into every task its work spawns, as `who::carried`
//! carries who asks; a blocking closure enters it with `sync_scope`, as `who::acting_as_blocking` does. A test-support build only.

use std::future::Future;
use std::pin::Pin;

use tokio::task::JoinHandle;

/// `tokio::spawn`, its future boxed; in a test, in the scope it was spawned from.
#[track_caller]
pub fn spawn<T: Send + 'static>(fut: impl Future<Output = T> + Send + 'static) -> JoinHandle<T> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(s) = scope::current() {
        let live = scope::Live::of(&s);
        return tokio::spawn(Box::pin(async move {
            let _live = live;
            scope::within(s, fut).await
        }) as Pin<Box<dyn Future<Output = T> + Send>>);
    }
    tokio::spawn(Box::pin(fut) as Pin<Box<dyn Future<Output = T> + Send>>)
}

/// `tokio::task::spawn_blocking`, its closure boxed; in a test, in the scope it was spawned from.
#[track_caller]
pub fn spawn_blocking<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> JoinHandle<R> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(s) = scope::current() {
        let live = scope::Live::of(&s);
        return tokio::task::spawn_blocking(Box::new(move || {
            let _live = live;
            scope::within_blocking(s, f)
        }) as Box<dyn FnOnce() -> R + Send>);
    }
    tokio::task::spawn_blocking(Box::new(f) as Box<dyn FnOnce() -> R + Send>)
}

/// Tests: a route's scope (LK.0's harness 2). Every outside ask counts against the scope it is made in, whenever it lands; a job's
/// work is marked so (LK.0b: an ask inside a runner job's work is the job's, any other is inline). Each scope counts its live tasks.
#[cfg(any(test, feature = "test-support"))]
pub mod scope {
    use std::future::Future;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A route's scope: its number, its tasks alive, and whether the work running is a runner job's.
    #[derive(Debug, Clone)]
    pub struct Scope {
        pub id: u64,
        pub live: Arc<AtomicUsize>,
        pub job: bool,
    }

    impl Scope {
        pub fn new(id: u64) -> Scope {
            Scope { id, live: Arc::new(AtomicUsize::new(0)), job: false }
        }

        /// Its tasks still alive.
        pub fn alive(&self) -> usize {
            self.live.load(Ordering::SeqCst)
        }
    }

    tokio::task_local! {
        static SCOPE: Scope;
    }

    /// The scope the running code is in, where it is in one.
    pub fn current() -> Option<Scope> {
        SCOPE.try_with(Clone::clone).ok()
    }

    /// `f` in scope `s`.
    pub async fn within<F: Future>(s: Scope, f: F) -> F::Output {
        SCOPE.scope(s, f).await
    }

    /// `f`, on a thread of its own, in scope `s` (a `block_on` inside it sees it too).
    pub fn within_blocking<R>(s: Scope, f: impl FnOnce() -> R) -> R {
        SCOPE.sync_scope(s, f)
    }

    /// A runner job's work: in its scope, marked a job's (the runner's `run`).
    pub async fn as_job<F: Future>(f: F) -> F::Output {
        match current() {
            Some(mut s) => {
                s.job = true;
                SCOPE.scope(s, f).await
            }
            None => f.await,
        }
    }

    /// One task alive in its scope, until dropped.
    pub struct Live(Arc<AtomicUsize>);

    impl Live {
        pub fn of(s: &Scope) -> Live {
            s.live.fetch_add(1, Ordering::SeqCst);
            Live(s.live.clone())
        }
    }

    impl Drop for Live {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
}
