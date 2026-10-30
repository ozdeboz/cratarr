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
