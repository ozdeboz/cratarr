//! What search reads of the app (App's wiring, the app split's phases 1 and 2): the services it uses, as the app holds them,
//! and the requests' marks through the requests' host, its supertrait. The app implements it (`app_search`); a function takes
//! `app: &Arc<H>` with `H: SearchHost`, so its callers pass the app as before, and search never names `App`.

use std::sync::Arc;

pub trait SearchHost: super::requests_host::RequestsHost + Send + Sync + 'static {
    /// Cratarr's own library, when open (`App::served`).
    fn served(&self) -> &Option<Arc<crate::serve::Serve>>;
    /// The servers in use, the folders behind their libraries, the catalogue (`App::library`).
    fn library(&self) -> &Arc<super::library::Library>;
    /// Playlist files, crates and what each account keeps of them (`App::playlists`).
    fn playlists(&self) -> &Arc<super::playlists::PlaylistService>;
}
