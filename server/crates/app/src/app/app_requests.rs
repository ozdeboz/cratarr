//! The app as the requests' functions read it (`requests_host::RequestsHost`), and the house's copy as a Want's hands take it
//! (`requests::House`).

use std::sync::Arc;

use anyhow::Result;

use super::App;
use super::intake_card::Waiting;
use super::requests_host::{Asked, RequestsHost};

impl RequestsHost for App {
    fn serve(&self) -> &Option<Arc<crate::serve::Serve>> {
        &self.served
    }

    fn main_folders(&self) -> Vec<i64> {
        self.library.main_folders()
    }

    fn library_name(&self, source_id: &str, folder: i64) -> String {
        self.incoming.library_name(source_id, folder)
    }

    fn lidarr(&self) -> Option<crate::lidarr::Lidarr> {
        super::discover::lidarr(self)
    }

    fn lidarr_add_with(&self) -> crate::lidarr::AddWith {
        super::discover::add_with(self)
    }

    fn house(&self) -> &dyn super::requests::House {
        self
    }

    fn made(self: Arc<Self>, work: Box<dyn FnOnce() -> Result<super::file_runs_base::Proposal> + Send>, also: Option<crate::intake::Also>) -> Asked<'static, Result<super::file_runs_base::Status>> {
        Box::pin(async move { super::file_runs::made_with(&self, work, also).await })
    }

    fn request_marks<'a>(&'a self, serve: &'a crate::serve::Serve) -> Asked<'a, Result<Arc<super::requests::Marks>>> {
        Box::pin(super::requests::marks(self, serve))
    }

    fn intake_waiting(&self) -> Asked<'_, Vec<super::intake_card::Waiting>> {
        Box::pin(waiting(self))
    }
}

impl super::requests::House for App {
    fn copy_into_own<'a>(&'a self, account: &'a str, record: i64, sharing: bool) -> std::pin::Pin<Box<dyn std::future::Future<Output = super::albums_base::WantCopy> + Send + 'a>> {
        Box::pin(self.copy_for(account, record, sharing))
    }
}

/// The records waiting in an Incoming library of Cratarr's own, as `intake_card::waiting` gives them: read from Incoming's list, so
/// here, above it.
async fn waiting(app: &super::App) -> Vec<Waiting> {
    let view = app.incoming.list(None).await;
    let mut out = Vec::new();
    let mut places = Vec::new();
    for s in view.sources.iter().filter(|s| s.id == crate::source::cratarr::KIND.id) {
        if let Some(e) = &s.error {
            tracing::warn!(target: "library", reason = %e, "records waiting not listed");
        }
        for a in &s.albums {
            places.push((a.id.clone(), a.copies.first().map(|c| (c.folder, c.dir.clone()))));
            out.push(Waiting { source: s.id.clone(), album: a.id.clone(), title: a.title.clone(), artist: a.artist.clone().unwrap_or_default(), library: a.library.clone(), verdict: None });
        }
    }
    // Each one's stand against a copy kept elsewhere (C37a; RF2: another record's, paired track for track): an upgrade where Replace
    // would take it, else in two libraries.
    if let Some(served) = app.served.clone() {
        use crate::serve::copies::replaces;
        let mains = app.library.main_folders();
        let stands = crate::run::spawn_blocking(move || {
            let found: Vec<Option<(String, crate::serve::copies::Copy)>> = places
                .iter()
                .map(|(id, at)| {
                    let c = served.library.copies_of(id).ok()?.into_iter().find(|c| at.as_ref().is_some_and(|(f, d)| c.folder == *f && &c.dir == d))?;
                    Some((id.clone(), c))
                })
                .collect();
            let asked: Vec<(String, crate::serve::copies::Copy)> = found.iter().flatten().cloned().collect();
            let mut stood = served.stands_of(&asked, &mains).unwrap_or_else(|e| {
                tracing::warn!(target: "library", reason = %format!("{e:#}"), "how records waiting stand not read");
                vec![None; asked.len()]
            });
            stood.reverse();
            found
                .iter()
                .map(|f| f.as_ref().and_then(|_| stood.pop().flatten()).map(|(better, kept)| if replaces(better, kept.kind) { "upgrade" } else { "duplicate" }))
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        for (w, v) in out.iter_mut().zip(stands) {
            w.verdict = v;
        }
    }
    out
}
