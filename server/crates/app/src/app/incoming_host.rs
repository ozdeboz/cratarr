//! What incoming's functions read of the app (App's wiring, the app split's phase 1): the services they use, as the app
//! holds them. The app implements it (`app_incoming`); a function takes `app: &Arc<H>` with `H: IncomingHost`, so its
//! callers pass the app as before, and incoming never names `App`.

use std::sync::Arc;

pub trait IncomingHost: Send + Sync + 'static {
    /// The servers in use, the folders behind their libraries, the catalogue (`App::library`).
    fn library(&self) -> &Arc<super::library::Library>;
    /// New music, verdicts, Delete and Undo, Lidarr (`App::incoming`).
    fn incoming(&self) -> &Arc<super::incoming::IncomingService>;
    /// What happened to albums, the runs on the one path among it (`App::journal`).
    fn journal(&self) -> &Arc<crate::journal::Journal>;
    /// Cratarr's own library, when open (`App::served`).
    fn served(&self) -> &Option<Arc<crate::serve::Serve>>;
}
