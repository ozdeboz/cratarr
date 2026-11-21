//! What incoming's functions read of the app (App's wiring, the app split's phase 1): the services they use, as the app
//! holds them, and what they ask of functions that still take the app (the requests' fetcher, a fetched track's add). The
//! app implements it (`app_incoming`); a function takes `app: &Arc<H>` with `H: IncomingHost`, so its callers pass the app
//! as before, and incoming never names `App`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;

/// A future a host hands back, as incoming awaits it (inside the HTTP handlers and the workers: `Send`).
pub type Asked<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait IncomingHost: Send + Sync + 'static {
    /// The servers in use, the folders behind their libraries, the catalogue (`App::library`).
    fn library(&self) -> &Arc<super::library::Library>;
    /// The albums of the Music library (`App::collection`).
    fn collection(&self) -> &Arc<super::collection::CollectionService>;
    /// New music, verdicts, Delete and Undo, Lidarr (`App::incoming`).
    fn incoming(&self) -> &Arc<super::incoming::IncomingService>;
    /// What happened to albums, the runs on the one path among it (`App::journal`).
    fn journal(&self) -> &Arc<crate::journal::Journal>;
    /// Cratarr's own library, when open (`App::served`).
    fn served(&self) -> &Option<Arc<crate::serve::Serve>>;
    /// The fetcher approved requests are handed to (`requests::fetcher`).
    fn request_fetcher(&self) -> Option<Box<dyn crate::ports::Fetcher>>;
    /// A passed fetch added to its album, a run on the one path (`track_fetch::add`); `also` written in its run's own transaction,
    /// given its Undo (RN's R2: the bulk item's Added with its run).
    fn add_fetched(self: Arc<Self>, fetch: String, also: Option<crate::intake::Also>) -> Asked<'static, Result<super::file_runs::Status>>;
}
