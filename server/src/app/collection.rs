//! Collection: the albums of each server's Music library, for the Collection page. Read-only; the lists are kept and
//! refreshed in the background (see `refresh`), as Incoming's are.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;

use super::library::{Active, Library, epoch_now, label_of};
use super::shelf::{Shelf, ShelfRead, Shelves};
use crate::source::{self, Source};

/// More than any music library here holds; the page shows them all.
const COLLECTION_LIMIT: usize = 100_000;

/// One server's Music library, newest first (the page orders it as the viewer chooses).
#[derive(Debug, Clone, Serialize)]
pub struct CollectionSourceView {
    pub id: String,
    pub label: String,
    /// False when the server cannot list albums at all.
    pub supported: bool,
    pub albums: Vec<source::Album>,
    pub error: Option<String>,
    /// When this list was read from the server, epoch seconds.
    pub as_of: i64,
    /// The libraries it was read from (the filter's library switches).
    pub libraries: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CollectionView {
    pub sources: Vec<CollectionSourceView>,
}

pub struct CollectionService {
    lib: Arc<Library>,
    shelves: Arc<Shelves>,
    /// The lists by server and libraries (see `refresh`).
    cache: Mutex<HashMap<String, CollectionSourceView>>,
    /// A server's list changed (albums came, went or changed), for pages that follow it (`/api/events`).
    changed: tokio::sync::broadcast::Sender<()>,
    /// Held while the lists are read: reading a library takes the server half a minute, so a page asking meanwhile
    /// waits for that read instead of starting its own.
    reading: Mutex<()>,
}

impl CollectionService {
    pub(super) fn new(lib: Arc<Library>, shelves: Arc<Shelves>) -> Self {
        Self { lib, shelves, cache: Mutex::new(HashMap::new()), changed: tokio::sync::broadcast::channel(16).0, reading: Mutex::new(()) }
    }

    /// Told when a refresh finds a server's list changed.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<()> {
        self.changed.subscribe()
    }

    /// The pages showing the albums told to read them again: what is said of them changed without a read (0.10, a
    /// record marked classical or not).
    pub fn tell_changed(&self) {
        let _ = self.changed.send(());
    }

    /// Every server with a Music library (or only `want`), its albums newest first. The kept lists, read from the
    /// server only when there is none yet (after a start, the first background pass is usually reading it already).
    pub async fn list(&self, want: Option<&str>) -> CollectionView {
        let mut sources = Vec::new();
        for (a, s, libraries) in self.music_sources() {
            if want.is_some_and(|w| w != a.cfg.id) {
                continue;
            }
            let key = key(&a, &libraries);
            let kept = self.cache.lock().await.get(&key).cloned();
            let view = match kept {
                Some(v) => v,
                None => {
                    let _reading = self.reading.lock().await;
                    let kept = self.cache.lock().await.get(&key).cloned();
                    match kept {
                        Some(v) => v,
                        None => {
                            let v = self.read(&a, &*s, &libraries).await;
                            self.cache.lock().await.insert(key, v.clone());
                            v
                        }
                    }
                }
            };
            sources.push(view);
        }
        CollectionView { sources }
    }

    /// Reads the lists again, keeping a list whose read failed. Tells the pages when one changed.
    /// Read the lists again a minute after an album action (Keep, Replace, Delete, an Undo): the server adds a kept
    /// album within seconds of being told, so it shows without waiting for the quarter-hour read.
    pub fn follow(self: &Arc<Self>, journal: &crate::journal::Journal) {
        use tokio::sync::broadcast::error::RecvError;
        let (mut done, mut undone) = (journal.subscribe(), journal.subscribe_undone());
        let me = self.clone();
        tokio::spawn(async move {
            loop {
                let heard = tokio::select! {
                    r = done.recv() => r,
                    r = undone.recv() => r,
                };
                if let Err(RecvError::Closed) = heard {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                // What came meanwhile is in this read.
                done = done.resubscribe();
                undone = undone.resubscribe();
                me.refresh().await;
            }
        });
    }

    pub async fn refresh(&self) {
        let _reading = self.reading.lock().await;
        let mut keep = HashSet::new();
        let mut changed = false;
        for (a, s, libraries) in self.music_sources() {
            let key = key(&a, &libraries);
            let fresh = self.read(&a, &*s, &libraries).await;
            let mut cache = self.cache.lock().await;
            if fresh.error.is_none() || !cache.contains_key(&key) {
                changed |= cache.get(&key).is_none_or(|old| old.albums != fresh.albums);
                cache.insert(key.clone(), fresh);
            }
            keep.insert(key);
        }
        let mut cache = self.cache.lock().await;
        let before = cache.len();
        cache.retain(|k, _| keep.contains(k));
        changed |= cache.len() != before;
        if changed {
            let _ = self.changed.send(());
        }
    }

    /// Each connected server with a Music library, and its library ids.
    fn music_sources(&self) -> Vec<(Arc<Active>, Arc<dyn Source>, Vec<String>)> {
        self.lib
            .connected()
            .into_iter()
            .filter(|(a, _)| !a.cfg.libraries.music.is_empty())
            .map(|(a, s)| {
                let ids = a.cfg.libraries.music.clone();
                (a, s, ids)
            })
            .collect()
    }

    async fn read(&self, a: &Active, s: &dyn Source, libraries: &[String]) -> CollectionSourceView {
        let ShelfRead { supported, albums, error } = self.shelves.read(a, s, libraries, Shelf::Music, COLLECTION_LIMIT).await;
        CollectionSourceView { id: a.cfg.id.clone(), label: label_of(&a.cfg), supported, albums, error, as_of: epoch_now(), libraries: libraries.to_vec() }
    }
}

fn key(a: &Active, libraries: &[String]) -> String {
    format!("{}\n{}", a.cfg.id, libraries.join("\n"))
}
