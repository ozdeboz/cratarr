//! The app as the requests' functions read it (`requests_host::RequestsHost`), and the house's copy as a Want's hands take it
//! (`requests::House`).

use std::sync::Arc;

use anyhow::Result;

use super::App;
use super::requests_host::{Asked, RequestsHost};

impl RequestsHost for App {
    fn library_name(&self, source_id: &str, folder: i64) -> String {
        self.incoming.library_name(source_id, folder)
    }

    fn lidarr(&self) -> Option<crate::lidarr::Lidarr> {
        super::discover::lidarr(self)
    }

    fn lidarr_add_with(&self) -> crate::lidarr::AddWith {
        super::discover::add_with(self)
    }

    // House is crate-wide while what it comes to (albums::WantCopy) is (RequestsHost::house).
    #[allow(private_interfaces)]
    fn house(&self) -> &dyn super::requests::House {
        self
    }

    fn request_marks<'a>(&'a self, serve: &'a crate::serve::Serve) -> Asked<'a, Result<Arc<super::requests::Marks>>> {
        Box::pin(super::requests::marks(self, serve))
    }

    fn intake_waiting(&self) -> Asked<'_, Vec<super::intake_card::Waiting>> {
        Box::pin(super::intake_card::waiting(self))
    }
}

impl super::requests::House for App {
    fn copy_into_own<'a>(&'a self, account: &'a str, record: i64, sharing: bool) -> std::pin::Pin<Box<dyn std::future::Future<Output = super::albums::WantCopy> + Send + 'a>> {
        Box::pin(self.copy_for(account, record, sharing))
    }
}
