//! What Health's checks read of the app (App's wiring, the app split's phases 1 and 2): the services they use, as the app
//! holds them, and what they ask of functions that still take the app (the Library's lists; the requests' marks and the
//! intake card's waiting records through the requests' host, its supertrait). The app implements it (`app_health`); Health's
//! checks take it as `&dyn HealthHost` (`health::Checks`), and Health never names `App`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// A future a host hands back, as Health awaits it (inside the HTTP handlers: `Send`).
pub type Asked<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait HealthHost: super::requests_host::RequestsHost + Send + Sync + 'static {
    /// Cratarr's own library, when open (`App::served`).
    fn served(&self) -> &Option<Arc<crate::serve::Serve>>;
    /// The servers in use, the folders behind their libraries, the catalogue (`App::library`).
    fn library(&self) -> &Arc<super::library::Library>;
    /// Playlist files, crates and what each account keeps of them (`App::playlists`).
    fn playlists(&self) -> &Arc<super::playlists::PlaylistService>;
    /// New music, verdicts, Lidarr (`App::incoming`).
    fn incoming(&self) -> &Arc<super::incoming::IncomingService>;
    /// Cratarr's players (`App::play_on`).
    fn play_on(&self) -> &Arc<super::playon::PlayOn>;
    /// The Settings page (`App::settings`).
    fn settings(&self) -> &super::settings::SettingsService;
    /// The Library's one list (`App::albums`).
    fn albums<'a>(&'a self, want: Option<&'a str>) -> Asked<'a, super::AlbumsView>;
}
