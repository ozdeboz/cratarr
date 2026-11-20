//! An import's job and the sources it reads, as the server holds them (`Serve::import`), below the app (moved from
//! `serve::import`, which keeps their names and runs the imports).

use std::sync::{Arc, Mutex, RwLock};

use serde::{Deserialize, Serialize};

use crate::source::Source;

/// The sources in use, by their id in the settings (`app::Library::connected_sources`).
pub type Sources = Arc<dyn Fn() -> Vec<(String, Arc<dyn Source>)> + Send + Sync>;

/// What to import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct What {
    #[serde(default)]
    pub plays: bool,
    #[serde(default)]
    pub favorites: bool,
    #[serde(default)]
    pub ratings: bool,
    #[serde(default)]
    pub added: bool,
}

impl Default for What {
    fn default() -> Self {
        Self { plays: true, favorites: true, ratings: true, added: true }
    }
}

/// An import as the page asks for it: from which source and account on it, into which Cratarr account. A source id
/// `subsonic:<address>` is a Subsonic server imported from without being one of Cratarr's sources (decision 0067),
/// logged in to as `user` with `password` (given for each run, never kept or shown).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub source: String,
    /// The account on the server, by its id there.
    pub user: String,
    /// The Cratarr (Subsonic) account.
    pub account: String,
    pub what: What,
    /// Count what would change, change nothing.
    #[serde(default)]
    pub preview: bool,
    #[serde(default, skip_serializing)]
    pub password: Option<String>,
}

/// What an import did (or, as a preview, would do).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// Plays added, over this many songs.
    pub plays: u64,
    pub songs: u64,
    pub starred: u64,
    pub unstarred: u64,
    pub rated: u64,
    /// Albums whose added date was set or moved earlier.
    pub added: u64,
    /// Tracks, albums and artists the library does not have.
    pub skipped: u64,
}

impl Counts {
    pub fn add(&mut self, o: &Counts) {
        self.plays += o.plays;
        self.songs += o.songs;
        self.starred += o.starred;
        self.unstarred += o.unstarred;
        self.rated += o.rated;
        self.added += o.added;
        self.skipped += o.skipped;
    }
}

/// Where an import is: `tracks`, `groups` (favourite albums and artists) or `added`, and how far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Step {
    pub name: &'static str,
    pub read: usize,
    pub total: Option<usize>,
}

/// The import running now, or the last one.
#[derive(Debug, Clone, Serialize)]
pub struct Job {
    #[serde(flatten)]
    pub request: Request,
    pub started: i64,
    pub finished: Option<i64>,
    pub step: Option<Step>,
    pub counts: Counts,
    pub error: Option<String>,
    /// Something left undone on purpose (see `UNSTAR_SHARE`).
    pub note: Option<String>,
}

#[derive(Default)]
pub struct Importer {
    sources: RwLock<Option<Sources>>,
    job: Mutex<Option<Job>>,
}

impl Importer {
    /// The sources to import from; until this is called there are none.
    pub fn connect(&self, sources: Sources) {
        *self.sources.write().unwrap_or_else(|p| p.into_inner()) = Some(sources);
    }

    /// The sources in use that can import.
    pub fn sources(&self) -> Vec<(String, Arc<dyn Source>)> {
        let f = self.sources.read().unwrap_or_else(|p| p.into_inner()).clone();
        f.map(|f| f()).unwrap_or_default().into_iter().filter(|(_, s)| s.imports()).collect()
    }

    pub fn job(&self) -> Option<Job> {
        self.job.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn update(&self, f: impl FnOnce(&mut Job)) {
        if let Some(j) = self.job.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
            f(j);
        }
    }

    /// Marks an import as running; refused while another runs.
    pub fn begin(&self, request: Request) -> Result<Job, &'static str> {
        let mut job = self.job.lock().unwrap_or_else(|p| p.into_inner());
        if job.as_ref().is_some_and(|j| j.finished.is_none()) {
            return Err("An import is running.");
        }
        let j = Job { request, started: super::now(), finished: None, step: None, counts: Counts::default(), error: None, note: None };
        *job = Some(j.clone());
        Ok(j)
    }
}
