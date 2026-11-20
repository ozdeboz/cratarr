//! What playing's functions read of the app (App's wiring, the app split's phase 1): the services they use, as the app holds
//! them. The app implements it (`app_playing`); a function takes `app: &Arc<H>` with `H: PlayingHost`, so its callers pass the
//! app as before, and playing never names `App`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// A future a host hands back, as playing awaits it (inside the HTTP handlers: `Send`).
pub type Asked<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait PlayingHost: Send + Sync + 'static {
    /// The servers in use, the folders behind their libraries, the catalogue (`App::library`).
    fn library(&self) -> &Arc<super::library::Library>;
    /// Playlist files, crates and what each account keeps of them (`App::playlists`).
    fn playlists(&self) -> &Arc<super::playlists::PlaylistService>;
    /// What plays on the servers' players, the album and artist views (`App::playing`).
    fn playing(&self) -> &super::playing::PlayingService;
    /// Cratarr's players (`App::play_on`).
    fn play_on(&self) -> &Arc<super::playon::PlayOn>;
    /// The visuals each sync group shares (`App::visuals`).
    fn visuals(&self) -> &Arc<super::visuals::GroupVisuals>;
    /// Cratarr's own library, when open (`App::served`).
    fn served(&self) -> &Option<Arc<crate::serve::Serve>>;
    /// Where an album that left Incoming is now: its id in its new library; none where it did not move there
    /// (`IncomingService::album_fate`).
    fn album_moved_to<'a>(&'a self, source: &'a str, id: &'a str) -> Asked<'a, Option<String>>;
}

/// Cratarr's own records said with their files' state (`file_facts::tell`, Incoming's), as the artist view lists them: a slot
/// on `PlayingService` the app sets as it is made (`PlayingService::set_file_facts`), never a static.
pub trait FileFacts: Send + Sync {
    fn tell(&self, source: &str, albums: &mut [crate::source::Album]);
}
