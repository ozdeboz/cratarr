//! What records' functions read of the app (App's wiring, the app split's phase 1): the services they use, as the app holds
//! them. The app implements it (`app_records`); a function takes `app: &Arc<H>` with `H: RecordsHost`, so its callers pass the
//! app as before, and records never names `App`. Discover's Lidarr pair is read through `&H`, its callers holding the app as
//! `&App` or as `&Arc<App>`: an `Arc` of a host is one too.

use std::sync::Arc;

pub trait RecordsHost: Send + Sync + 'static {
    /// The servers in use, the folders behind their libraries, the catalogue (`App::library`).
    fn library(&self) -> &Arc<super::library::Library>;
    /// Cratarr's own library, when open (`App::served`).
    fn served(&self) -> &Option<Arc<crate::serve::Serve>>;
    /// The albums of the Music library (`App::collection`).
    fn collection(&self) -> &Arc<super::CollectionService>;
    /// The artists' pages (`App::artists`).
    fn artists(&self) -> &Option<Arc<super::artists::ArtistInfo>>;
    /// Lidarr's connection as saved (`IncomingService::lidarr_state`).
    fn lidarr_config(&self) -> Option<crate::settings::LidarrConfig>;
    /// Lidarr's client, where it is connected and switched on (`IncomingService::lidarr_state`).
    fn lidarr(&self) -> Option<crate::lidarr::Lidarr>;
}

impl<T: RecordsHost + ?Sized> RecordsHost for Arc<T> {
    fn library(&self) -> &Arc<super::library::Library> {
        T::library(self)
    }

    fn served(&self) -> &Option<Arc<crate::serve::Serve>> {
        T::served(self)
    }

    fn collection(&self) -> &Arc<super::CollectionService> {
        T::collection(self)
    }

    fn artists(&self) -> &Option<Arc<super::artists::ArtistInfo>> {
        T::artists(self)
    }

    fn lidarr_config(&self) -> Option<crate::settings::LidarrConfig> {
        T::lidarr_config(self)
    }

    fn lidarr(&self) -> Option<crate::lidarr::Lidarr> {
        T::lidarr(self)
    }
}
