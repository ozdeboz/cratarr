//! What playing's functions read of the app (App's wiring, the app split's phase 1): the services they use, as the app holds
//! them. The app implements it (`app_playing`); a function takes `app: &Arc<H>` with `H: PlayingHost`, so its callers pass the
//! app as before, and playing never names `App`.

use std::sync::Arc;

pub trait PlayingHost: Send + Sync + 'static {
    /// The servers in use, the folders behind their libraries, the catalogue (`App::library`).
    fn library(&self) -> &Arc<super::library::Library>;
    /// Playlist files, crates and what each account keeps of them (`App::playlists`).
    fn playlists(&self) -> &Arc<super::playlists::PlaylistService>;
}
