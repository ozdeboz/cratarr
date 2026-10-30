//! Who a request is for (decision 0051, several users): the account signed in, whether it is an administrator, and how
//! far the way it came in may go. The access check (`access::require`) sets it around each request; what keeps a user's
//! own data reads it (`Serve::current_user`). A task the request hands work to (`spawn_blocking`, `spawn`) does not
//! carry it: read it first and pass the name along.

use std::future::Future;

/// How far a request may go: a sign-in goes everywhere its account may; an API key reads, or reads and controls the
/// players, as it was made to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Full,
    Control,
    Read,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Who {
    pub user: String,
    pub admin: bool,
    pub scope: Scope,
    /// Whether the request came from the home network (`serve::proxy::at_home`): another user's shared device is
    /// theirs to use from there only (`app::playon`).
    pub home: bool,
}

tokio::task_local! {
    static CURRENT: Who;
}

/// Runs `f` as `who`.
pub async fn acting_as<F: Future>(who: Who, f: F) -> F::Output {
    CURRENT.scope(who, f).await
}

/// Who the running request is for; `None` outside one (a task of its own, a background pass).
pub fn current() -> Option<Who> {
    CURRENT.try_with(|w| w.clone()).ok()
}

/// `f` as the running request's account, for a task of its own that works for the request (a live channel's).
pub fn carried<F: Future>(f: F) -> impl Future<Output = F::Output> {
    let who = current();
    async move {
        match who {
            Some(w) => acting_as(w, f).await,
            None => f.await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_request_is_for_its_account_and_a_task_of_its_own_for_no_one() {
        let ann = Who { user: "ann".into(), admin: false, scope: Scope::Full, home: true };
        assert_eq!(current(), None);
        let (inside, spawned, carried) = acting_as(ann.clone(), async {
            let spawned = tokio::spawn(async { current() }).await.unwrap();
            let carried = tokio::spawn(super::carried(async { current() })).await.unwrap();
            (current(), spawned, carried)
        })
        .await;
        assert_eq!(inside, Some(ann.clone()));
        assert_eq!(spawned, None, "a spawned task carries no one: read it first and pass it on");
        assert_eq!(carried, Some(ann), "unless it is carried");
    }
}
