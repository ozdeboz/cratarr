//! What the requests' functions read of the app (App's wiring, the app split's phase 1): what they use of the services it holds,
//! and what they ask of functions that still take the app (Lidarr as Discover reads it, the house's copy, a fetched track's run, the
//! requests' marks, the records waiting in Incoming). The app implements it (`app_requests`); a function takes `app: &Arc<H>` with `H: RequestsHost`, so
//! its callers pass the app as before, and requests never names `App`. Health's and search's hosts take it as a supertrait, so none
//! of its methods is named as one of theirs.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;

/// A future a host hands back, as requests await it (inside the HTTP handlers and the workers: `Send`).
pub type Asked<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait RequestsHost: Send + Sync + 'static {
    /// Cratarr's own library, when open (`App::served`).
    fn serve(&self) -> &Option<Arc<crate::serve::Serve>>;
    /// The catalogue folders of the main libraries (`Library::main_folders`).
    fn main_folders(&self) -> Vec<i64>;
    /// A catalogue folder's library, as Incoming names it (`IncomingService::library_name`).
    fn library_name(&self, source_id: &str, folder: i64) -> String;
    /// Lidarr's client, where it is connected and switched on (`discover::lidarr`).
    fn lidarr(&self) -> Option<crate::lidarr::Lidarr>;
    /// What Lidarr adds an album with: the profiles and root folder chosen on its page in Connections (`discover::add_with`).
    fn lidarr_add_with(&self) -> crate::lidarr::AddWith;
    /// The house's copy of a record into an account's own music, as a Want's hands take it (`App::copy_for`).
    fn house(&self) -> &dyn super::requests::House;
    /// A passed fetch's proposal made a run on the one path (`file_runs::made`).
    fn made(self: Arc<Self>, work: Box<dyn FnOnce() -> Result<super::file_runs_base::Proposal> + Send>) -> Asked<'static, Result<super::file_runs_base::Status>>;
    /// The requests' marks (`requests::marks`).
    fn request_marks<'a>(&'a self, serve: &'a crate::serve::Serve) -> Asked<'a, Result<Arc<super::requests::Marks>>>;
    /// The records waiting in Incoming with their intake cards (`intake_card::waiting`).
    fn intake_waiting(&self) -> Asked<'_, Vec<super::intake_card::Waiting>>;
}
