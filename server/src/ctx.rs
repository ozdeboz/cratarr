//! What a request reaches (the audit, 0.10): the app, Cratarr's own library, the players and who asks, handed to each
//! handler instead of read from process-wide statics. What a handler uses is in its signature, and a test hands it its
//! own.

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::app::{App, ToggleError};
use crate::serve::Serve;
use crate::serve::who::{self, Who};

/// A handler's way to the app (`api::router`'s state), Cratarr's own library, sync play and who asks.
#[derive(Clone)]
pub struct Ctx {
    pub app: Arc<App>,
    /// Cratarr's own library (decision 0030): `main` always opens it; none only where a test builds the app without it.
    pub served: Option<Arc<Serve>>,
    /// Who the request is for (`access::require`); none on an open route.
    pub who: Option<Who>,
}

impl Ctx {
    /// The context of the running request (or of none: no one asks).
    pub fn of(app: &Arc<App>) -> Self {
        Self { app: app.clone(), served: app.served.clone(), who: who::current() }
    }

    /// Cratarr's own library, or the refusal every handler gives without it.
    pub fn serve(&self) -> Result<Arc<Serve>, ToggleError> {
        self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))
    }

    /// The account the request acts for (`Serve::user_for`): none without Cratarr's own library.
    pub fn account(&self) -> Option<String> {
        self.served.as_ref().and_then(|s| s.user_for(self.who.as_ref()))
    }

    /// The sync play server, once it listens (`main`).
    pub fn snapcast(&self) -> Option<Arc<crate::snapcast::Snapcast>> {
        self.app.snapcast.get().cloned()
    }
}

impl FromRequestParts<Arc<App>> for Ctx {
    type Rejection = Infallible;

    async fn from_request_parts(_parts: &mut Parts, app: &Arc<App>) -> Result<Self, Self::Rejection> {
        Ok(Self::of(app))
    }
}

/// When a change was made on the device (0.12, decision 0087): the `X-Cratarr-At` header (epoch milliseconds) an outbox
/// sends with every write it sends late, read here once for every handler. `None` without it, or unreadable: made now.
/// A time ahead of the server's clock is taken as now, so a device whose clock runs ahead cannot outlast later changes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChangeAt(pub Option<i64>);

impl ChangeAt {
    pub const HEADER: &'static str = "x-cratarr-at";

    /// As the header says it, against `now` (epoch ms).
    pub fn read(value: Option<&str>, now: i64) -> Self {
        Self(value.and_then(|v| v.trim().parse::<i64>().ok()).filter(|&ms| ms > 0).map(|ms| ms.min(now)))
    }

    /// Whether the header was sent: the answer then says whether the change was made (`applied`).
    pub fn sent(self) -> bool {
        self.0.is_some()
    }

    /// The change's time, or now (epoch ms).
    pub fn or_now(self) -> i64 {
        self.0.unwrap_or_else(now_ms)
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl<S: Send + Sync> FromRequestParts<S> for ChangeAt {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(Self::read(parts.headers.get(Self::HEADER).and_then(|v| v.to_str().ok()), now_ms()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changes_time_is_read_from_its_header_and_never_ahead_of_now() {
        assert_eq!(ChangeAt::read(Some("1792000000000"), 1_800_000_000_000), ChangeAt(Some(1_792_000_000_000)));
        assert_eq!(ChangeAt::read(Some(" 5 "), 10), ChangeAt(Some(5)));
        assert_eq!(ChangeAt::read(Some("1900000000000"), 1_800_000_000_000), ChangeAt(Some(1_800_000_000_000)), "ahead: now");
        assert_eq!(ChangeAt::read(Some("soon"), 10), ChangeAt(None));
        assert_eq!(ChangeAt::read(Some("-3"), 10), ChangeAt(None));
        assert_eq!(ChangeAt::read(None, 10), ChangeAt(None));
        assert!(!ChangeAt(None).sent() && ChangeAt(Some(1)).sent());
    }
}
