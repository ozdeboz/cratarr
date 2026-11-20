//! `device_index`'s standing, in base: every catalogue folder as a device names it (`Library::index_libraries`), and where
//! the library's log stands for a device, which base's change log adds where a device asks. `device_index` keeps the
//! index itself.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use cratarr_rules::index::{self, Kind};

use super::ToggleError;
use super::changes::IndexStands;
use super::library::Library;
use crate::serve::Serve;
use crate::settings::LibraryKind;

impl Library {
    /// Every catalogue folder as a device names it: its library where one holds it, else its path, so a server's library not
    /// answering for now does not take its rows off every device. By its id, a folder's, `order` saying the libraries' order. Blocking.
    pub(super) fn index_libraries(&self, serve: &Serve) -> Vec<index::Library> {
        let (kinds, live, folders) = (self.folder_kinds(), self.live(), self.catalogue().folders());
        let of = |path: &Path| {
            live.iter().find_map(|a| {
                let placed = a.placement.libraries.iter().find(|p| p.folders.iter().any(|f| Path::new(f) == path))?;
                Some((a, placed, a.libraries().into_iter().find(|u| u.id == placed.id)))
            })
        };
        let mut rows: Vec<index::Library> = folders
            .iter()
            .enumerate()
            .map(|(n, (folder, path))| {
                let at = kinds.iter().position(|(f, _)| *f == folder);
                let order = at.unwrap_or(kinds.len() + n) as u32;
                let kind = match at.map_or(LibraryKind::Incoming, |at| kinds[at].1) {
                    LibraryKind::Main => Kind::Main,
                    LibraryKind::Own => Kind::Own,
                    LibraryKind::Incoming => Kind::Incoming,
                };
                let (key, name) = (path.display().to_string(), path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned()));
                match of(path.as_path()) {
                    Some((a, placed, used)) => {
                        let owner = used.and_then(|u| u.owner).filter(|o| serve.users.get(o).is_some_and(|u| u.active));
                        index::Library { id: folder, key: placed.id.clone(), name: placed.name.clone(), kind, order, colour: a.colour_of(&placed.id), owner }
                    }
                    None => index::Library { id: folder, key, name, kind, order, colour: None, owner: None },
                }
            })
            .collect();
        rows.sort_by_key(|l| l.id);
        rows
    }
}

/// Where the log stands for a device at `since`, nothing built: whether it has anything to ask for. Blocking.
pub(super) fn stands_for(serve: &Serve, libraries: &[index::Library], since: Option<i64>) -> Result<IndexStands> {
    let turn = serve.device_turn();
    let cat = serve.catalogue();
    let (rev, floor) = (cat.index_rev()?, cat.index_floor()?);
    let Some(since) = since.filter(|s| *s >= floor && *s <= rev) else { return Ok(IndexStands { rev, changed: true, reset: Some(true) }) };
    let facts = cat.crate_facts()?;
    Ok(IndexStands { rev, changed: cat.index_moved(since, rev)? || !turn.stands(&facts, serve.built_at(), libraries), reset: None })
}

/// `/api/changes`' word on the index, for a device at `since`.
pub async fn stands(library: &Library, serve: Arc<Serve>, since: Option<i64>) -> Result<IndexStands, ToggleError> {
    let libraries = library.index_libraries(&serve);
    let found = crate::run::spawn_blocking(move || stands_for(&serve, &libraries, since));
    Ok(found.await.map_err(|e| anyhow!("{e}"))??)
}
