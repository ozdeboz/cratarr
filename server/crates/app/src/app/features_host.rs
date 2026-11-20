//! What search's and Health's functions read of the app (App's wiring, the app split's phase 1): the services they use, as the
//! app holds them, and what they ask of functions that still take the app (the requests' marks, the intake card's waiting
//! records, the Library's lists). The app implements it (`app_features`); a function takes `app: &Arc<H>` with
//! `H: FeaturesHost`, so its callers pass the app as before, and features never names `App`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;

/// A future a host hands back, as features await it (inside the HTTP handlers: `Send`).
pub type Asked<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait FeaturesHost: Send + Sync + 'static {
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
    /// The requests' marks (`requests::marks`).
    fn request_marks<'a>(&'a self, serve: &'a crate::serve::Serve) -> Asked<'a, Result<Arc<super::requests::Marks>>>;
    /// The records waiting in Incoming with their intake cards (`intake_card::waiting`).
    fn intake_waiting(&self) -> Asked<'_, Vec<super::intake_card::Waiting>>;
}
