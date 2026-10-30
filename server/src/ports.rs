//! Ports (decision 0011): the outside things a feature acts on, as traits, so the services can be tested with fakes.
//! The media server's port is `source::Source` (decision 0005); the playlist files are `playlists::Store`.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use crate::source::Fut;

/// Files on disk. Blocking: call from `spawn_blocking`.
pub trait Files: Send + Sync {
    /// Move `rel` (an album folder or file) into the trash inside `folder`: a rename on the same file system, never a
    /// copy. Returns the token that restores it.
    fn trash(&self, folder: &Path, rel: &str, now_epoch: i64) -> Result<String>;
    /// Put a trashed item back where it was. Returns where that is.
    fn restore(&self, folder: &Path, token: &str) -> Result<String>;
    /// Empty what has been in the trash inside `folder` longer than `keep_days`. Returns how many went.
    fn purge(&self, folder: &Path, now_epoch: i64, keep_days: u32) -> Result<usize>;
    /// Whether Cratarr can create and remove a file in `folder` (decision 0010).
    fn can_write(&self, folder: &Path) -> bool;
    /// Move files between Cratarr's folders, all or none, nothing overwritten (Keep and Replace, decision 0013).
    fn move_all(&self, plan: &[crate::moves::Step]) -> Result<()>;
    /// Undo `move_all`.
    fn move_back(&self, plan: &[crate::moves::Step]) -> Result<()>;
}

/// The disk itself.
pub struct Disk;

impl Files for Disk {
    fn trash(&self, folder: &Path, rel: &str, now_epoch: i64) -> Result<String> {
        crate::trash::trash(folder, rel, now_epoch)
    }

    fn restore(&self, folder: &Path, token: &str) -> Result<String> {
        crate::trash::restore(folder, token)
    }

    fn purge(&self, folder: &Path, now_epoch: i64, keep_days: u32) -> Result<usize> {
        crate::trash::purge(folder, now_epoch, keep_days)
    }

    fn can_write(&self, folder: &Path) -> bool {
        crate::mounts::writable(folder)
    }

    fn move_all(&self, plan: &[crate::moves::Step]) -> Result<()> {
        crate::moves::apply(plan)
    }

    fn move_back(&self, plan: &[crate::moves::Step]) -> Result<()> {
        crate::moves::revert(plan)
    }
}

/// Where new music comes from (Lidarr). Best effort: an action Cratarr takes stands when this fails. A read and a
/// write, so a caller can record what it will change before changing it (plan, then act: a retry after a crash
/// re-applies the recorded plan instead of reading a state it already changed).
pub trait Acquisition: Send + Sync {
    /// What is being fetched now for this MusicBrainz release group (empty when nothing is).
    fn fetching<'a>(&'a self, release_group: &'a str) -> Fut<'a, Vec<i64>>;
    /// Fetch, or stop fetching, these. Safe to repeat.
    fn set_fetching<'a>(&'a self, ids: &'a [i64], on: bool) -> Fut<'a, ()>;
}

impl Acquisition for crate::lidarr::Lidarr {
    fn fetching<'a>(&'a self, release_group: &'a str) -> Fut<'a, Vec<i64>> {
        Box::pin(self.monitored(release_group))
    }

    fn set_fetching<'a>(&'a self, ids: &'a [i64], on: bool) -> Fut<'a, ()> {
        Box::pin(self.set_monitored(ids, on))
    }
}

/// What carries out an approved request (decision 0070): Lidarr first; slskd directly, say, could take its place without
/// the requests, their queue or their statuses changing. An album is named by its MusicBrainz release group where known.
pub trait Fetcher: Send + Sync {
    /// Fetch this album into Incoming. What was done, in words.
    fn want<'a>(&'a self, release_group: Option<&'a str>, artist: &'a str, album: &'a str) -> Fut<'a, String>;
    /// How far it is with each of these albums, all asked at once (the Requests page shows many).
    fn statuses<'a>(&'a self, release_groups: &'a [String]) -> Fut<'a, std::collections::HashMap<String, crate::lidarr::Progress>>;
    /// Stop fetching it (what is there already stays).
    fn cancel<'a>(&'a self, release_group: &'a str) -> Fut<'a, ()>;
}

/// Lidarr as the fetcher: Want with the profiles and root folder chosen on its page in Connections.
pub struct LidarrFetcher {
    pub lidarr: crate::lidarr::Lidarr,
    pub with: crate::lidarr::AddWith,
}

impl Fetcher for LidarrFetcher {
    fn want<'a>(&'a self, release_group: Option<&'a str>, artist: &'a str, album: &'a str) -> Fut<'a, String> {
        Box::pin(self.lidarr.want(&self.with, release_group, artist, album))
    }

    fn statuses<'a>(&'a self, release_groups: &'a [String]) -> Fut<'a, std::collections::HashMap<String, crate::lidarr::Progress>> {
        Box::pin(self.lidarr.progress(release_groups))
    }

    fn cancel<'a>(&'a self, release_group: &'a str) -> Fut<'a, ()> {
        Box::pin(self.lidarr.unwant(release_group))
    }
}

/// The real adapters, as the services hold them.
pub fn disk() -> Arc<dyn Files> {
    Arc::new(Disk)
}
