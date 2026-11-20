//! The app as records' functions read it (`records_host::RecordsHost`).

use std::sync::Arc;

use super::App;
use super::records_host::RecordsHost;

impl RecordsHost for App {
    fn library(&self) -> &Arc<super::library::Library> {
        &self.library
    }

    fn served(&self) -> &Option<Arc<crate::serve::Serve>> {
        &self.served
    }

    fn collection(&self) -> &Arc<super::CollectionService> {
        &self.collection
    }

    fn artists(&self) -> &Option<Arc<super::artists::ArtistInfo>> {
        &self.artists
    }

    fn lidarr_config(&self) -> Option<crate::settings::LidarrConfig> {
        self.incoming.lidarr_state().0
    }

    fn lidarr(&self) -> Option<crate::lidarr::Lidarr> {
        self.incoming.lidarr_state().1
    }
}
